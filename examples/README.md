## Examples of how to use the Reth SDK

This directory contains a number of examples showcasing various capabilities of
the `reth-*` crates.

All examples can be executed with:

```
cargo run --example $name
```

A good starting point for the examples would be [`db-access`](db-access.rs)
and [`rpc-db`](rpc-db).

If you've got an example you'd like to see here, please feel free to open an
issue. Otherwise if you've got an example you'd like to add, please feel free
to make a PR!

### log-lookup

This tool returns logs from specific blocks, optionally with the transaction index as well (though that seems to incur no speedup).
That's the output of a `chifra list` command from [TrueBlocks](https://trueblocks.io). I remove the address from appearances csv's for easier loading:
```sh
chifra list 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 --fmt csv | cut -d, -f2,3 > steth.csv
chifra list 0x442af784A788A5bd6F42A01Ebe9F287a871243fb --fmt csv | cut -d, -f2,3 > steth_legacy_oracle.csv
chifra list 0x852ded011285fe67063a08005c71a85690503cee --fmt csv | cut -d, -f2,3 > steth_accounting_oracle.csv
```

Build `log-lookup` to save ~1s each time, or run it with `cargo run --example=log-lookup --package=examples --message-format=json` instead.
```sh
RUSTFLAGS="-C target-cpu=native" cargo build --profile maxperf --example log-lookup --package examples
```

Returning all logs from the steth legacy oracle:
```sh
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_legacy_oracle.csv 0x442af784a788a5bd6f42a01ebe9f287a871243fb > results.csv
1.69s user 0.40s system 337% cpu 0.619 total
```

Returning just the PostTotalShares logs takes the same amount of time, even though there are 6x fewer of them (1011 logs in 9489 blocks vs. 5923 logs without the topic filter).
I believe there's no way to get a log from a block without returning all receipts first, then filtering.
```sh
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_legacy_oracle.csv 0x442af784a788a5bd6f42a01ebe9f287a871243fb 0xdafd48d1eba2a416b2aca45e9ead3ad18b84e868fa6d2e1a3048bfd37ed10a32 > results.csv
1.60s user 0.47s system 332% cpu 0.622 total
```

I can't tell if this uses caching or not (db-access.rs says none of these functions use caching), because sometimes I get a much slower result:
```sh
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_legacy_oracle.csv 0x442af784a788a5bd6f42a01ebe9f287a871243fb 0xdafd48d1eba2a416b2aca45e9ead3ad18b84e868fa6d2e1a3048bfd37ed10a32 > results.csv
1.92s user 1.72s system 50% cpu 7.185 total
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_legacy_oracle.csv 0x442af784a788a5bd6f42a01ebe9f287a871243fb 0xdafd48d1eba2a416b2aca45e9ead3ad18b84e868fa6d2e1a3048bfd37ed10a32 > results.csv
1.52s user 0.47s system 326% cpu 0.612 total
```

Similar results querying the lido v2 oracle, with or without the filter for TokenRebased (268 logs in 2211 blocks vs. 558 without a topic filter.)
```sh
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_accounting_oracle.csv 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 > results.csv
steth_accounting_oracle.csv 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 > results.csv
0.33s user 0.08s system 319% cpu 0.130 total
time RAYON_NUM_THREADS=4 target/maxperf/examples/log-lookup steth_accounting_oracle.csv 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 0xff08c3ef606d198e316ef5b822193c489965899eb4e3c248cea1a4626c3eda50 > results.csv
0.32s user 0.11s system 316% cpu 0.136 total
```

This fails parsing a larger query, probably since I've never written Rust before:
```sh
chifra list 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 --fmt csv | cut -d, -f2,3 > steth.csv
target/maxperf/examples/log-lookup steth.csv 0xae7ab96520de3a18e5e111b5eaab095312d7fe84 0xff08c3ef606d198e316ef5b822193c489965899eb4e3c248cea1a4626c3eda50
thread '<unnamed>' panicked at examples/log-lookup.rs:152:66:
called `Result::unwrap()` on an `Err` value: Database(Read(DatabaseErrorInfo { message: "wrong signature of a runtime object(s)", code: -30420 }))
```

For reference, PostTotalShares and TokenRebased queries take 41.28 seconds and 69.69 (nice!) seconds accessing my local reth node through [Ape](https://github.com/ApeWorX/ape).

See the flow of one of these oracle updates here: https://etherscan.io/tx/0xe54e20c06303a975264af1b0c0b48f5dd9d810e25a0c911acaa3fe51dd8ae80d#eventlog
