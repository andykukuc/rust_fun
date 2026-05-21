# Yahoo Mail Cleanup

A command-line utility written in Rust that connects to a Yahoo Mail account via IMAP and helps manage email folders interactively.

## Features

- Connects to `imap.mail.yahoo.com:993` over TLS
- Auto-cleans the Bulk folder on every run
- Lists all folders with message counts and total mailbox size
- Interactive folder selection with three actions:
  - **(c) Clean** — delete messages older than N days, or type `all` to clear the entire folder
  - **(s) Top senders** — show the top 25 senders by message count
  - **(m) Mark all read** — mark every message in the folder as read
- Dynamic chunk sizing for IMAP commands — balances performance and server session safety
- Prints a dry-run summary (message count, chunk size, iterations) before any delete operation
- Credentials loaded from `.env`, never stored in source

## Setup

1. Add your credentials to a `.env` file in the project root:

```
YAHOO_USERNAME=your_email@yahoo.com
YAHOO_APP_PASSWORD=your_app_password
```

> You must use a Yahoo **App Password**, not your regular login password. Generate one at:
> Yahoo Account Info → Account Security → Generate app password

2. Build and run:

```sh
cargo run
```

## CLI Flags

| Flag | Description |
|------|-------------|
| `--folder <name>` | Skip folder selection, use this folder directly |
| `--action <c\|s\|m>` | Skip action prompt |
| `--days <n\|all>` | Skip days prompt (use with `--action c`) |

## Dependencies

- `imap` — IMAP protocol
- `native-tls` — TLS connection
- `dotenv` — `.env` credential loading
- `chrono` — date calculations

---

> **Warning:** Delete operations are permanent and cannot be undone. Always verify the folder and day count before confirming.
