//! Functions specific to the wallet.
//!
//! The [`Wallet`] should be used together with a [`Canister`].
//!
//! ```
//! # async fn run() {
//! use ic_test_utils::{get_agent, Canister};
//!
//! let user = "bob";
//! let agent = get_agent(user, None).await.unwrap();
//! let wallet = Canister::new_wallet(&agent, user, None);
//! # }
//! ```
use std::fs::{canonicalize, read_to_string};
use std::path::PathBuf;

use candid::{CandidType, Decode, Deserialize, Encode};
use ic_agent::ic_types::Principal;
use ic_agent::{agent::UpdateBuilder, Agent};

use super::Canister;
use crate::get_waiter;
use crate::{Error, Result};

pub const WALLET_IDS_PATH: &str = "../../.dfx/local/wallets.json";

fn get_wallet_principal<'a>(
    account_name: impl AsRef<str>,
    wallet_id_path: impl Into<Option<&'a str>>,
) -> Result<Principal> {
    let wallet_id_path = PathBuf::from(wallet_id_path.into().unwrap_or(WALLET_IDS_PATH));
    let wallet_id_path = canonicalize(&wallet_id_path)?;
    let json_str = match read_to_string(wallet_id_path.clone()) {
        Ok(identity) => identity,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::Generic(format!(
                "wallet ids not found at path: {:?}",
                wallet_id_path
            )));
        }
        e @ Err(_) => e?,
    };
    let json = serde_json::from_str::<serde_json::Value>(&json_str)?;
    let id = json["identities"][account_name.as_ref()]["local"]
        .as_str()
        .ok_or(Error::InvalidOrMissingAccountInJson)?;
    let principal = Principal::from_text(id)?;
    Ok(principal)
}

/// The balance result of a `Wallet::balance` call.
#[derive(Debug, CandidType, Deserialize)]
pub struct BalanceResult {
    pub amount: u64,
}

/// The result of a `Wallet::call_forward` call.
#[derive(Debug, CandidType, Deserialize)]
pub struct CallResult {
    #[serde(with = "serde_bytes")]
    #[serde(rename = "return")]
    pub payload: Vec<u8>,
}

#[derive(CandidType, Deserialize)]
pub struct CreateResult {
    pub canister_id: Principal,
}

#[derive(Debug, CandidType, Deserialize)]
struct CallForwardArgs {
    canister: Principal,
    method_name: String,
    #[serde(with = "serde_bytes")]
    args: Vec<u8>,
    cycles: u64,
}

/// Wallet for cycles
pub struct Wallet;

impl<'agent> Canister<'agent, Wallet> {
    /// Create a new wallet canister.
    /// If the `wallet_id_path` is `None` then the default [`WALLET_IDS_PATH`] will
    /// be used.
    pub fn new_wallet<'a>(
        agent: &'agent Agent,
        account_name: impl AsRef<str>,
        wallet_id_path: impl Into<Option<&'a str>>,
    ) -> Result<Self> {
        let id = get_wallet_principal(account_name, wallet_id_path)?;
        let inst = Self::new(id, agent);
        Ok(inst)
    }

    /// Get the current balance of a canister
    pub async fn balance(&self) -> Result<BalanceResult> {
        let mut builder = self.agent.query(self.principal(), "wallet_balance");
        builder.with_arg(&Encode!(&())?);
        let data = builder.call().await?;
        let balance = Decode!(&data, BalanceResult)?;
        Ok(balance)
    }

    /// Forward a call through the wallet, so cycles can be spent.
    pub async fn call_forward(&self, call: UpdateBuilder<'_>, cycles: u64) -> Result<Vec<u8>> {
        let call_forward_args = CallForwardArgs {
            canister: call.canister_id,
            method_name: call.method_name,
            args: call.arg,
            cycles,
        };
        let mut builder = self.agent.update(self.principal(), "wallet_call");
        builder.with_arg(&Encode!(&call_forward_args)?);
        let data = builder.call_and_wait(get_waiter()).await?;
        let val = Decode!(&data, std::result::Result<CallResult, String>)??;
        Ok(val.payload)
    }

    // There seem to be no use of compute allocation, memory allocation or freezing threshold.
    // If they are needed in the future we can add them as they are just newtypes around numbers,
    // and they should be sent along with the canister settings.
    /// Create an empty canister.
    /// This does not install the wasm code for the canister.
    /// To do that call [`Canister::install_code`] after creating a canister.
    pub async fn create_canister(
        &self,
        cycles: u64,
        controllers: impl Into<Option<Vec<Principal>>>,
    ) -> Result<Principal> {
        #[derive(Debug, CandidType, Deserialize)]
        struct In {
            cycles: u64,
            settings: CanisterSettings,
        }

        #[derive(Debug, CandidType, Deserialize)]
        struct CanisterSettings {
            controllers: Option<Vec<Principal>>,
            compute_allocation: Option<u8>,
            memory_allocation: Option<u64>,
            freezing_threshold: Option<u64>,
        }

        let mut builder = self
            .agent
            .update(self.principal(), "wallet_create_canister");
        let args = In {
            cycles,
            settings: CanisterSettings {
                controllers: controllers.into(),
                compute_allocation: None,
                memory_allocation: None,
                freezing_threshold: None,
            },
        };
        builder.with_arg(&Encode!(&args)?);
        let data = builder.call_and_wait(get_waiter()).await?;
        let result = Decode!(&data, std::result::Result<CreateResult, String>)??;
        Ok(result.canister_id)
    }
}

// -----------------------------------------------------------------------------
//     - TODO -
//     Do we need even need these types?
// -----------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
pub struct ComputeAllocation(u8);

impl std::convert::From<ComputeAllocation> for u8 {
    fn from(compute_allocation: ComputeAllocation) -> Self {
        compute_allocation.0
    }
}

macro_rules! try_from_compute_alloc_decl {
    ( $t: ty ) => {
        impl std::convert::TryFrom<$t> for ComputeAllocation {
            type Error = Error;

            fn try_from(value: $t) -> Result<Self> {
                if (value as i64) < 0 || (value as i64) > 100 {
                    Err(Error::MustBeAPercentage())
                } else {
                    Ok(Self(value as u8))
                }
            }
        }
    };
}

try_from_compute_alloc_decl!(u8);
try_from_compute_alloc_decl!(u16);
try_from_compute_alloc_decl!(u32);
try_from_compute_alloc_decl!(u64);
try_from_compute_alloc_decl!(i8);
try_from_compute_alloc_decl!(i16);
try_from_compute_alloc_decl!(i32);
try_from_compute_alloc_decl!(i64);

pub struct MemoryAllocation(u64);

impl std::convert::From<MemoryAllocation> for u64 {
    fn from(memory_allocation: MemoryAllocation) -> Self {
        memory_allocation.0
    }
}

macro_rules! try_from_memory_alloc_decl {
    ( $t: ty ) => {
        impl std::convert::TryFrom<$t> for MemoryAllocation {
            type Error = Error;

            fn try_from(value: $t) -> Result<Self> {
                if (value as i64) < 0 || (value as i64) > (1i64 << 48) {
                    Err(Error::InvalidMemorySize(value as u64))
                } else {
                    Ok(Self(value as u64))
                }
            }
        }
    };
}

try_from_memory_alloc_decl!(u8);
try_from_memory_alloc_decl!(u16);
try_from_memory_alloc_decl!(u32);
try_from_memory_alloc_decl!(u64);
try_from_memory_alloc_decl!(i8);
try_from_memory_alloc_decl!(i16);
try_from_memory_alloc_decl!(i32);
try_from_memory_alloc_decl!(i64);
