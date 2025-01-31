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
    let input: Vec<u8> = env::read();
    let input: ClientExecutorInput = serde_json::from_slice(&input).unwrap();

    // Execute the block.
    let header = ClientExecutor.execute(input).unwrap();
    let block_hash = header.hash_slow();

    println!("block_hash: {:?}", block_hash);
}
