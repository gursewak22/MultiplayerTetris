#!/bin/bash
set -e

# --- CONFIGURATION ---
TARGET_IP="192.168.137.1" # Replace with your specific IP
PORT="8080"
# ---------------------

cd "$(dirname "$0")"

echo "Building..."
cargo build 2>&1

echo "Starting server..."
# Pass the IP as an argument if your Rust code supports it, 
# or ensure the Rust code is hardcoded to 0.0.0.0
mintty -h always -e sh -c "cargo run --bin server" &

sleep 1

echo "Starting renderer..."
mintty -h always -e sh -c "cargo run --bin renderer" &

sleep 2

echo "Opening game at http://$TARGET_IP:$PORT..."
cygstart "http://$TARGET_IP:$PORT"

echo ""
echo "Server and renderer are running."
echo "Others can join at http://$TARGET_IP:$PORT"