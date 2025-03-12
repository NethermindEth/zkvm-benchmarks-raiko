#[cfg(feature = "sp1")]
use std::fs;
use std::str::FromStr;
use anyhow::Context;
#[cfg(feature = "sp1")]
use sp1_prover::{
    build, components::CpuProverComponents, utils::get_cycles,
};
#[cfg(feature = "sp1")]
use sp1_sdk::{setup_logger, SP1Context, SP1Prover, SP1Stdin};
#[cfg(all(feature = "sp1", not(feature = "cuda")))]
use sp1_stark::SP1ProverOpts;

#[cfg(all(feature = "cuda", feature = "sp1"))]
use sp1_cuda::SP1CudaProver;
use raiko_lib::input::{GuestInput, GuestOutput};
use raiko_lib::prover::Prover;
#[cfg(feature = "sp1")]
use crate::{
    types::ProgramId,
    utils::{get_elf, get_reth_input, read_block, time_operation},
};

use crate::{EvalArgs, PerformanceReport};

pub struct SP1Evaluator;

impl SP1Evaluator {
    #[cfg(feature = "sp1")]
    pub fn eval(args: &EvalArgs) -> PerformanceReport {
        // Setup the logger.
        setup_logger();

        let program_name = match args.program {
            ProgramId::Reth | ProgramId::Raiko => format!(
                "{}_{}",
                args.program.to_string(),
                args.block_number.unwrap().to_string()
            ),
            ProgramId::Fibonacci => format!(
                "{}_{}",
                args.program.to_string(),
                args.fibonacci_input.unwrap().to_string()
            ),
            _ => args.program.to_string(),
        };

        // Set enviroment variables to configure the prover.
        std::env::set_var("SHARD_SIZE", format!("{}", 1 << args.shard_size));
        // if args.program == ProgramId::Reth {
        //     std::env::set_var("SHARD_CHUNKING_MULTIPLIER", "4");
        // }

        // For now, keep Raiko prover separate
        // TODO: Unify
        // if matches!(args.program, ProgramId::Raiko) {
        //     let dir_suffix = args.taiko_blocks_dir_suffix.as_deref().expect("taiko_blocks_dir_suffix not provided");
        //     let block_number = args.block_number.expect("block_number not provided");
        //     let guest_input_json = String::from_utf8(read_block(&format!("blocks-taiko_{dir_suffix}"), block_number, "json")).unwrap();
        //     let guest_input: GuestInput = serde_json::from_str(&guest_input_json).unwrap();
        //     let rt = tokio::runtime::Runtime::new().unwrap();
        //     let _proof = rt.block_on(RaikoSp1Prover::run(guest_input.clone())).expect("Failed to run Raiko prover");
        //     return PerformanceReport::default();
        // }

        // todo!();

        // Get stdin
        let stdin = {
            let mut stdin = SP1Stdin::new();
            match args.program {
                ProgramId::Reth => {
                    let input = get_reth_input(args);
                    stdin.write_vec(input);
                }
                ProgramId::Fibonacci => {
                    stdin.write(&args.fibonacci_input.expect("missing fibonacci input"));
                }
                ProgramId::Raiko => {
                    let dir_suffix = args.taiko_blocks_dir_suffix.as_deref().expect("taiko_blocks_dir_suffix not provided");
                    let block_number = args.block_number.expect("block_number not provided");
                    let guest_input_json = String::from_utf8(read_block(&format!("blocks-taiko_{dir_suffix}"), block_number, "json")).unwrap();
                    let guest_input: GuestInput = serde_json::from_str(&guest_input_json).unwrap();

                    stdin.write(&guest_input);
                }
                _ => (/* NOOP */),
            }
            stdin
        };

        let elf_path = get_elf(args);
        let elf = fs::read(&elf_path).unwrap();

        let cycles = get_cycles(&elf, &stdin);

        let prover = SP1Prover::<CpuProverComponents>::new();

        #[cfg(feature = "cuda")]
        let server = SP1CudaProver::new().expect("Failed to initialize CUDA prover");

        // Setup the program.
        #[cfg(not(feature = "cuda"))]
        let (_, pk_d, program, vk) = prover.setup(&elf);

        #[cfg(feature = "cuda")]
        let (_, vk) = server.setup(&elf).unwrap();

        // Execute the program.
        let context = SP1Context::default();
        // let (_, execution_duration) =
        //     time_operation(|| prover.execute(&elf, &stdin, context.clone()).unwrap());
        let execution_duration = std::time::Duration::from_secs(10);

        // Setup the prover opionts.
        #[cfg(not(feature = "cuda"))]
        let opts = SP1ProverOpts::auto();

        // Generate the core proof (CPU).
        #[cfg(not(feature = "cuda"))]
        let (core_proof, prove_core_duration) = time_operation(|| {
            prover
                .prove_core(&pk_d, program, &stdin, opts, context)
                .unwrap()
        });

        // Generate the core proof (CUDA).
        #[cfg(feature = "cuda")]
        let (core_proof, prove_core_duration) =
            time_operation(|| server.prove_core(&stdin).unwrap());

        let num_shards = core_proof.proof.0.len();

        // Verify the proof.
        let core_bytes = bincode::serialize(&core_proof).unwrap();
        let (_, verify_core_duration) = time_operation(|| {
            prover
                .verify(&core_proof.proof, &vk)
                .expect("Proof verification failed")
        });

        #[cfg(not(feature = "cuda"))]
        let (compress_proof, compress_duration) =
            time_operation(|| prover.compress(&vk, core_proof, vec![], opts).unwrap());

        #[cfg(feature = "cuda")]
        let (compress_proof, compress_duration) =
            time_operation(|| server.compress(&vk, core_proof, vec![]).unwrap());

        let compress_bytes = bincode::serialize(&compress_proof).unwrap();
        println!("recursive proof size: {}", compress_bytes.len());

        let (_, verify_compress_duration) = time_operation(|| {
            prover
                .verify_compressed(&compress_proof, &vk)
                .expect("Proof verification failed")
        });

        #[cfg(not(feature = "cuda"))]
        let (shrink_proof, shrink_prove_duration) =
            time_operation(|| prover.shrink(compress_proof.clone(), opts).unwrap());

        #[cfg(feature = "cuda")]
        let (shrink_proof, shrink_prove_duration) =
            time_operation(|| server.shrink(compress_proof.clone()).unwrap());

        prover
            .verify_shrink(&shrink_proof, &vk)
            .expect("Proof verification failed");

        #[cfg(not(feature = "cuda"))]
        let (wrap_proof, wrap_prove_duration) =
            time_operation(|| prover.wrap_bn254(shrink_proof.clone(), opts).unwrap());

        #[cfg(feature = "cuda")]
        let (wrap_proof, wrap_prove_duration) =
            time_operation(|| server.wrap_bn254(shrink_proof.clone()).unwrap());

        let artifacts_dir =
            build::try_build_groth16_bn254_artifacts_dev(&wrap_proof.vk, &wrap_proof.proof);

        // Warm up the prover.
        prover.wrap_groth16_bn254(wrap_proof.clone(), &artifacts_dir);

        let (_groth16_proof, groth16_duration) =
            time_operation(|| prover.wrap_groth16_bn254(wrap_proof.clone(), &artifacts_dir));

        let artifacts_dir =
            build::try_build_plonk_bn254_artifacts_dev(&wrap_proof.vk, &wrap_proof.proof);

        // Warm up the prover.
        prover.wrap_plonk_bn254(wrap_proof.clone(), &artifacts_dir);

        let (_plonk_proof, plonk_duration) =
            time_operation(|| prover.wrap_plonk_bn254(wrap_proof, &artifacts_dir));

        let prove_duration = prove_core_duration + compress_duration;
        let core_khz = cycles as f64 / prove_core_duration.as_secs_f64() / 1_000.0;
        let overall_khz = cycles as f64 / prove_duration.as_secs_f64() / 1_000.0;

        // Create the performance report.
        PerformanceReport {
            program: program_name,
            prover: args.prover.to_string(),
            shard_size: args.shard_size,
            shards: num_shards,
            cycles: cycles as u64,
            speed: (cycles as f64) / prove_core_duration.as_secs_f64(),
            execution_duration: execution_duration.as_secs_f64(),
            prove_duration: prove_duration.as_secs_f64(),
            core_prove_duration: prove_core_duration.as_secs_f64(),
            core_verify_duration: verify_core_duration.as_secs_f64(),
            core_proof_size: core_bytes.len(),
            core_khz,
            compress_prove_duration: compress_duration.as_secs_f64(),
            compress_verify_duration: verify_compress_duration.as_secs_f64(),
            compress_proof_size: compress_bytes.len(),
            overall_khz,
            wrap_prove_duration: wrap_prove_duration.as_secs_f64(),
            shrink_prove_duration: shrink_prove_duration.as_secs_f64(),
            groth16_prove_duration: groth16_duration.as_secs_f64(),
            plonk_prove_duration: plonk_duration.as_secs_f64(),
        }
    }

    #[cfg(not(feature = "sp1"))]
    pub fn eval(_args: &EvalArgs) -> PerformanceReport {
        panic!("SP1 feature is not enabled. Please compile with --features sp1");
    }
}
