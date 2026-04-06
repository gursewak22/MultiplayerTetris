#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Building..."
cargo build 2>&1

echo "Starting server..."
osascript -e 'tell app "Terminal" to do script "cd '"$(pwd)"' && cargo run --bin server"'

sleep 1

echo "Starting renderer..."
osascript -e 'tell app "Terminal" to do script "cd '"$(pwd)"' && cargo run --bin renderer"'

sleep 2

echo "Opening game in browser..."
open "http://localhost:8080"

echo ""
echo "Server and renderer are running in Terminal windows."
echo "Open http://localhost:8080 in a second browser tab for Player 2."
