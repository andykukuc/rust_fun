use imap::Session;
use native_tls::TlsConnector;
use std::net::TcpStream;
use dotenv::dotenv;
use std::env;
use chrono::{Duration, Utc};
use std::io::{self, Write};
use std::collections::HashMap;

//Author: Andy Kukuc

fn optimal_chunk_size(total: usize) -> usize {
    match total {
        0 => 1,
        n if n <= 500 => n,                      // one shot — fastest, safe
        n if n <= 2_000 => (n / 4).min(500),     // ~4 iterations — fast, low strain
        n if n <= 10_000 => (n / 10).min(500),   // ~10–20 iterations — balanced
        _ => 500,                                 // hard cap — session protection
    }
}

fn connect_to_gmail(username: &str, password: &str)
    -> imap::error::Result<Session<native_tls::TlsStream<TcpStream>>>
{
    let tls = TlsConnector::builder().build().unwrap();
    let tcp_stream = TcpStream::connect(("imap.gmail.com", 993))?;
    let tls_stream = tls.connect("imap.gmail.com", tcp_stream)
        .map_err(|e| imap::error::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    let client = imap::Client::new(tls_stream);
    let session = client.login(username, password).map_err(|e| e.0)?;
    Ok(session)
}

fn folder_size(session: &mut Session<native_tls::TlsStream<TcpStream>>, folder: &str) -> imap::error::Result<u64> {
    let mailbox = session.examine(folder)?;
    if mailbox.exists == 0 {
        return Ok(0);
    }
    let uids: Vec<u32> = session.search("ALL")?.into_iter().collect();
    let mut total: u64 = 0;
    let chunk_size = optimal_chunk_size(uids.len());
    for chunk in uids.chunks(chunk_size) {
        let uid_set = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let fetches = session.fetch(&uid_set, "RFC822.SIZE")?;
        total += fetches.iter().filter_map(|f| f.size).map(|s| s as u64).sum::<u64>();
    }
    Ok(total)
}

fn list_folders(session: &mut Session<native_tls::TlsStream<TcpStream>>) -> imap::error::Result<Vec<String>> {
    let folders = session.list(None, Some("*"))?;
    let names: Vec<String> = folders.iter().map(|f| f.name().to_string()).collect();

    println!("\nAvailable folders/labels:");
    for (i, folder_name) in names.iter().enumerate() {
        let count = session.examine(folder_name).map(|mb| mb.exists).unwrap_or(0);
        println!("{}: {} ({} messages)", i + 1, folder_name, count);
    }

    print!("\nCalculating total mailbox size...");
    io::stdout().flush().unwrap();
    let mut total_bytes: u64 = 0;
    for folder_name in &names {
        total_bytes += folder_size(session, folder_name).unwrap_or(0);
    }
    println!(
        "\rTotal mailbox usage: {:.2} MB ({:.2} GB)          ",
        total_bytes as f64 / (1024.0 * 1024.0),
        total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    Ok(names)
}

fn cleanup_folder(session: &mut Session<native_tls::TlsStream<TcpStream>>, folder: &str, days_old: i64)
    -> imap::error::Result<()>
{
    session.select(folder)?;
    let cutoff_date = Utc::now().date_naive() - Duration::days(days_old);
    let query = format!("BEFORE {}", cutoff_date.format("%d-%b-%Y"));
    let ids: Vec<u32> = session.search(query)?.into_iter().collect();
    if ids.is_empty() {
        println!("No messages older than {} days found in '{}'.", days_old, folder);
        return Ok(());
    }
    let chunk_size = optimal_chunk_size(ids.len());
    let iterations = (ids.len() + chunk_size - 1) / chunk_size;
    println!("[Dry run] {} messages · chunk size {} · {} iteration(s)", ids.len(), chunk_size, iterations);
    println!("Deleting {} messages from '{}'...", ids.len(), folder);
    for chunk in ids.chunks(chunk_size) {
        let id_list = chunk.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        session.store(&id_list, "+FLAGS (\\Deleted)")?;
    }
    session.expunge()?;
    println!("Cleanup complete.");
    Ok(())
}

fn show_top_senders(session: &mut Session<native_tls::TlsStream<TcpStream>>, folder: &str, top_n: usize)
    -> imap::error::Result<()>
{
    session.examine(folder)?;
    let uids: Vec<u32> = session.search("ALL")?.into_iter().collect();
    if uids.is_empty() {
        println!("No messages in '{}'.", folder);
        return Ok(());
    }
    let chunk_size = optimal_chunk_size(uids.len());
    println!("Fetching FROM headers for {} messages (this may take a moment)...", uids.len());
    let mut sender_counts: HashMap<String, usize> = HashMap::new();
    for chunk in uids.chunks(chunk_size) {
        let uid_set = chunk.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let fetches = session.fetch(&uid_set, "BODY.PEEK[HEADER.FIELDS (FROM)]")?;
        for fetch in fetches.iter() {
            if let Some(header_bytes) = fetch.header() {
                let header = String::from_utf8_lossy(header_bytes);
                for line in header.lines() {
                    let lower = line.to_ascii_lowercase();
                    if lower.starts_with("from:") {
                        let sender = line[5..].trim().to_string();
                        *sender_counts.entry(sender).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let mut sorted: Vec<(&String, &usize)> = sender_counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    println!("\nTop {} senders in '{}':", top_n.min(sorted.len()), folder);
    for (sender, count) in sorted.iter().take(top_n) {
        println!("  {:>5}  {}", count, sender);
    }
    Ok(())
}

fn mark_all_read(session: &mut Session<native_tls::TlsStream<TcpStream>>, folder: &str)
    -> imap::error::Result<()>
{
    session.select(folder)?;
    let ids: Vec<u32> = session.search("UNSEEN")?.into_iter().collect();
    if ids.is_empty() {
        println!("No unread messages in '{}'.", folder);
        return Ok(());
    }
    let chunk_size = optimal_chunk_size(ids.len());
    println!("Marking {} messages as read in '{}'...", ids.len(), folder);
    for chunk in ids.chunks(chunk_size) {
        let id_list = chunk.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        session.store(&id_list, "+FLAGS (\\Seen)")?;
    }
    println!("Done.");
    Ok(())
}

fn clean_folder_all(session: &mut Session<native_tls::TlsStream<TcpStream>>, folder: &str)
    -> imap::error::Result<()>
{
    session.select(folder)?;
    let ids: Vec<u32> = session.search("ALL")?.into_iter().collect();
    if ids.is_empty() {
        println!("Spam folder '{}' is already empty.", folder);
        return Ok(());
    }
    let chunk_size = optimal_chunk_size(ids.len());
    let iterations = (ids.len() + chunk_size - 1) / chunk_size;
    println!("[Dry run] {} messages · chunk size {} · {} iteration(s)", ids.len(), chunk_size, iterations);
    println!("Auto-cleaning spam folder '{}': deleting {} messages...", folder, ids.len());
    for chunk in ids.chunks(chunk_size) {
        let id_list = chunk.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        session.store(&id_list, "+FLAGS (\\Deleted)")?;
    }
    session.expunge()?;
    println!("Spam folder cleared.");
    Ok(())
}

fn read_line(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap();
    buf.trim().to_string()
}

fn parse_named_arg(flag: &str) -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).cloned()
}

fn parse_days_arg() -> Option<i64> {
    parse_named_arg("--days")?.parse().ok()
}

fn prompt_days() -> i64 {
    loop {
        let input = read_line("Delete messages older than how many days? ");
        if let Ok(n) = input.parse::<i64>() {
            if n > 0 {
                return n;
            }
        }
        println!("Please enter a positive number.");
    }
}

fn main() {
    dotenv().ok();
    let username = env::var("GMAIL_USERNAME").expect("GMAIL_USERNAME not set");
    let password = env::var("GMAIL_APP_PASSWORD").expect("GMAIL_APP_PASSWORD not set");

    println!("Connecting to Gmail (imap.gmail.com)...");

    match connect_to_gmail(&username, &password) {
        Ok(mut session) => {
            // Auto-clean [Gmail]/Spam on every run
            let spam_folders: Vec<String> = match session.list(None, Some("*")) {
                Ok(names) => names
                    .iter()
                    .map(|f| f.name().to_string())
                    .filter(|name| name.to_ascii_lowercase().contains("spam"))
                    .collect(),
                Err(_) => vec![],
            };
            for spam in &spam_folders {
                if let Err(e) = clean_folder_all(&mut session, spam) {
                    eprintln!("Failed to auto-clean '{}': {}", spam, e);
                }
            }
            if spam_folders.is_empty() {
                println!("No spam folder found to auto-clean.");
            }

            let cli_folder = parse_named_arg("--folder");
            let cli_action = parse_named_arg("--action");

            match list_folders(&mut session) {
                Ok(folders) => {
                    let folder_name = if let Some(ref name) = cli_folder {
                        if folders.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                            name.clone()
                        } else {
                            eprintln!("Folder '{}' not found.", name);
                            session.logout().unwrap();
                            return;
                        }
                    } else {
                        let choice = read_line("\nEnter the number of the folder: ");
                        match choice.parse::<usize>() {
                            Ok(index) if index > 0 && index <= folders.len() => folders[index - 1].clone(),
                            _ => { println!("Invalid selection."); session.logout().unwrap(); return; }
                        }
                    };

                    let action = cli_action.unwrap_or_else(|| {
                        read_line("Action — (c)lean old messages, (s)how top senders, (m)ark all read: ")
                    });

                    match action.to_ascii_lowercase().as_str() {
                        "c" => {
                            let days = parse_days_arg().unwrap_or_else(prompt_days);
                            if let Err(e) = cleanup_folder(&mut session, &folder_name, days) {
                                eprintln!("Cleanup failed: {}", e);
                            }
                        }
                        "s" => {
                            if let Err(e) = show_top_senders(&mut session, &folder_name, 25) {
                                eprintln!("Failed to fetch senders: {}", e);
                            }
                        }
                        "m" => {
                            if let Err(e) = mark_all_read(&mut session, &folder_name) {
                                eprintln!("Mark read failed: {}", e);
                            }
                        }
                        _ => println!("Unknown action."),
                    }
                }
                Err(e) => eprintln!("Failed to list folders: {}", e),
            }
            session.logout().unwrap();
        }
        Err(e) => eprintln!("Connection failed: {}", e),
    }
}

