use std::{
    env, fs,
    time::{Duration, Instant},
};

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

pub fn get_reth_input(args: &EvalArgs) -> ClientExecutorInput {
    if let Some(block_number) = args.block_number {
        let current_dir = env::current_dir().expect("Failed to get current working directory");

        let blocks_dir = current_dir.join("eval").join("blocks");

        let file_path = blocks_dir.join(format!("{}.bin", block_number));

        if let Ok(bytes) = fs::read(file_path) {
            bincode::deserialize(&bytes).expect("Unable to deserialize input")
        } else {
            panic!("Unable to read block file");
        }
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
