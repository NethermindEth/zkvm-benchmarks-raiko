//! An implementation of a type-1, bytecompatible compatible, zkEVM written in Rust & SP1.
//!
//! The flow for the guest program is based on Zeth and rsp.
//!
//! Reference: https://github.com/risc0/zeth
//!            https://github.com/succinctlabs/rsp

#![no_main]
sp1_zkvm::entrypoint!(main);

use rsp_client_executor::{io::ClientExecutorInput, ClientExecutor, EthereumVariant};

fn main() {
    // Read the input.
    let input: Vec<u8> = sp1_zkvm::io::read_vec();
    let input = bincode::deserialize::<ClientExecutorInput>(&input).unwrap();

    // let input: Vec<u8> = read();
    // let input = bincode::deserialize::<ClientExecutorInput>(&input).unwrap();

    // Execute the block.
    let header = ClientExecutor
        .execute::<EthereumVariant>(input)
        .expect("failed to execute client");
    let block_hash = header.hash_slow();

    println!("block_hash: {:?}", block_hash);
}
