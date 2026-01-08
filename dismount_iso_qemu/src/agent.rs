use serde_json::Value;
use std::io;
use crate::virsh;

/// Try guest-get-osinfo and return a friendly OS string if present.
pub fn try_guest_get_osinfo(vm: &str, timeout_secs: u64) -> io::Result<Option<String>> {
    let payload = r#"{"execute":"guest-get-osinfo"}"#;
    let json: Value = virsh::virsh_qemu_agent(vm, payload, timeout_secs)?;
    if let Some(ret) = json.get("return") {
        if let Some(pretty_name) = ret.get("pretty-name").and_then(|v| v.as_str()) {
            return Ok(Some(pretty_name.to_string()));
        }
        if let Some(pretty) = ret.get("pretty").and_then(|v| v.as_str()) {
            return Ok(Some(pretty.to_string()));
        }
        if let Some(name) = ret.get("name").and_then(|v| v.as_str()) {
            let ver = ret.get("version").and_then(|v| v.as_str()).unwrap_or("");
            return Ok(Some(if ver.is_empty() { name.to_string() } else { format!("{} {}", name, ver) }));
        }
        // Fallback: stringify the return object if nothing else matched
        return Ok(Some(ret.to_string()));
    }
    Ok(None)
}

/// Try guest-get-os (older RPC) — similar parsing strategy.
pub fn try_guest_get_os(vm: &str, timeout_secs: u64) -> io::Result<Option<String>> {
    let payload = r#"{"execute":"guest-get-os"}"#;
    let json: Value = virsh::virsh_qemu_agent(vm, payload, timeout_secs)?;
    if let Some(ret) = json.get("return") {
        if let Some(pretty) = ret.get("pretty").and_then(|v| v.as_str()) {
            return Ok(Some(pretty.to_string()));
        }
        if let Some(name) = ret.get("name").and_then(|v| v.as_str()) {
            let ver = ret.get("version").and_then(|v| v.as_str()).unwrap_or("");
            return Ok(Some(if ver.is_empty() { name.to_string() } else { format!("{} {}", name, ver) }));
        }
    }
    Ok(None)
}
