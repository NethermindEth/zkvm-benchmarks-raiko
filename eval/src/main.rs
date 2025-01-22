mod risc0;
mod types;
mod utils;

use std::{
    fs::{create_dir_all, OpenOptions},
    path::PathBuf,
};

use block_downloader::BlockDownloader;
use clap::Parser;
use csv::WriterBuilder;
use eyre::{eyre, Result};
use risc0::Risc0Evaluator;
use serde::Serialize;
use types::{ProgramId, ProverId};
use url::Url;

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
    #[arg(long)]
    rpc_url: Option<Url>,
}

/// The performance report of a zkVM on a program.
#[derive(Debug, Serialize, Default)]
pub struct PerformanceReport {
    /// The program that is being evaluated.
    pub program: String,
    /// The prover that is being evaluated.
    pub prover: String,
    // /// The hash function that is being evaluated.
    // pub hashfn: String,
    /// The shard size that is being evaluated.
    pub shard_size: u64,
    /// The number of shards.
    pub shards: usize,
    /// The reported number of cycles.
    ///
    /// Note that this number may vary based on the zkVM.
    pub cycles: u64,
    /// The reported speed in cycles per second.
    pub speed: f64,
    /// The reported duration of the execution in seconds.
    pub execution_duration: f64,
    /// The reported duration of the prover in seconds.
    pub prove_duration: f64,
    /// The reported duration of the core proving time in seconds.
    pub core_prove_duration: f64,
    /// The reported duration of the verifier in seconds.
    pub core_verify_duration: f64,
    /// The size of the core proof.
    pub core_proof_size: usize,
    /// The reported duration of the recursive proving time in seconds.
    pub compress_prove_duration: f64,
    /// The reported duration of the verifier in seconds.
    pub compress_verify_duration: f64,
    /// The size of the recursive proof in bytes.
    pub compress_proof_size: usize,
}

/// Ensures that the required block is available locally.
/// If not, downloads it using the BlockDownloader.
async fn ensure_block_available(args: &EvalArgs) -> Result<()> {
    if args.program != ProgramId::Reth {
        return Ok(());
    }

    let block_number = if let Some(block_number) = args.block_number {
        block_number
    } else {
        return Ok(());
    };

    let blocks_dir = PathBuf::from("eval/blocks");
    let block_path = blocks_dir.join(format!("{}.bin", block_number));

    // If the block file doesn't exist, download it
    if !block_path.exists() {
        tracing::info!("Block {} not found locally, downloading...", block_number);

        // Check if RPC URL is provided when needed
        let rpc_url = args.rpc_url.clone().ok_or_else(|| {
            eyre!(
                "RPC URL is required when block {} is not available locally",
                block_number
            )
        })?;

        let downloader = BlockDownloader::new(blocks_dir, rpc_url)?;
        downloader.download_and_save_block(block_number).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    let args = EvalArgs::parse();

    // Ensure block is available if needed
    ensure_block_available(&args).await?;

    // Select the correct implementation based on the prover.
    let report = match args.prover {
        ProverId::Risc0 => Risc0Evaluator::eval(&args),
        ProverId::SP1 => todo!(),
    };

    // Create the results directory if it doesn't exist.
    let results_dir = PathBuf::from("results");
    create_dir_all(&results_dir)?;

    // Create the file
    let filename = format!("{}_{}.csv", args.filename, env!("VERGEN_GIT_SHA"));
    let path = results_dir.join(filename);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.clone())?;

    // Write the row and the header if needed.
    let mut writer = WriterBuilder::new().from_writer(&file);
    if file.metadata()?.len() == 0 {
        writer.write_record(&[
            "program",
            "prover",
            "hashfn",
            "shard_size",
            "shards",
            "cycles",
            "speed",
            "execution_duration",
            "prove_duration",
            "core_prove_duration",
            "core_verify_duration",
            "core_proof_size",
            "compress_prove_duration",
            "compress_verify_duration",
            "compress_proof_size",
        ])?;
    }
    writer.serialize(&[
        report.program,
        report.prover,
        //report.hashfn,
        report.shard_size.to_string(),
        report.shards.to_string(),
        report.cycles.to_string(),
        report.speed.to_string(),
        report.execution_duration.to_string(),
        report.prove_duration.to_string(),
        report.core_prove_duration.to_string(),
        report.core_verify_duration.to_string(),
        report.core_proof_size.to_string(),
        report.compress_prove_duration.to_string(),
        report.compress_verify_duration.to_string(),
        report.compress_proof_size.to_string(),
    ])?;
    writer.flush()?;

    let latest_filename = format!("{}_latest.csv", args.filename);
    let latest_path = results_dir.join(latest_filename);
    std::fs::copy(&path, &latest_path)?;

    Ok(())
}
