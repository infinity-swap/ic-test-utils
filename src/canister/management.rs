//! Functions and structures to interact with management canister.
//!
//! The [`Management`] should be used together with a [`Canister`].

use ic_cdk::export::candid;
use ic_cdk::export::candid::{
    encode_args, utils::ArgumentEncoder, CandidType, Decode, Deserialize, Encode, Principal,
};

use super::wallet::Wallet;
use super::{Agent, Canister, CreateResult};
use crate::{get_waiter, Result};

/// The install mode of the canister to install. If a canister is already installed,
/// using [InstallMode::Install] will be an error. [InstallMode::Reinstall] overwrites
/// the module, and [InstallMode::Upgrade] performs an Upgrade step.
#[derive(Copy, Clone, CandidType, Deserialize, Eq, PartialEq)]
pub enum InstallMode {
    /// Install wasm
    #[serde(rename = "install")]
    Install,
    /// Reinstall wasm
    #[serde(rename = "reinstall")]
    Reinstall,
    /// Upgrade wasm
    #[serde(rename = "upgrade")]
    Upgrade,
}

/// Installation arguments for [`Canister::install_code`].
#[derive(CandidType, Deserialize)]
pub struct CanisterInstall {
    /// [`InstallMode`]
    pub mode: InstallMode,
    /// Canister id
    pub canister_id: Principal,
    #[serde(with = "serde_bytes")]
    /// Wasm module as raw bytes
    pub wasm_module: Vec<u8>,
    #[serde(with = "serde_bytes")]
    /// Any aditional arguments to be passed along
    pub arg: Vec<u8>,
}

#[derive(CandidType, Deserialize)]
struct In {
    canister_id: Principal,
}

// -----------------------------------------------------------------------------
//     - Management container -
// -----------------------------------------------------------------------------

/// The management canister is used to install code, upgrade, stop and delete
/// canisters.
///
/// ```
/// # use ic_agent::Agent;
/// use ic_test_utils::canister::Canister;
/// # async fn run(agent: &Agent, principal: ic_cdk::export::candid::Principal) {
/// let wallet = Canister::new_wallet(agent, "account_name", None).unwrap();
/// let management = Canister::new_management(agent);
/// management.stop_canister(&wallet, principal).await;
/// # }
/// ```
#[derive(Clone, Copy)]
pub struct Management;

impl<'agent> Canister<'agent, Management> {
    /// Create a new management canister
    pub fn new_management(agent: &'agent Agent) -> Self {
        let id = Principal::management_canister();
        Self::new(id, agent)
    }

    // Make a call through the wallet so cycles
    // can be spent
    async fn through_wallet_call<Out>(
        &self,
        wallet: &Canister<'_, Wallet>,
        fn_name: &str,
        cycles: u64,
        arg: Option<Vec<u8>>,
    ) -> Result<Out>
    where
        Out: CandidType + for<'de> Deserialize<'de>,
    {
        let call = self.update_raw(fn_name, arg)?;
        let result = wallet.call_forward(call, cycles).await?;
        let out = Decode!(&result, Out)?;
        Ok(out)
    }

