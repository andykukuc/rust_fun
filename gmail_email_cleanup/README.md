# Rust Email Cleanup Utility

A simple command-line utility written in Rust to connect to a Yahoo Mail account via IMAP and delete old emails from a specified folder.

## Features

- Securely connects to Yahoo's IMAP server (`imap.mail.yahoo.com:993`) using TLS.
- Lists all available mail folders/labels in your account.
- Prompts the user to interactively select a folder for cleanup.
- Deletes emails older than a specified number of days (currently hardcoded to 30 days).
- Uses a `.env` file to manage credentials securely, keeping them out of the source code.

## Prerequisites

- Rust and Cargo must be installed.
- A Yahoo Mail account.

## Setup & Installation

1.  **Clone the repository:**
    ```sh
    git clone <your-repository-url>
    cd rust_email_cleanup
    ```

2.  **Create a `.env` file:**
    In the root of the project directory, create a file named `.env`.

3.  **Add your credentials to the `.env` file:**
    ```
    YAHOO_USERNAME="your_yahoo_email@yahoo.com"
    YAHOO_APP_PASSWORD="your_yahoo_app_password"
    ```

    > **IMPORTANT:** You must generate an **App Password** from your Yahoo account security settings. Your regular login password will not work due to modern security practices (2FA/MFA). You can usually generate one by going to your Yahoo Account Info -> Account Security -> Generate app password.

## Usage

1.  **Build and run the project with Cargo:**
    ```sh
    cargo run
    ```

2.  The application will connect to your account and display a numbered list of your mail folders.

3.  Enter the number corresponding to the folder you wish to clean up and press `Enter`.

4.  The application will find and delete emails older than 30 days from the selected folder.

## Key Dependencies

- `imap`: For IMAP communication with the mail server.
- `native-tls`: For creating a secure TLS connection.
- `dotenv`: For loading environment variables from the `.env` file.
- `chrono`: For handling date and time calculations to find old emails.

---

> **⚠️ WARNING:** This tool permanently deletes emails by moving them to the trash and expunging them. Use it with caution. It is highly recommended to test it on a non-critical folder or a test account first. The author is not responsible for any data loss.

