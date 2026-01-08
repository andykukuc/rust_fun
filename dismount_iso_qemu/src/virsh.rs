// src/virsh.rs
use std::process::Command;
use std::io;
use serde_json::Value;

/// Simple wrapper to call `virsh qemu-agent-command` and return parsed JSON.
pub fn virsh_qemu_agent(vm: &str, payload: &str, timeout_secs: u64) -> io::Result<Value> {
    let out = Command::new("virsh")
        .args(["qemu-agent-command", "--timeout", &timeout_secs.to_string(), vm, payload])
        .output()?;
    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("virsh qemu-agent-command failed: {}", String::from_utf8_lossy(&out.stderr)),
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let json: Value = serde_json::from_str(&s)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("json parse: {}", e)))?;
    Ok(json)
}

/// Return VM names from `virsh list --all --name`.
/// Trims empty lines and returns Vec<String>.
pub fn list_vms() -> io::Result<Vec<String>> {
    let out = Command::new("virsh")
        .args(["list", "--all", "--name"])
        .output()?;
    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("virsh list failed: {}", String::from_utf8_lossy(&out.stderr)),
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let vms: Vec<String> = s
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    Ok(vms)
}

/// Return the raw `virsh dominfo <vm>` output as a String.
pub fn dominfo_raw(vm: &str) -> io::Result<String> {
    let out = Command::new("virsh")
        .args(["dominfo", vm])
        .output()?;
    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("virsh dominfo failed: {}", String::from_utf8_lossy(&out.stderr)),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Read a file from a VM using guest-file-open, guest-file-read, guest-file-close.
pub fn ga_read_file(vm: &str, path: &str) -> io::Result<Vec<u8>> {
    use base64::Engine;
    
    // 1. Open the file
    let open_payload = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": {"path": path, "mode": "r"}
    });
    let open_result = virsh_qemu_agent(vm, &open_payload.to_string(), 10)?;
    let handle = open_result
        .get("return")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get file handle"))?;

    // 2. Read the file in chunks
    let mut content = Vec::new();
    loop {
        let read_payload = serde_json::json!({
            "execute": "guest-file-read",
            "arguments": {"handle": handle, "count": 4096}
        });
        let read_result = virsh_qemu_agent(vm, &read_payload.to_string(), 10)?;
        
        let ret = read_result
            .get("return")
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No return in guest-file-read"))?;
        
        let buf_b64 = ret
            .get("buf-b64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No buf-b64 in response"))?;
        
        let chunk = base64::engine::general_purpose::STANDARD
            .decode(buf_b64)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("base64 decode: {}", e)))?;
        
        let eof = ret.get("eof").and_then(|v| v.as_bool()).unwrap_or(false);
        
        content.extend_from_slice(&chunk);
        
        if eof {
            break;
        }
    }

    // 3. Close the file
    let close_payload = serde_json::json!({
        "execute": "guest-file-close",
        "arguments": {"handle": handle}
    });
    let _ = virsh_qemu_agent(vm, &close_payload.to_string(), 10)?;

    Ok(content)
}

/// Write a file to a VM using guest-file-open, guest-file-write, guest-file-close.
pub fn ga_write_file(vm: &str, path: &str, content: &[u8]) -> io::Result<()> {
    use base64::Engine;
    
    // 1. Open the file for writing
    let open_payload = serde_json::json!({
        "execute": "guest-file-open",
        "arguments": {"path": path, "mode": "w"}
    });
    let open_result = virsh_qemu_agent(vm, &open_payload.to_string(), 10)?;
    let handle = open_result
        .get("return")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get file handle"))?;

    // 2. Write the file in chunks
    let chunk_size = 4096;
    for chunk in content.chunks(chunk_size) {
        let buf_b64 = base64::engine::general_purpose::STANDARD.encode(chunk);
        let write_payload = serde_json::json!({
            "execute": "guest-file-write",
            "arguments": {"handle": handle, "buf-b64": buf_b64}
        });
        virsh_qemu_agent(vm, &write_payload.to_string(), 10)?;
    }

    // 3. Close the file
    let close_payload = serde_json::json!({
        "execute": "guest-file-close",
        "arguments": {"handle": handle}
    });
    let _ = virsh_qemu_agent(vm, &close_payload.to_string(), 10)?;

    Ok(())
}