    /// Install code in an existing canister through the `Wallet` interface.
    /// To create a canister first use [`Canister::create_canister`]
    pub async fn install_code<'wallet_agent, T: ArgumentEncoder>(
        &self,
        wallet: &Canister<'wallet_agent, Wallet>,
        canister_id: Principal,
        bytecode: Vec<u8>,
        arg: T,
    ) -> Result<()> {
        let install_args = CanisterInstall {
            mode: InstallMode::Install,
            canister_id,
            wasm_module: bytecode,
            arg: encode_args(arg)?,
        };

        let args = Encode!(&install_args)?;
        self.through_wallet_call::<()>(wallet, "install_code", 0, Some(args))
            .await?;

        Ok(())
    }

    /// Install code in an existing canister without calling to ledger canister with
    /// raw input arguments.
    /// To create a canister first use [`Canister::create_canister`]
    pub async fn install_code_directly_raw_args(
        &self,
        canister_id: Principal,
        bytecode: Vec<u8>,
        arg_raw: Vec<u8>,
    ) -> Result<()> {
        let install_args = CanisterInstall {
            mode: InstallMode::Install,
            canister_id,
            wasm_module: bytecode,
            arg: arg_raw,
        };

        let args = Encode!(&install_args)?;
        self.update_raw("install_code", Some(args))?
            .call_and_wait(get_waiter())
            .await?;

        Ok(())
    }

    /// Create an empty canister.
    /// This does not install the wasm code for the canister.
    /// To do that call [`Canister::install_code`] after creating a canister.
    pub async fn create_canister(
        &self,
        cycles: Option<u64>,
        controllers: impl Into<Option<Vec<Principal>>>,
        is_provisional: bool,
    ) -> Result<Principal> {
        #[derive(CandidType)]
        struct In {
            cycles: candid::Nat,
            settings: CanisterSettings,
        }

        #[derive(CandidType)]
        struct InProvisional {
            cycles: Option<candid::Nat>,
            settings: CanisterSettings,
        }

        #[derive(CandidType, Deserialize)]
        pub struct CanisterSettings {
            pub controllers: Option<Vec<Principal>>,
            pub compute_allocation: Option<candid::Nat>,
            pub memory_allocation: Option<candid::Nat>,
            pub freezing_threshold: Option<candid::Nat>,
        }

        let builder = if is_provisional {
            let mut builder = self
                .agent
                .update(self.principal(), "provisional_create_canister_with_cycles");
            let args = InProvisional {
                cycles: cycles.map(Into::into),
                settings: CanisterSettings {
                    controllers: controllers.into(),
                    compute_allocation: None,
                    memory_allocation: None,
                    freezing_threshold: None,
                },
            };
            builder.with_arg(&Encode!(&args)?);
            builder
        } else {
            let mut builder = self.agent.update(self.principal(), "create_canister");
            let args = CanisterSettings {
                controllers: controllers.into(),
                compute_allocation: None,
                memory_allocation: None,
                freezing_threshold: None,
            };
            builder.with_arg(&Encode!(&args)?);
            builder
        };

        let data = builder.call_and_wait(get_waiter()).await?;
        let result = Decode!(&data, CreateResult)?;
        Ok(result.canister_id)
    }

    /// Upgrade an existing canister.
    /// Upgrading a canister for a test is possible even if the underlying binary hasn't changed
    pub async fn upgrade_code<'wallet_agent, T: CandidType>(
        &self,
        wallet: &Canister<'wallet_agent, Wallet>,
        canister_id: Principal,
        bytecode: Vec<u8>,
        arg: T,
    ) -> Result<()> {
        let install_args = CanisterInstall {
            mode: InstallMode::Upgrade,
            canister_id,
            wasm_module: bytecode,
            arg: Encode!(&arg)?,
        };

        let args = Encode!(&install_args)?;
        self.through_wallet_call::<Principal>(wallet, "upgrade_code", 0, Some(args))
            .await?;
        Ok(())
    }

    /// Stop a running canister
    pub async fn stop_canister<'wallet_agent>(
        &self,
        wallet: &Canister<'wallet_agent, Wallet>,
        canister_id: Principal, // canister to stop
    ) -> Result<()> {
        let arg = Encode!(&In { canister_id })?;
        self.through_wallet_call::<()>(wallet, "stop_canister", 0, Some(arg))
            .await?;
        Ok(())
    }

    /// Stop a running canister without interacting with Wallet.
    pub async fn stop_canister_directly(
        &self,
        canister_id: Principal, // canister to stop
    ) -> Result<()> {
        let arg = Encode!(&In { canister_id })?;
        self.update("stop_canister", Some(arg))?
            .call_and_wait(get_waiter())
            .await?;
        Ok(())
    }

    /// Delete a canister. The target canister can not be running,
    /// make sure the canister has stopped first: [`Canister::stop_canister`]
    pub async fn delete_canister<'wallet_agent>(
        &self,
        wallet: &Canister<'wallet_agent, Wallet>,
        canister_id: Principal, // canister to delete
    ) -> Result<()> {
        let arg = Encode!(&In { canister_id })?;
        self.through_wallet_call(wallet, "delete_canister", 0, Some(arg))
            .await?;
        Ok(())
    }

    /// Delete a canister without interacting with Wallet. The target canister can not be running,
    /// make sure the canister has stopped first: [`Canister::stop_canister`]
    pub async fn delete_canister_directly(
        &self,
        canister_id: Principal, // canister to delete
    ) -> Result<()> {
        let arg = Encode!(&In { canister_id })?;
        self.update("delete_canister", Some(arg))?
            .call_and_wait(get_waiter())
            .await?;
        Ok(())
    }
}
