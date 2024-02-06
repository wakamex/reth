use reth_db::open_db_read_only;
use reth_primitives::{Address, ChainSpecBuilder, B256};
use reth_provider::{HeaderProvider, ProviderFactory, ReceiptProvider};
use reth_rpc_types::{Filter, FilteredParams, BloomFilter};
use reth_rpc_types_compat::log::from_primitive_log;
use std::{path::Path, str::FromStr};
use serde::Deserialize;
use std::fs::File;
use csv::Reader;
use std::sync::Arc;
use reth_db::DatabaseEnv;
use reth_provider::DatabaseProviderRO;
use rayon::prelude::*;
use std::error::Error;
use std::env::{self, args};

// Providers are zero cost abstractions on top of an opened MDBX Transaction
// exposing a familiar API to query the chain's information without requiring knowledge
// of the inner tables.
//
// These abstractions do not include any caching and the user is responsible for doing that.
// Other parts of the code which include caching are parts of the `EthApi` abstraction.
#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Opens a RO handle to the database file.
    // TODO: Should be able to do `ProviderFactory::new_with_db_path_ro(...)` instead of
    //  doing in 2 steps.
    let db = open_db_read_only(Path::new("/nvme/reth_mainnet/db/mdbx.dat"), Default::default())?;

    // Instantiate a provider factory for Ethereum mainnet using the provided DB.
    // TODO: Should the DB version include the spec so that you do not need to specify it here?
    let spec = ChainSpecBuilder::mainnet().build();
    let factory = ProviderFactory::new(db, spec.into());

    // This call opens a RO transaction on the database. To write to the DB you'd need to call
    // the `provider_rw` function and look for the `Writer` variants of the traits.
    let provider = Arc::new(factory.provider()?);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {println!("Must provide appearances file as the first argument.");}
    if args.len() < 3 {println!("Must provide address as the second argument.");}
    if args.len() > 4 {println!("Provided too many arguments.");}
    if args.len() == 3 {let _ = get_appearances(provider, args[1].as_str(), args[2].as_str());}
    else if args.len() == 4 {let _ = get_appearances_with_event(provider, args[1].as_str(), args[2].as_str(), args[3].as_str());}

    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct Transaction {
    blockNumber: u64,
    transactionIndex: u32,
}

fn get_appearances(
    provider: Arc<DatabaseProviderRO<DatabaseEnv>>,file_name: &str, address: &str
) -> Result<(), Box<dyn Error>> {
    let file = File::open(file_name)?;
    let mut rdr = Reader::from_reader(file);
    let transactions: Vec<Transaction> = rdr.deserialize()
        .collect::<Result<_, csv::Error>>()?;
    println!("Loaded {} transactions", transactions.len());

    let addr = Address::from_str(address).unwrap();
    let filter = Filter::new().address(addr);
    let filter_params = FilteredParams::new(Some(filter));
    let address_filter = FilteredParams::address_filter(&addr.into());

    // Use Rayon to process each transaction in parallel
    transactions.par_iter().for_each(|transaction| {
        let provider = Arc::clone(&provider);
        process_transaction(provider, transaction.blockNumber, &filter_params, &address_filter).expect("Failed to process transaction");
    });
    Ok(())
}

fn get_appearances_with_event(
    provider: Arc<DatabaseProviderRO<DatabaseEnv>>,file_name: &str, address: &str, event_signature: &str
) -> Result<(), Box<dyn Error>> {
    let file = File::open(file_name)?;
    let mut rdr = Reader::from_reader(file);
    let transactions: Vec<Transaction> = rdr.deserialize()
        .collect::<Result<_, csv::Error>>()?;
    println!("Loaded {} transactions", transactions.len());

    let addr = Address::from_str(address).unwrap();
    let topic = B256::from_str(event_signature).unwrap();

    let filter = Filter::new().address(addr).event_signature(topic);
    let filter_params = FilteredParams::new(Some(filter));
    let address_filter = FilteredParams::address_filter(&addr.into());
    let topics_filter = FilteredParams::topics_filter(&[topic.into()]);

    // Use Rayon to process each transaction in parallel
    transactions.par_iter().for_each(|transaction| {
        let provider = Arc::clone(&provider);
        process_transaction_known_index(provider, transaction.blockNumber, transaction.transactionIndex, &filter_params, &address_filter, &topics_filter).expect("Failed to process transaction");
    });
    Ok(())
}

fn process_transaction(
    provider: Arc<DatabaseProviderRO<DatabaseEnv>>,
    block_num: u64,
    filter_params: &FilteredParams,
    address_filter: &BloomFilter,
) -> Result<(), eyre::Error> {
    let _receipts = provider.receipts_by_block(block_num.into()).unwrap().unwrap();
    let header = provider.header_by_number(block_num).unwrap();
    let bloom = header.unwrap().logs_bloom;

    if FilteredParams::matches_address(bloom, &address_filter)
    {
        for _receipt in &_receipts {
            for log in &_receipt.logs {
                let log = from_primitive_log(log.clone());
                if filter_params.filter_address(&log) && filter_params.filter_topics(&log) {
                    println!("{log:?}")
                }
            }
        }
    }
    Ok(())
}

fn process_transaction_known_index(
    provider: Arc<DatabaseProviderRO<DatabaseEnv>>,
    block_num: u64,
    txn_num: u32,
    filter_params: &FilteredParams,
    address_filter: &BloomFilter,
    topics_filter: &[BloomFilter],
) -> Result<(), eyre::Error> {
    let _receipts = provider.receipts_by_block(block_num.into()).unwrap().unwrap();
    let header = provider.header_by_number(block_num).unwrap();
    let bloom = header.unwrap().logs_bloom;

    if FilteredParams::matches_address(bloom, &address_filter) &&
        FilteredParams::matches_topics(bloom, &topics_filter)
    {
        let _receipt = &_receipts[txn_num as usize];
        for log in &_receipt.logs {
            let log = from_primitive_log(log.clone());
            if filter_params.filter_address(&log) && filter_params.filter_topics(&log) {
                println!("{log:?}")
            }
        }
    }
    Ok(())
}