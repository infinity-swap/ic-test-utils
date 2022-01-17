//! Interact with canisters in tests.
//!
//! ```
//! use ic_test_utils::canister::Canister;
//!
//! # async fn run<'a, T>(canister: Canister<'a, T>, principal: ic_cdk::export::candid::Principal, agent: &'a ic_agent::Agent) {
//! let wallet = Canister::new_wallet(agent, "bob", None).unwrap();
//! let management = Canister::new_management(agent);
//! # }
//! ```
use std::marker::PhantomData;

use crate::Result;
use ic_agent::agent::{Agent, QueryBuilder, UpdateBuilder};
use ic_agent::ic_types::Principal;
use ic_cdk::export::candid::{CandidType, Encode};
use serde::Deserialize;

pub mod management;
pub mod wallet;

/// Represent a Canister in a test case
#[derive(Clone, Copy)]
pub struct Canister<'agent, T> {
    id: Principal,
    pub(crate) agent: &'agent Agent,
    _phantom_data: PhantomData<T>,
}

impl<'agent, T> Canister<'agent, T> {
    /// Create a new canister with a given `Principal`
    pub fn new(id: Principal, agent: &'agent Agent) -> Self {
        Self {
            id,
            agent,
            _phantom_data: PhantomData,
        }
    }

    /// The id of the canister
    pub fn principal(&self) -> &Principal {
        &self.id
    }

    /// Update
    fn update_raw(
        &self,
        method_name: impl Into<String>,
        args: Option<Vec<u8>>,
    ) -> Result<UpdateBuilder<'_>> {
        let mut builder = self.agent.update(&self.id, method_name);
        if let Some(ref args) = args {
            builder.with_arg(args);
        }
        Ok(builder)
    }

    /// Update call to the canister
    pub fn update<A: CandidType>(
        &self,
        method_name: impl Into<String>,
        args: Option<A>,
    ) -> Result<UpdateBuilder<'_>> {
        let mut builder = self.agent.update(&self.id, method_name);
        if let Some(ref args) = args {
            let args = Encode!(args)?;
            builder.with_arg(args);
        }
        Ok(builder)
    }

    /// Query the canister
    pub fn query(&self, method_name: impl Into<String>) -> QueryBuilder<'_> {
        self.agent.query(&self.id, method_name)
    }
}

/// 'Create canister' request result.
#[derive(CandidType, Deserialize, Debug)]
pub struct CreateResult {
    /// An identifier of the created canister.
    pub canister_id: Principal,
}
