use raiko_lib::{
    input::{
        AggregationGuestInput, AggregationGuestOutput, GuestInput, GuestOutput,
        ZkAggregationGuestInput,
    },
    proof_type::ProofType,
    prover::{IdStore, IdWrite, Proof, ProofKey, Prover, ProverConfig, ProverError, ProverResult},
    Measurement,
};
use reth_primitives::B256;
use serde::{Deserialize, Serialize};
use sp1_prover::components::CpuProverComponents;
use sp1_sdk::{
    network::FulfillmentStrategy, NetworkProver, Prover as SP1ProverTrait, SP1Proof, SP1ProofMode,
    SP1ProofWithPublicValues, SP1ProvingKey, SP1VerifyingKey,
};
use sp1_sdk::{HashableKey, ProverClient, SP1Stdin};
use std::{borrow::BorrowMut, env, sync::Arc, time::Duration};
use tracing::{debug, error, info};
use anyhow::{anyhow, Result};

pub const ELF: &[u8] = include_bytes!("../../benchmarks/raiko-sp1/elf/raiko-sp1");
pub const AGGREGATION_ELF: &[u8] = include_bytes!("../../benchmarks/raiko-sp1/elf/raiko-sp1-aggregation");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sp1Param {
    pub recursion: RecursionMode,
}

const DEFAULT_TRUE: fn() -> bool = || true;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecursionMode {
    /// The proof mode for an SP1 core proof.
    Core,
    /// The proof mode for a compressed proof.
    Compressed,
    /// The proof mode for a PlonK proof.
    #[default]
    Plonk,
}

impl From<RecursionMode> for SP1ProofMode {
    fn from(value: RecursionMode) -> Self {
        match value {
            RecursionMode::Core => SP1ProofMode::Core,
            RecursionMode::Compressed => SP1ProofMode::Compressed,
            RecursionMode::Plonk => SP1ProofMode::Plonk,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Sp1Response {
    pub proof: Option<String>,
    /// for aggregation
    pub sp1_proof: Option<SP1ProofWithPublicValues>,
    pub vkey: Option<SP1VerifyingKey>,
}

pub struct Sp1Prover;

// TODO: Use cuda/non-cuda
impl Sp1Prover {
    async fn run(
        input: GuestInput,
        output: &GuestOutput,
        config: &ProverConfig,
    ) -> Result<SP1ProofWithPublicValues> {
        let param = Sp1Param::deserialize(config.get("sp1").unwrap()).unwrap();

        println!("param: {param:?}");
        let mut stdin = SP1Stdin::new();
        stdin.write(&input);

        let client: Box<dyn SP1ProverTrait<CpuProverComponents>> = Box::new(ProverClient::builder().cuda().build());
        let client = Arc::new(client);
        let (pk, vk) = client.setup(ELF);

        info!(
            "Sp1 Prover: block {:?} with vk {:?}",
            output.header.number,
            vk.bytes32()
        );

        debug!("Proving locally with recursion mode: {:?}", param.recursion);
        let prove_mode = SP1ProofMode::Plonk;
        let prove_result = client
            .prove(&pk, &stdin, prove_mode)
            .map_err(|e| anyhow!("Sp1: local proving failed: {e}"))?;

        Ok(prove_result)
    }

    async fn aggregate(
        input: AggregationGuestInput,
    ) -> Result<SP1ProofWithPublicValues> {
        let block_inputs: Vec<B256> = input
            .proofs
            .iter()
            .map(|proof| proof.input.unwrap())
            .collect::<Vec<_>>();
        let block_proof_vk = serde_json::from_str::<SP1VerifyingKey>(
            &input.proofs.first().unwrap().uuid.clone().unwrap(),
        )
        .map_err(|e| anyhow!("Failed to parse SP1 vk: {e}"))?;
        let stark_vk = block_proof_vk.vk.clone();
        let image_id = block_proof_vk.hash_u32();
        let aggregation_input = ZkAggregationGuestInput {
            image_id,
            block_inputs,
        };
        info!(
            "Collect {:?} proofs aggregation pi inputs: {:?}",
            input.proofs.len(),
            aggregation_input.block_inputs
        );

        let mut stdin = SP1Stdin::new();
        stdin.write(&aggregation_input);
        for proof in input.proofs.iter() {
            let sp1_proof = serde_json::from_str::<SP1Proof>(&proof.quote.clone().unwrap())
                .map_err(|e| anyhow!("Failed to parse SP1 proof: {e}"))?;
            match sp1_proof {
                SP1Proof::Compressed(block_proof) => {
                    stdin.write_proof(*block_proof, stark_vk.clone());
                }
                _ => {
                    error!("unsupported proof type for aggregation: {sp1_proof:?}");
                }
            }
        }

        let client: Box<dyn SP1ProverTrait<CpuProverComponents>> = Box::new(ProverClient::builder().cuda().build());
        let client = Arc::new(client);
        let (pk, vk) = client.setup(AGGREGATION_ELF);

        info!(
            "sp1 aggregate: {:?} based {:?} blocks with vk {:?}",
            reth_primitives::hex::encode_prefixed(stark_vk.hash_bytes()),
            input.proofs.len(),
            vk.bytes32()
        );

        let prove_result = client
            .prove(&pk, &stdin, SP1ProofMode::Plonk)
            .expect("proving failed");

        Ok(prove_result)
    }
}
