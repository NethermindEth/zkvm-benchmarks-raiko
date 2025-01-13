mod types;

use clap::Parser;
use types::{ProgramId, ProverId};

#[derive(Parser, Clone)]
#[command(about = "Evaluate the performance of a zkVM on a program.")]
pub struct EvalArgs {
    #[arg(long)]
    program: ProgramId,
    #[arg(long)]
    prover: ProverId,
    // #[arg(long)]
    // hashfn: HashFnId,
    #[arg(long)]
    shard_size: u64,
    #[arg(long)]
    filename: String,
    #[arg(long)]
    block_number: Option<u64>,
    #[arg(long)]
    fibonacci_input: Option<u32>,
}

fn main() {
    //let args = Eval
}
