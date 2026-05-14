use std::sync::Arc;
use anyhow::Result;

use crate::probe::ProbeManager;
use crate::cli::flows::modify::modify_file_flow;

pub fn run(probe_mgr: Arc<ProbeManager>) -> Result<()> {
    loop {
        // Display VM status table before menu
        match crate::virsh::list_vms() {
            Ok(vms) => {
                if vms.is_empty() {
                    println!("\nNo VMs found.");
                    println!("Make sure libvirt is running and you have VMs defined.");
                    println!("Try: virsh list --all\n");
                } else {
                    println!("\n{:20} {:40} {:24} {}", "VM", "OS", "Memory (used/max)", "CPU time");
                    println!("{}", "-".repeat(110));
                    for vm in &vms {
                        // OS probe (cached by ProbeManager)
                        let os = match probe_mgr.get_os(vm) {
                            Ok(Some(s)) => s,
                            Ok(None) => "(unknown)".to_string(),
                            Err(e) => format!("error: {}", e),
                        };

                        // dominfo probe (raw virsh output -> parsed DomInfo)
                        let dominfo = match crate::virsh::dominfo_raw(vm) {
                            Ok(raw) => crate::utils::parse_dominfo(&raw),
                            Err(_) => crate::utils::DomInfo { max_memory_mb: None, used_memory_mb: None, cpu_time: None },
                        };

                        // Memory formatting
                        let mem_used = crate::utils::format_memory_kib(dominfo.used_memory_mb);
                        let mem_max = crate::utils::format_memory_kib(dominfo.max_memory_mb);
                        let mem = if mem_used != "(unknown)" && mem_max != "(unknown)" {
                            format!("{} / {}", mem_used, mem_max)
                        } else if mem_used != "(unknown)" {
                            mem_used
                        } else if mem_max != "(unknown)" {
                            mem_max
                        } else {
                            "(unknown)".to_string()
                        };

                        // CPU time
                        let cpu = dominfo.cpu_time
                            .as_deref()
                            .and_then(|s| crate::utils::parse_cpu_time_to_seconds(s))
                            .map(|secs| crate::utils::format_seconds_dhms(secs))
                            .unwrap_or_else(|| dominfo.cpu_time.clone().unwrap_or_else(|| "(unknown)".to_string()));

                        println!("{:20} {:40} {:24} {}", vm, os, mem, cpu);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to list VMs: {}", e);
            }
        }

        println!("\n--- MENU ---");
        println!("1) Mount ISO");
        println!("2) Scan mounted ISOs");
        println!("3) Modify file in VM");
        println!("4) Exit");
        print!("Select option: ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        match input.trim() {
            "1" => println!("Mount ISO not implemented."),
            "2" => {
                let vms = crate::virsh::list_vms()?;
                for vm in vms {
                    match probe_mgr.get_os(&vm) {
                        Ok(Some(os)) => println!("{:20} {}", vm, os),
                        _ => println!("{:20} (unknown)", vm),
                    }
                }
            }
            "3" => {
                if let Err(e) = modify_file_flow() {
                    eprintln!("Error: {}", e);
                }
            }
            "4" => break,
            _ => println!("Invalid option"),
        }
    }
    Ok(())
}
