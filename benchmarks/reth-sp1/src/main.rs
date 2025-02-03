//! An implementation of a type-1, bytecompatible compatible, zkEVM written in Rust & SP1.
//!
//! The flow for the guest program is based on Zeth and rsp.
//!
//! Reference: https://github.com/risc0/zeth
//!            https://github.com/succinctlabs/rsp

#![no_main]
sp1_zkvm::entrypoint!(main);

use rsp_client_executor::{io::ClientExecutorInput, ChainVariant, ClientExecutor};

use sp1_zkvm::io::read;

fn main() {
    // Read the input.
    let input: Vec<u8> = read();
    let input = bincode::deserialize::<ClientExecutorInput>(&input).unwrap();

    // Execute the block.
    let chain = ChainVariant::mainnet();
    let header = ClientExecutor.execute(input, &chain).unwrap();
    let block_hash = header.hash_slow();

    println!("block_hash: {:?}", block_hash);
}
