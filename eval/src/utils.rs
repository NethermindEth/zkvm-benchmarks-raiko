use std::{
    env,
    time::{Duration, Instant},
};

use alloy_provider::ReqwestProvider;
use block_downloader::HostExecutor;
use reth_client::io::ClientExecutorInput;

use crate::{
    types::{ProgramId, ProverId},
    EvalArgs,
};

pub fn get_elf(args: &EvalArgs) -> String {
    let mut program_dir = args.program.to_string();
    if args.program == ProgramId::Reth {
        program_dir += "-";
        program_dir += args.prover.to_string().as_str();
    }

    let current_dir = env::current_dir().expect("Failed to get current working directory");

    let elf_path = match args.prover {
        ProverId::Risc0 => current_dir.join(format!(
            "benchmarks/{}/target/riscv-guest/methods/{}/riscv32im-risc0-zkvm-elf/release/{}",
            program_dir, program_dir, program_dir
        )),
        _ => panic!("prover not supported"),
    };

    let elf_path_str = elf_path
        .to_str()
        .expect("Failed to convert path to string")
        .to_string();
    println!("elf path: {}", elf_path_str);
    elf_path_str
}

pub async fn get_reth_input(args: &EvalArgs) -> ClientExecutorInput {
    if let Some(block_number) = args.block_number {
        // There is a dependency mismatch that does not allow
        // `ClientExecutorInput` to be fetched from the file. Download the block
        // from the node
        let rpc_url = args.rpc_url.clone().expect("RPC URL is required");

        let provider = ReqwestProvider::new_http(rpc_url);
        let executor = HostExecutor::new(provider);

        executor.execute(block_number).await.unwrap()

        // let current_dir = env::current_dir().expect("Failed to get current working directory");
        // let blocks_dir = current_dir.join("eval").join("blocks");
        // let file_path = blocks_dir.join(format!("{}.bin", block_number));

        // tracing::info!("Reading block file: {}", file_path.to_str().unwrap());

        // match fs::read(&file_path) {
        //     Ok(bytes) => {
        //         tracing::info!("Successfully read {} bytes from file", bytes.len());
        //         match bincode::deserialize(&bytes) {
        //             Ok(input) => {
        //                 tracing::info!("Successfully deserialized block input");
        //                 input
        //             }
        //             Err(e) => {
        //                 tracing::error!("Deserialization error: {:?}", e);
        //                 panic!("Failed to deserialize block file: {:?}", e);
        //             }
        //         }
        //     }
        //     Err(e) => {
        //         tracing::error!("Failed to read block file: {:?}", e);
        //         panic!("Unable to read block file: {:?}", e);
        //     }
        // }
    } else {
        panic!("Block number is required for Reth program");
    }
}

pub fn time_operation<T, F: FnOnce() -> T>(operation: F) -> (T, Duration) {
    let start = Instant::now();
    let result = operation();
    let duration = start.elapsed();
    (result, duration)
}
