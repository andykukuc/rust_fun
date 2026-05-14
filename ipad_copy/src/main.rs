use anyhow::{anyhow, Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rusty_libimobiledevice::idevice::get_devices;
use rusty_libimobiledevice::services::afc::{AfcClient, AfcFileMode};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Local output directory
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    fs::create_dir_all(&args.output)
        .with_context(|| format!("Failed to create output dir {:?}", args.output))?;

    // 1️⃣ Wait for device
    println!("Waiting for device to be connected and trusted...");
    let device = loop {
        let mut devices = get_devices().unwrap_or_default();
        if !devices.is_empty() {
            println!("Device detected: {:?}", devices[0].get_udid());
            break devices.remove(0);
        }
        thread::sleep(Duration::from_secs(2));
    };

    // 2️⃣ Wait for AFC
    println!("Waiting for AFC service to become available...");
    let mut afc_client = None;
    loop {
        match AfcClient::start_service(&device, "com.apple.afc") {
            Ok(client) => {
                afc_client = Some(client);
                println!("AFC connection established.");
                break;
            }
            Err(e) => {
                eprintln!("AFC not ready yet: {}", e);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
    let mut afc = afc_client
        .ok_or_else(|| anyhow!("Failed to connect to AFC — is the device unlocked and trusted?"))?;

    // 3️⃣ Wait for DCIM folder
    let remote_root = "DCIM";
    println!("Waiting for DCIM folder to be accessible...");
    let entries = loop {
        match afc.read_directory(remote_root) {
            Ok(list) => {
                if list.iter().any(|s| s != "." && s != "..") {
                    println!("DCIM folder found.");
                    break list;
                } else {
                    println!("DCIM is empty — no photos/videos yet.");
                    return Ok(());
                }
            }
            Err(e) => {
                eprintln!("DCIM not ready yet: {}", e);
                thread::sleep(Duration::from_secs(2));
            }
        }
    };

    // 4️⃣ Collect files
    let mut files_to_copy = Vec::new();
    for sub in entries.into_iter().filter(|s| s != "." && s != "..") {
        let full_subdir = format!("{}/{}", remote_root, sub);
        if let Ok(files) = afc.read_directory(&full_subdir) {
            for f in files.into_iter().filter(|s| s != "." && s != "..") {
                let remote_path = format!("{}/{}", full_subdir, f);
                if let Some(ext) = Path::new(&remote_path).extension().and_then(|e| e.to_str()) {
                    match ext.to_ascii_lowercase().as_str() {
                        "jpg" | "jpeg" | "heic" => files_to_copy.push(remote_path),
                        _ => {}
                    }
                }
            }
        }
    }

    if files_to_copy.is_empty() {
        println!("No matching files found in DCIM.");
        return Ok(());
    }

    // 5️⃣ Copy files with progress bar
    let pb = ProgressBar::new(files_to_copy.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("##-"),
    );

    for remote in files_to_copy {
        let filename = Path::new(&remote)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let local_path = args.output.join(filename);

        let handle = afc
            .file_open(&remote, AfcFileMode::ReadOnly)
            .map_err(|e| anyhow!("file_open({remote}): {e}"))?;

        let mut contents: Vec<u8> = Vec::new();
        let chunk: u32 = 128 * 1024;
        loop {
            let part = afc
                .file_read(handle, chunk)
                .map_err(|e| anyhow!("file_read({remote}): {e}"))?;
            if part.is_empty() {
                break;
            }
            contents.extend_from_slice(&part);
            if part.len() < chunk as usize {
                break;
            }
        }

        File::create(&local_path)?.write_all(&contents)?;

        pb.inc(1);
        pb.set_message(filename.to_string());
    }

    pb.finish_with_message("Done");
    Ok(())
}
