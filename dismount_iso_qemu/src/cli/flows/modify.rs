use anyhow::Result;
use sha2::{Sha256, Digest};

use crate::cli::prompts::prompt;
use crate::utils::{normalize_windows_path, resolve_local_path, open_in_editor};
use crate::virsh;

pub fn modify_file_flow() -> Result<()> {
    let vms = virsh::list_vms()?;
    if vms.is_empty() {
        println!("No VMs found.");
        return Ok(());
    }

    let vm = prompt("VM name: ")?;
    if !vms.iter().any(|v| v == &vm) {
        println!("VM '{}' not found.", vm);
        return Ok(());
    }

    let remote_raw = prompt("Path inside VM (e.g. C:\\nps.xml): ")?;
    let remote_path = normalize_windows_path(&remote_raw);

    let local_raw = prompt("Local file to edit (Linux path): ")?;
    if local_raw.contains(":\\") {
        println!("ERROR: Local path must be a Linux path.");
        return Ok(());
    }

    let local_path = resolve_local_path(&local_raw, &remote_raw);
    println!("Using local file: {}", local_path.display());

    let original = virsh::ga_read_file(&vm, &remote_path)?;
    std::fs::write(&local_path, &original)?;

    let original_hash = Sha256::digest(&original);

    println!("Opening editor...");
    open_in_editor(&local_path)?;

    let updated = std::fs::read(&local_path)?;
    let updated_hash = Sha256::digest(&updated);

    if original_hash == updated_hash {
        println!("No changes detected. Skipping push.");
        return Ok(());
    }

    virsh::ga_write_file(&vm, &remote_path, &updated)?;
    println!("File successfully updated in VM.");

    Ok(())
}
