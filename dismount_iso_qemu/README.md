### Overview
A compact Rust CLI tool that enumerates libvirt/QEMU virtual machines, probes each guest for OS, memory, and CPU telemetry, prints a human‑readable status table on demand, and provides an interactive menu for VM management and file editing. Designed for reliability, safety, and easy extension into production workflows.

---

### Features
- **Interactive VM scanning** that lists VM name, detected OS, memory used/max, and normalized CPU time.  
- **Multi‑strategy OS detection** using QEMU guest agent RPCs (`guest-get-osinfo`, `guest-get-os`, `guest-exec`) with conservative fallbacks.  
- **In-VM file editing** - edit files inside VMs using your local editor with hash-based change detection.  
- **Dominfo parsing** to extract memory and CPU metrics from `virsh dominfo`.  
- **Human readable formatting** for memory (KiB → KiB/MiB/GiB) and CPU time (days/hours/minutes/seconds).  
- **ProbeManager** with configurable timeouts and cache TTL to reduce repeated slow probes.  
- **Modular codebase** split into `cli`, `virsh`, `agent`, `probe`, and `utils` for easy testing and extension.

---

### Installation
- **Prerequisites**: Rust toolchain (cargo), libvirt and virsh installed, appropriate permissions to run `virsh` commands.  
- **Build**:
```bash
git clone <repo>
cd dismount_iso_qemu
cargo build --release
```
- **Run**:
```bash
cargo run
# or use the built binary
target/release/dismount_iso_qemu
```

---

### Usage
- **Interactive menu**: upon starting, the CLI displays:
```
--- MENU ---
1) Mount ISO
2) Scan mounted ISOs
3) Modify file in VM
4) Exit
Select option:
```
- **VM Scan** (option 2): displays a fresh table of all VMs with their OS, memory usage, and CPU time:
```
VM                   OS                                       Memory (used/max)     CPU time
--------------------------------------------------------------------------------------------------------------
pinhole_new          Ubuntu 18.04.6 LTS                       8.0 GiB / 8.0 GiB      1d 23h 18m 39s
apollo_nms           CentOS Stream 10 (Coughlan)              8.0 GiB / 8.0 GiB      28d 4h 26m 33s
fs00                 Windows Server 2022 Datacenter           32.0 GiB / 32.0 GiB    2d 13h 59m 20s
```
- **Modify file in VM** (option 3): interactively edit files inside VMs:
  1. Prompts for VM name
  2. Prompts for remote file path (e.g., `C:\nps.xml` for Windows or `/etc/config` for Linux)
  3. Prompts for local file path (uses remote filename if empty)
  4. Downloads the file via QEMU guest agent
  5. Opens it in your `$EDITOR` (defaults to nano)
  6. Detects changes via SHA256 hash
  7. Uploads modified file back to VM only if changed
- **Configuration**: set `LIBVIRT_URI` environment variable to change the libvirt connection string:
```bash
export LIBVIRT_URI="qemu+ssh://root@host/system"
```
- **Editor**: set `EDITOR` environment variable to use your preferred editor:
```bash
export EDITOR=vim
```

---

### Configuration
- **Probe timeout**: configured in `main.rs` via `Duration::from_secs(5)`; increase for slow guests.  
- **Cache TTL**: configured in `ProbeManager` via `Duration::from_secs(60)`; increase to reduce probe frequency.  
- **Localization**: `virsh dominfo` output can vary by locale; adjust `parse_dominfo` if your environment uses non‑English labels.  
- **Productionization tips**:
  - Run as a systemd service or container for continuous monitoring.  
  - Expose metrics (Prometheus) and structured logs for observability.  
  - Parallelize probes with a thread pool or `rayon` for large VM fleets.

---

### Roadmap
- **ISO mounting/unmounting** via QEMU guest agent or virsh commands.  
- **Background scanning** with a channel to update the CLI without interleaving prompts.  
- **Parallel probes** to reduce scan latency for large VM fleets.  
- **Diff preview** before pushing file changes back to VM.  
- **Rollback on failure** when file write to VM fails.  
- **Cache dominfo** results in `ProbeManager` and add TTL per metric.  
- **Prometheus metrics and health checks** for integration with monitoring systems.  
- **Integration tests** that mock `virsh` and guest agent responses to validate parsing and fallbacks.  

---

**Quick start tip**: keep `LIBVIRT_URI` and probe timeout tuned to your environment, and add the binary to systemd for continuous status reporting.