#![deny(missing_docs)]
#![doc = include_str!("../README.md")]
use std::path::Path;

use ic_agent::agent::http_transport::ReqwestHttpReplicaV2Transport;
use ic_agent::identity::BasicIdentity;
use ic_agent::Agent;

mod errors;
pub use errors::{Error, Result};

pub mod canister;

pub use canister::{
    management::{CanisterInstall, InstallMode, Management},
    wallet::Wallet,
    Canister,
};

const URL: &str = "http://localhost:8000";

/// Get the identity for an account.
/// This is useful for testing.
///
/// If this is ever needed outside of `get_agent` just make this
/// function public.
fn get_identity(account_name: impl AsRef<Path>) -> Result<BasicIdentity> {
    let mut ident_path = dirs::config_dir().ok_or(crate::Error::MissingConfig)?;
    ident_path.push("dfx/identity");
    ident_path.push(account_name);
    ident_path.push("identity.pem");

    let identity = BasicIdentity::from_pem_file(ident_path)?;
    Ok(identity)
}

/// Get an agent by identity name.
///
/// This is assuming there is an agent identity available.
/// If no identities area available then clone the correct **identity** project.
///
/// ```text
/// # Clone the identity project first
/// mkdir -p ~/.config/dfx/identity/
/// cp -Rn ./identity/.config/dfx/identity/* ~/.config/dfx/identity/
/// ```
pub async fn get_agent(name: impl AsRef<Path>, url: Option<&str>) -> Result<Agent> {
    let identity = get_identity(name)?;

    let url = url.unwrap_or(URL);
    let transport = ReqwestHttpReplicaV2Transport::create(url)?;

    let agent = Agent::builder()
        .with_transport(transport)
        .with_identity(identity)
        .build()?;

    agent.fetch_root_key().await?;

    Ok(agent)
}

/// Create a default `Delay` with a throttle of 500ms
/// and a timout of five minutes.
pub fn get_waiter() -> garcon::Delay {
    garcon::Delay::builder()
        .throttle(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(60 * 5))
        .build()
}
