use std::{collections::HashMap, iter::once};

use eyre::Result;
use itertools::Itertools;
use reth_primitives::{
    revm_primitives::{AccountInfo, Bytecode},
    Block, Header,
};
use reth_trie::TrieAccount;
use revm_primitives::{keccak256, Address, B256, U256};
use serde::{Deserialize, Serialize};

use crate::{db::WitnessDb, state::EthereumState};

/// The input for the client to execute a block and fully verify the STF (state transition
/// function).
///
/// Instead of passing in the entire state, we only pass in the state roots along with merkle proofs
/// for the storage slots that were modified and accessed.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClientExecutorInput {
    /// The current block (which will be executed inside the client).
    pub current_block: Block,
    /// The previous block headers starting from the most recent. There must be at least one header
    /// to provide the parent state root.
    pub ancestor_headers: Vec<Header>,
    /// Network state as of the parent block.
    pub parent_state: EthereumState,
    /// Requests to account state and storage slots.
    pub state_requests: HashMap<Address, Vec<U256>>,
    /// Account bytecodes.
    pub bytecodes: Vec<Bytecode>,
}

impl ClientExecutorInput {
    /// Gets the immediate parent block's header.
    #[inline(always)]
    pub fn parent_header(&self) -> &Header {
        &self.ancestor_headers[0]
    }

    /// Creates a [`WitnessDb`].
    pub fn witness_db(&self) -> Result<WitnessDb> {
        self.create_witness_db()
    }

    #[inline(always)]
    fn state(&self) -> &EthereumState {
        &self.parent_state
    }

    #[inline(always)]
    fn state_anchor(&self) -> B256 {
        self.parent_header().state_root
    }

    #[inline(always)]
    fn state_requests(&self) -> impl Iterator<Item = (&Address, &Vec<U256>)> {
        self.state_requests.iter()
    }

    #[inline(always)]
    fn bytecodes(&self) -> impl Iterator<Item = &Bytecode> {
        self.bytecodes.iter()
    }

    #[inline(always)]
    fn headers(&self) -> impl Iterator<Item = &Header> {
        once(&self.current_block.header).chain(self.ancestor_headers.iter())
    }

    /// Creates a [`WitnessDb`] from a [`WitnessInput`] implementation. To do so, it verifies the
    /// state root, ancestor headers and account bytecodes, and constructs the account and
    /// storage values by reading against state tries.
    ///
    /// NOTE: For some unknown reasons, calling this trait method directly from outside of the type
    /// implementing this trait causes a zkVM run to cost over 5M cycles more. To avoid this, define
    /// a method inside the type that calls this trait method instead.
    #[inline(always)]
    fn create_witness_db(&self) -> Result<WitnessDb> {
        let state = self.state();

        if self.state_anchor() != state.state_root() {
            eyre::bail!("parent state root mismatch");
        }

        let bytecodes_by_hash = self
            .bytecodes()
            .map(|code| (code.hash_slow(), code))
            .collect::<HashMap<_, _>>();

        let mut accounts = HashMap::new();
        let mut storage = HashMap::new();
        for (&address, slots) in self.state_requests() {
            let hashed_address = keccak256(address);
            let hashed_address = hashed_address.as_slice();

            let account_in_trie = state.state_trie.get_rlp::<TrieAccount>(hashed_address)?;

            accounts.insert(
                address,
                match account_in_trie {
                    Some(account_in_trie) => AccountInfo {
                        balance: account_in_trie.balance,
                        nonce: account_in_trie.nonce,
                        code_hash: account_in_trie.code_hash,
                        code: Some(
                            (*bytecodes_by_hash
                                .get(&account_in_trie.code_hash)
                                .ok_or_else(|| eyre::eyre!("missing bytecode"))?)
                            // Cloning here is fine as `Bytes` is cheap to clone.
                            .to_owned(),
                        ),
                    },
                    None => Default::default(),
                },
            );

            if !slots.is_empty() {
                let mut address_storage = HashMap::new();

                let storage_trie = state
                    .storage_tries
                    .get(hashed_address)
                    .ok_or_else(|| eyre::eyre!("parent state does not contain storage trie"))?;

                for &slot in slots {
                    let slot_value = storage_trie
                        .get_rlp::<U256>(keccak256(slot.to_be_bytes::<32>()).as_slice())?
                        .unwrap_or_default();
                    address_storage.insert(slot, slot_value);
                }

                storage.insert(address, address_storage);
            }
        }

        // Verify and build block hashes
        let mut block_hashes: HashMap<u64, B256> = HashMap::new();
        for (child_header, parent_header) in self.headers().tuple_windows() {
            if parent_header.number != child_header.number - 1 {
                eyre::bail!("non-consecutive blocks");
            }

            if parent_header.hash_slow() != child_header.parent_hash {
                eyre::bail!("parent hash mismatch");
            }

            block_hashes.insert(parent_header.number, child_header.parent_hash);
        }

        Ok(WitnessDb {
            accounts,
            storage,
            block_hashes,
        })
    }
}
