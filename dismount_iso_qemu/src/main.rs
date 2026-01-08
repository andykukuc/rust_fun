mod cli;
mod virsh;
mod agent;
mod probe;
mod utils;

use std::sync::Arc;
use std::time::Duration;
use probe::ProbeManager;

/// Entry point: create the ProbeManager and enter the interactive CLI.
/// The menu will handle displaying VM information.
fn main() -> anyhow::Result<()> {
    let libvirt_uri = std::env::var("LIBVIRT_URI").unwrap_or_else(|_| "qemu:///system".into());
    let timeout = Duration::from_secs(5);
    let cache_ttl = Duration::from_secs(60);

    let probe_mgr = Arc::new(ProbeManager::new(libvirt_uri, timeout, cache_ttl)?);

    // Enter interactive CLI (blocking)
    cli::menu::run(probe_mgr)?;
    Ok(())
}
