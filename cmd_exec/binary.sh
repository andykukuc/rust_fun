#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Variables
APP_NAME="cmd_exec"
BUILD_DIR="target/x86_64-unknown-linux-musl/release"
DEPLOY_DIR="cmd_exec_deploy"
FRONTEND_DIR="frontend"
ARCHIVE_NAME="${APP_NAME}_deploy.tar.gz"

# Step 1: Install dependencies for building a static binary
echo "Installing dependencies for building a static binary..."
sudo apt-get update && sudo apt-get install -y musl-tools || sudo dnf install -y musl-gcc

# Step 2: Build the Rust application as a static binary
echo "Building the Rust application as a static binary..."
cargo build --release --target x86_64-unknown-linux-musl

# Step 3: Create the deployment directory
echo "Creating the deployment directory..."
rm -rf $DEPLOY_DIR
mkdir -p $DEPLOY_DIR

# Step 4: Copy the compiled binary to the deployment directory
echo "Copying the compiled binary to the deployment directory..."
cp $BUILD_DIR/$APP_NAME $DEPLOY_DIR/

# Step 5: Copy the frontend files to the deployment directory
echo "Copying the frontend files to the deployment directory..."
cp -r $FRONTEND_DIR $DEPLOY_DIR/

# Step 6: Package the deployment directory into a tar.gz archive
echo "Packaging the deployment directory into a tar.gz archive..."
tar -czvf $ARCHIVE_NAME $DEPLOY_DIR

# Step 7: Print success message
echo "Build and packaging complete!"
echo "Deployment archive created: $ARCHIVE_NAME"