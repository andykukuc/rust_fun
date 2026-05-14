use sysinfo::{System, Disks, Networks};
use serde_json::{json, Value};

pub fn get_system_info() -> Value {
    let mut sys = System::new_all();
    sys.refresh_all();

    let disks = Disks::new_with_refreshed_list();
    let networks = Networks::new_with_refreshed_list();

    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
    let os = format!(
        "{} {}",
        System::name().unwrap_or_default(),
        System::os_version().unwrap_or_default()
    );
    let uptime = System::uptime();
    let cpu_count = sys.cpus().len();
    let cpu_usage: f32 = if cpu_count > 0 {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpu_count as f32
    } else {
        0.0
    };

    let disk_info: Vec<Value> = disks
        .iter()
        .map(|d| {
            json!({
                "name": d.name().to_string_lossy(),
                "total_bytes": d.total_space(),
                "available_bytes": d.available_space(),
            })
        })
        .collect();

    let net_info: Vec<Value> = networks
        .iter()
        .map(|(name, data)| {
            json!({
                "interface": name,
                "rx_bytes": data.total_received(),
                "tx_bytes": data.total_transmitted(),
            })
        })
        .collect();

    json!({
        "hostname": hostname,
        "os": os,
        "uptime_seconds": uptime,
        "cpu_count": cpu_count,
        "cpu_usage_percent": cpu_usage,
        "total_ram_bytes": sys.total_memory(),
        "used_ram_bytes": sys.used_memory(),
        "disks": disk_info,
        "networks": net_info,
    })
}
