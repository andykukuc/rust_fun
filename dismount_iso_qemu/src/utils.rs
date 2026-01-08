// src/utils.rs

use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, bail};

/// Normalize Windows paths for QEMU-GA
pub fn normalize_windows_path(path: &str) -> String {
    path.replace("\\", "\\\\")
}

/// Resolve a local Linux path safely
pub fn resolve_local_path(local: &str, remote: &str) -> PathBuf {
    if local.trim().is_empty() {
        let filename = Path::new(remote)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        PathBuf::from(filename.as_ref())
    } else {
        PathBuf::from(local)
    }
}

/// Open a file in the user's editor
pub fn open_in_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .unwrap_or_else(|_| "nano".to_string());

    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to launch editor '{}': {}", editor, e))?;

    if !status.success() {
        bail!("Editor exited with non-zero status");
    }

    Ok(())
}

/// Small struct to hold parsed dominfo values.
/// Note: field names use `_mb` to match existing callers, but many libvirt
/// installations report memory in KiB. The caller is responsible for treating
/// these numeric values appropriately (we provide `format_memory_kib`).
#[derive(Debug, Clone)]
pub struct DomInfo {
    pub max_memory_mb: Option<u64>,
    pub used_memory_mb: Option<u64>,
    pub cpu_time: Option<String>, // keep human string like "613h 33m 33s" or "154359.4s"
}

/// Parse `virsh dominfo` output for Max memory, Used memory, and CPU time.
/// This function extracts the first numeric token after the colon for memory
/// lines and the remainder of the line for CPU time.
pub fn parse_dominfo(s: &str) -> DomInfo {
    let mut max_memory_mb = None;
    let mut used_memory_mb = None;
    let mut cpu_time = None;

    for line in s.lines() {
        let l = line.trim();
        if l.starts_with("Max memory:") {
            if let Some(val) = l.splitn(2, ':').nth(1) {
                let v = val.trim().split_whitespace().next().unwrap_or("");
                if let Ok(n) = v.parse::<u64>() {
                    max_memory_mb = Some(n);
                }
            }
        } else if l.starts_with("Used memory:") {
            if let Some(val) = l.splitn(2, ':').nth(1) {
                let v = val.trim().split_whitespace().next().unwrap_or("");
                if let Ok(n) = v.parse::<u64>() {
                    used_memory_mb = Some(n);
                }
            }
        } else if l.starts_with("CPU time:") {
            if let Some(val) = l.splitn(2, ':').nth(1) {
                cpu_time = Some(val.trim().to_string());
            }
        }
    }

    DomInfo { max_memory_mb, used_memory_mb, cpu_time }
}

/// Parse CPU time strings commonly seen in `virsh dominfo`:
/// - "613h 33m 33s"
/// - "154359.4s"
/// - "12345s" or plain numeric seconds
///
/// Returns total seconds as u64, or None if parsing fails.
pub fn parse_cpu_time_to_seconds(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // If the string contains space-separated tokens like "1h 2m 3s"
    if s.contains(' ') || (s.contains('h') || s.contains('m')) {
        let mut total: u64 = 0;
        for token in s.split_whitespace() {
            let token = token.trim();
            if token.ends_with('h') {
                if let Ok(v) = token[..token.len() - 1].parse::<u64>() {
                    total = total.saturating_add(v.saturating_mul(3600));
                } else {
                    return None;
                }
            } else if token.ends_with('m') {
                if let Ok(v) = token[..token.len() - 1].parse::<u64>() {
                    total = total.saturating_add(v.saturating_mul(60));
                } else {
                    return None;
                }
            } else if token.ends_with('s') {
                // allow fractional seconds like "154359.4s"
                let num = &token[..token.len() - 1];
                if let Ok(f) = num.parse::<f64>() {
                    total = total.saturating_add(f as u64);
                } else {
                    return None;
                }
            } else {
                // fallback: numeric seconds (possibly fractional)
                if let Ok(f) = token.parse::<f64>() {
                    total = total.saturating_add(f as u64);
                } else {
                    return None;
                }
            }
        }
        return Some(total);
    }

    // Single token cases: "154359.4s", "154359s", or plain number
    let token = if s.ends_with('s') { &s[..s.len() - 1] } else { s };
    if let Ok(f) = token.parse::<f64>() {
        return Some(f as u64);
    }
    None
}

/// Convert an optional KiB value (as returned by many `virsh dominfo` outputs)
/// into a human readable string using binary units (KiB, MiB, GiB).
///
/// Note: callers that parsed memory as MB/KiB should pass the correct unit.
/// This helper assumes the numeric value is KiB (common for many libvirt setups).
pub fn format_memory_kib(kib: Option<u64>) -> String {
    match kib {
        None => "(unknown)".to_string(),
        Some(k) => {
            // treat k as KiB; convert to bytes for unit scaling
            let mut bytes = k.saturating_mul(1024u64);
            const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
            let mut unit = 0usize;
            while bytes >= 1024 && unit < UNITS.len() - 1 {
                bytes = bytes / 1024;
                unit += 1;
            }
            if unit >= 2 {
                // show one decimal for MiB+ for readability
                let denom = 1024u64.pow(unit as u32);
                let value = (k as f64 * 1024.0) / (denom as f64);
                format!("{:.1} {}", value, UNITS[unit])
            } else {
                format!("{} {}", bytes, UNITS[unit])
            }
        }
    }
}

/// Format seconds into a compact human string: "1d 2h 3m 4s" but omit zero units.
pub fn format_seconds_dhms(mut secs: u64) -> String {
    if secs == 0 {
        return "0s".to_string();
    }
    let days = secs / 86_400;
    secs %= 86_400;
    let hours = secs / 3600;
    secs %= 3600;
    let mins = secs / 60;
    let secs = secs % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if mins > 0 {
        parts.push(format!("{}m", mins));
    }
    if secs > 0 {
        parts.push(format!("{}s", secs));
    }
    parts.join(" ")
}
