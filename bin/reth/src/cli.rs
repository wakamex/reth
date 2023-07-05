//! CLI definition and entrypoint to executable
use crate::{
    chain, config, db, debug_cmd,
    dirs::{LogsDir, PlatformPath},
    node, p2p,
    runner::CliRunner,
    stage, test_vectors,
    version::{LONG_VERSION, SHORT_VERSION},
};
use clap::{ArgAction, Args, Parser, Subcommand};
use reth_tracing::{
    tracing::{metadata::LevelFilter, Level, Subscriber},
    tracing_subscriber::{filter::Directive, registry::LookupSpan, EnvFilter, layer::Layer},
    BoxedLayer, FileWorkerGuard,
    tracing::Event,
};
use std::time::Instant;
use reth_stages::StageSet;

#[derive(Debug)]
struct TimingLayer<L> {
    inner: L,
}

impl<L> TimingLayer<L> {
    fn new(inner: L) -> Self {
        TimingLayer { inner }
    }
}

impl<S, L> Layer<S> for TimingLayer<L>
where
    S: LookupSpan<'static> + tracing::Subscriber,
    L: Layer<S>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: reth_tracing::tracing_subscriber::layer::Context<'_, S>) {
        let start_time = Instant::now(); // Start measuring time

        self.inner.on_event(event, _ctx);

        let elapsed_time = start_time.elapsed(); // Calculate elapsed time
        println!("Time spent logging: {:?}", elapsed_time); // Print elapsed time
    }

    // Implement other required methods...
}

/// Parse CLI options, set up logging and run the chosen command.
pub fn run() -> eyre::Result<()> {
    let opt = Cli::parse();

    let mut layers = vec![reth_tracing::stdout(opt.verbosity.directive())];
    let _guard = opt.logs.layer()?.map(|(layer, guard)| {
        layers.push(layer);
        guard
    });

    reth_tracing::init(layers);

    let runner = CliRunner::default();

    match opt.command {
        Commands::Node(command) => runner.run_command_until_exit(|ctx| command.execute(ctx)),
        Commands::Init(command) => runner.run_blocking_until_ctrl_c(command.execute()),
        Commands::Import(command) => runner.run_blocking_until_ctrl_c(command.execute()),
        Commands::Db(command) => runner.run_blocking_until_ctrl_c(command.execute()),
        Commands::Stage(command) => runner.run_blocking_until_ctrl_c(command.execute()),
        Commands::P2P(command) => runner.run_until_ctrl_c(command.execute()),
        Commands::TestVectors(command) => runner.run_until_ctrl_c(command.execute()),
        Commands::Config(command) => runner.run_until_ctrl_c(command.execute()),
        Commands::Debug(command) => runner.run_command_until_exit(|ctx| command.execute(ctx)),
    }
}

/// Commands to be executed
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the node
    #[command(name = "node")]
    Node(node::Command),
    /// Initialize the database from a genesis file.
    #[command(name = "init")]
    Init(chain::InitCommand),
    /// This syncs RLP encoded blocks from a file.
    #[command(name = "import")]
    Import(chain::ImportCommand),
    /// Database debugging utilities
    #[command(name = "db")]
    Db(db::Command),
    /// Manipulate individual stages.
    #[command(name = "stage")]
    Stage(stage::Command),
    /// P2P Debugging utilities
    #[command(name = "p2p")]
    P2P(p2p::Command),
    /// Generate Test Vectors
    #[command(name = "test-vectors")]
    TestVectors(test_vectors::Command),
    /// Write config to stdout
    #[command(name = "config")]
    Config(config::Command),
    /// Various debug routines
    #[command(name = "debug")]
    Debug(debug_cmd::Command),
}

#[derive(Debug, Parser)]
#[command(author, version = SHORT_VERSION, long_version = LONG_VERSION, about = "Reth", long_about = None)]
struct Cli {
    /// The command to run
    #[clap(subcommand)]
    command: Commands,

