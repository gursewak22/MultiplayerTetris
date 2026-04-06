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

sleep 1

echo "Starting Player 1..."
osascript -e 'tell app "Terminal" to do script "cd '"$(pwd)"' && cargo run --bin client"'

sleep 1

echo "Starting Player 2..."
osascript -e 'tell app "Terminal" to do script "cd '"$(pwd)"' && cargo run --bin client"'

echo "All started. Check the Terminal windows."
