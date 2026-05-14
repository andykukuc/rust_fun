# Command Executor Deployment Script

This project provides a Rust-based command execution application with a web-based frontend. The included shell script, `build_and_package.sh`, automates the process of building, packaging, and preparing the application for deployment on any Linux server, even those without internet access or Rust installed.

## Features

- **Rust Backend**: Executes Linux commands and serves API endpoints.
- **Web-Based Frontend**: A simple interface to interact with the backend.
- **Self-Contained Deployment**: Packages the application into a single archive for easy deployment.
- **No Dependencies on Target Server**: Runs on any Linux distribution without requiring Rust or internet access.

## What the Script Does

The `build_and_package.sh` script performs the following steps:

1. **Builds a Static Binary**:
   - Compiles the Rust backend into a self-contained static binary using `musl-tools`.
   - Ensures the binary can run on any Linux distribution without requiring Rust or other dependencies.

2. **Packages the Application**:
   - Copies the compiled binary and the `frontend` directory (containing `index.html`, `style.css`, and `index.js`) into a deployment directory.
   - Compresses the deployment directory into a `.tar.gz` archive for easy transfer.

3. **Prepares for Offline Deployment**:
   - The resulting archive can be deployed to a server without internet access.

## How to Use the Script

### Prerequisites

- **Rust Installed**: Ensure Rust is installed on the build machine. Install it using:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **Build Tools**: Install `musl-tools` for building a static binary:
  ```bash
  sudo apt-get install musl-tools  # For Debian/Ubuntu
  sudo dnf install musl-gcc        # For Fedora
  ```

### Steps to Build and Package

1. Clone the repository:
   ```bash
   git clone <repository-url>
   cd cmd_exec
   ```

2. Run the script:
   ```bash
   ./build_and_package.sh
   ```

3. The script will generate a `cmd_exec_deploy.tar.gz` file in the project directory.

### Deploying the Application

1. Transfer the `cmd_exec_deploy.tar.gz` file to the target server:
   ```bash
   scp cmd_exec_deploy.tar.gz user@<server-ip>:/path/to/deploy
   ```

2. Extract the archive on the target server:
   ```bash
   tar -xzvf cmd_exec_deploy.tar.gz
   cd cmd_exec_deploy
   ```

3. Run the application:
   ```bash
   ./cmd_exec --port 8080
   ```

4. Access the application in a browser:
   ```
   http://<server-ip>:8080
   ```

## Use Cases

- **Offline Deployment**:
  - Deploy the application to servers without internet access.
- **Cross-Distribution Compatibility**:
  - Run the application on any Linux distribution without installing Rust or other dependencies.
- **Simplified Deployment**:
  - Package the backend and frontend into a single archive for easy transfer and deployment.

## Notes

- Ensure the target server has the required port (e.g., `8080`) open in its firewall.
- If deploying on a server with `systemd`, consider creating a systemd service to run the application automatically on boot.

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.

## Acknowledgments

- Built with Rust and the Warp framework.
- Inspired by the need for lightweight, portable web applications.