    #[clap(flatten)]
    logs: Logs,

    #[clap(flatten)]
    verbosity: Verbosity,
}

/// The log configuration.
#[derive(Debug, Args)]
#[command(next_help_heading = "Logging")]
pub struct Logs {
    /// The flag to enable persistent logs.
    #[arg(long = "log.persistent", global = true, conflicts_with = "journald")]
    persistent: bool,

    /// The path to put log files in.
    #[arg(
        long = "log.directory",
        value_name = "PATH",
        global = true,
        default_value_t,
        conflicts_with = "journald"
    )]
    log_directory: PlatformPath<LogsDir>,

    /// Log events to journald.
    #[arg(long = "log.journald", global = true, conflicts_with = "log_directory")]
    journald: bool,

    /// The filter to use for logs written to the log file.
    #[arg(long = "log.filter", value_name = "FILTER", global = true, default_value = "error")]
    filter: String,
}

impl<DB> Logs<DB> // Add the constraint for the `DB` type parameter
where
    DB: StageSet<DB>, // Add the constraint for the `DB` type parameter
{
    /// Builds a tracing layer from the current log options.
    pub fn layer<S>(&self) -> eyre::Result<Option<(BoxedLayer<S>, Option<FileWorkerGuard>)>>
    where
        S: Subscriber + StageSet<DB>, // Add the StageSet trait as a bound
        for<'a> S: LookupSpan<'a>,
    {
        let filter = EnvFilter::builder().parse(&self.filter)?;

        let subscriber = S::builder()
            .with_env_filter(filter.clone()) // Add the environment filter to the subscriber
            .with(TimingLayer::new(filter)) // Add the timing layer to the subscriber
            .try_init();

        if self.journald {
            let layer = reth_tracing::journald(filter).expect("Could not connect to journald");
            Ok(Some((Box::new(layer), None)))
        } else if self.persistent {
            let (layer, guard) = reth_tracing::file(filter, &self.log_directory, "reth.log");
            Ok(Some((Box::new(layer), Some(guard))))
        } else {
            Ok(None)
        }
    }
}

/// The verbosity settings for the cli.
#[derive(Debug, Copy, Clone, Args)]
#[command(next_help_heading = "Display")]
pub struct Verbosity {
    /// Set the minimum log level.
    ///
    /// -v      Errors
    /// -vv     Warnings
    /// -vvv    Info
    /// -vvvv   Debug
    /// -vvvvv  Traces (warning: very verbose!)
    #[clap(short, long, action = ArgAction::Count, global = true, default_value_t = 3, verbatim_doc_comment, help_heading = "Display")]
    verbosity: u8,

    /// Silence all log output.
    #[clap(long, alias = "silent", short = 'q', global = true, help_heading = "Display")]
    quiet: bool,
}

impl Verbosity {
    /// Get the corresponding [Directive] for the given verbosity, or none if the verbosity
    /// corresponds to silent.
    pub fn directive(&self) -> Directive {
        if self.quiet {
            LevelFilter::OFF.into()
        } else {
            let level = match self.verbosity - 1 {
                0 => Level::ERROR,
                1 => Level::WARN,
                2 => Level::INFO,
                3 => Level::DEBUG,
                _ => Level::TRACE,
            };

            format!("{level}").parse().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Tests that the help message is parsed correctly. This ensures that clap args are configured
    /// correctly and no conflicts are introduced via attributes that would result in a panic at
    /// runtime
    #[test]
    fn test_parse_help_all_subcommands() {
        let reth = Cli::command();
        for sub_command in reth.get_subcommands() {
            let err = Cli::try_parse_from(["reth", sub_command.get_name(), "--help"])
                .err()
                .unwrap_or_else(|| {
                    panic!("Failed to parse help message {}", sub_command.get_name())
                });

            // --help is treated as error, but
            // > Not a true "error" as it means --help or similar was used. The help message will be sent to stdout.
            assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        }
    }
}
