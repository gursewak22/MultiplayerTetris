# Client (UI / Input)

## Role

Intentionally thin. Captures keyboard input and forwards commands to the Server over WebSocket. Receives rendered frames from the Graphics Renderer and displays them. Contains **zero game logic** and **zero rendering logic**.

## Architecture

```
┌─────────────────┐    send commands     ┌─────────────────────────┐
│                 │ ──────────────────►  │                         │
│     Client      │                      │     Server Logic        │
│   (UI / Input)  │                      │  (Game Rules / State)   │
│                 │ ◄── frame output ─── │                         │
└─────────────────┘                      └─────────────────────────┘
```

## Responsibilities

- Listen for keyboard events: move left, move right, rotate, hard drop, soft drop, swap request
- Serialize input events and send them to the Server via WebSocket
- Receive rendered frame data from the Graphics Renderer and display to the user
- No validation, no state, no rendering calculations

## Rust Crate

- **Type:** Standalone binary
- **Key dependencies:**
  - `tokio` — async runtime
  - `tokio-tungstenite` or `tungstenite` — WebSocket communication
  - A terminal or windowing input library (e.g. `crossterm` for keyboard events)

## Message Format (outgoing to Server)

```json
{ "type": "input", "action": "move_left" }
{ "type": "input", "action": "move_right" }
{ "type": "input", "action": "rotate_cw" }
{ "type": "input", "action": "hard_drop" }
{ "type": "input", "action": "soft_drop" }
{ "type": "input", "action": "swap" }
```

## Message Format (incoming from Renderer)

```json
{ "type": "frame", "data": { ... } }
```

## Hosting / Running the Client

### Prerequisites

- Rust toolchain (`rustup`, `cargo`) — install from https://rustup.rs
- A running Server instance (see `server.md`)
- A running Renderer instance (see `renderer.md`)

### Build

```bash
# From the project root
cargo build --release --bin client
```

### Run

```bash
# Connect to server at localhost:9001
./target/release/client --server ws://127.0.0.1:9001

# Or with environment variable
SERVER_URL=ws://127.0.0.1:9001 ./target/release/client
```

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `ws://127.0.0.1:9001` | WebSocket URL of the Server |
| `--renderer` | `ws://127.0.0.1:9002` | WebSocket URL of the Renderer (for frame display) |

### Startup Order

1. Start the Server first (`server.md`)
2. Start the Renderer (`renderer.md`)
3. Start the Client last — it connects to both

### Troubleshooting

- **Connection refused:** Ensure the Server is running and the port matches.
- **No input response:** Check that the terminal has focus and raw mode is enabled.
- **Blank screen:** Confirm the Renderer is running and the frame feed URL is correct.
