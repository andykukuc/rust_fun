# rust_fun

A collection of Rust projects — tools, utilities, and experiments built while learning Rust and solving real problems. Each subdirectory is a standalone Rust project.

## Projects

### `dismount_iso_qemu/`

A CLI tool for managing QEMU/KVM virtual machines. Enumerates libvirt VMs, probes each guest for OS, memory, and CPU telemetry, displays a status table, and provides an interactive menu for VM management and ISO handling. Built for day-to-day home lab server management.

### `cmd_exec/`

A Rust backend + web frontend combo for executing shell commands remotely through a browser. Includes a build script that packages the app for deployment on servers without internet access or Rust installed.

### `rust_email_cleanup/`

Connects to a Yahoo Mail account via IMAP and deletes old emails from a specified folder. Useful for automating inbox/bulk mail cleanup on a schedule.

### `ipad_copy/`

Copies files from an iPad to a local machine over USB. Handles the iOS file transfer protocol for syncing photos and documents without iTunes.

### `ipad_mtp_copy/`

iPad/iOS file copy tool using the MTP (Media Transfer Protocol) interface via Windows WPD (Windows Portable Devices) API. Includes a `wpd_enum` library crate for device enumeration.

### `app_creator_librenms/`

Creates and registers applications in LibreNMS (network monitoring platform) via its API. Automates onboarding of new devices and services into the monitoring stack.

### `copy_ios/`

iOS device file copy utility — copies files from an iOS device to local storage.

### `wasm_code/`

WebAssembly experiments in Rust using `wasm-bindgen`. Compiled to WASM and served as a web app.

## Building

Each project is self-contained with its own `Cargo.toml`. To build a specific project:

```bash
cd dismount_iso_qemu
cargo build --release
```

## Requirements

- Rust 1.70+
- Some projects have additional requirements (libvirt, IMAP credentials, WPD on Windows) — see individual project source for details

## License

MIT
