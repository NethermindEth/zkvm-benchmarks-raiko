//! An implementation of a type-1, bytecompatible compatible, zkEVM written in Rust & SP1.
//!
//! The flow for the guest program is based on Zeth and rsp.
//!
//! Reference: https://github.com/risc0/zeth
//!            https://github.com/succinctlabs/rsp

#![no_main]
risc0_zkvm::guest::entry!(main);

use reth_client::{io::ClientExecutorInput, ClientExecutor};

use risc0_zkvm::guest::env;

fn main() {
    // Read the input.
    let mut input: ClientExecutorInput = env::read();

    // Execute the block.
    let executor = ClientExecutor;
    let header = executor.execute(input).unwrap();
    let block_hash = header.hash_slow();

    env::commit(&block_hash);
}
