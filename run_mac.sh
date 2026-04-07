#!/bin/bash
set -e

# --- CONFIGURATION ---
TARGET_IP="localhost" # Replace with your specific IP
PORT="8080"
# ---------------------

cd "$(dirname "$0")"

echo "Building..."
cargo build 2>&1

echo "Starting server..."
# Opens a new Terminal.app window on macOS and runs the server
osascript -e "tell app \"Terminal\" to do script \"cd '$(pwd)' && cargo run --bin server\""

sleep 1

echo "Starting renderer..."
# Opens a new Terminal.app window on macOS and runs the renderer
osascript -e "tell app \"Terminal\" to do script \"cd '$(pwd)' && cargo run --bin renderer\""

sleep 2

echo "Opening game at http://$TARGET_IP:$PORT..."
open "http://$TARGET_IP:$PORT"

echo ""
echo "Server and renderer are running."
echo "Others can join at http://$TARGET_IP:$PORT"
