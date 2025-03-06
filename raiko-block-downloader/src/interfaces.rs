use alloy_primitives::{Address, B256};
use raiko_lib::{
    input::BlobProofType,
};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
/// A request for a proof.
pub struct ProofRequest {
    /// The block number for the block to generate a proof for.
    pub block_number: u64,
    /// The l1 block number of the l2 block be proposed.
    pub l1_inclusion_block_number: u64,
    /// The network to generate the proof for.
    pub network: String,
    /// The L1 network to generate the proof for.
    pub l1_network: String,
    /// Graffiti.
    pub graffiti: B256,
    /// The protocol instance data.
    #[serde_as(as = "DisplayFromStr")]
    pub prover: Address,
    /// Blob proof type.
    pub blob_proof_type: BlobProofType,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, ToSchema)]
#[serde(default)]
/// A request for proof aggregation of multiple proofs.
pub struct AggregationRequest {
    /// The block numbers and l1 inclusion block numbers for the blocks to aggregate proofs for.
    pub block_numbers: Vec<(u64, Option<u64>)>,
    /// The network to generate the proof for.
    pub network: Option<String>,
    /// The L1 network to generate the proof for.
    pub l1_network: Option<String>,
    // Graffiti.
    pub graffiti: Option<String>,
    /// The protocol instance data.
    pub prover: Option<String>,
    /// The proof type.
    pub proof_type: Option<String>,
    /// Blob proof type.
    pub blob_proof_type: Option<String>,
}
