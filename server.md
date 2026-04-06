# Server Logic (Game Rules / State)

## Role

Single source of truth for all game state. Validates every client command, enforces all Tetris rules, manages multiplayer sessions, and pushes draw instructions to the Graphics Renderer each tick.

## Architecture

```
┌─────────────────┐    send commands     ┌─────────────────────────┐
│                 │ ──────────────────►  │                         │
│     Client      │                      │     Server Logic        │
│   (UI / Input)  │                      │  (Game Rules / State)   │
│                 │                      │                         │
└─────────────────┘                      └────────────┬────────────┘
                                                      │
                                                      │ draw instructions
                                                      ▼
                                         ┌────────────────────────┐
                                         │   Graphics Renderer    │
                                         │  (Draw / Frame Output) │
                                         └────────────────────────┘
```

## Responsibilities

### Game State
- Two 10×20 boards (one per player)
- Current piece and next piece per player
- Score and level per player
- Gravity timer and lock delay enforcement
- Swap cooldown: 10-second cooldown, no immediate reswap allowed

### Game Rules
- Collision detection
- Line clear detection and scoring
- Piece locking logic
- Independent 7-bag randomizer per player (fair piece distribution)

### Multiplayer & Social
- Lobby management
- Challenge system: invite, accept, decline
- Persistent player rankings
- Spectator broadcasting
- Tournament brackets: 8-player single elimination with automatic progression

## Rust Crate

- **Type:** Async server binary
- **Key dependencies:**
  - `tokio` — async runtime
  - `tokio-tungstenite` — WebSocket server
  - `serde` / `serde_json` — message serialization
  - A persistence layer (e.g. `sqlx` + SQLite or PostgreSQL) for rankings

## WebSocket Endpoints

| Path | Direction | Description |
|------|-----------|-------------|
| `/client` | Client → Server | Receives input commands from players |
| `/renderer` | Server → Renderer | Pushes draw instruction frames |
| `/spectate` | Server → Spectator | Broadcasts game state to spectators |

## Draw Instruction Format (sent to Renderer each tick)

```json
{
  "type": "draw",
  "board_p1": [[0,0,...], ...],
  "board_p2": [[0,0,...], ...],
  "piece_p1": { "type": "T", "x": 4, "y": 0, "rotation": 0 },
  "piece_p2": { "type": "L", "x": 3, "y": 2, "rotation": 1 },
  "next_p1": "S",
  "next_p2": "I",
  "score_p1": 1200,
  "score_p2": 800,
  "swap_cooldown_p1": 7.3,
  "swap_cooldown_p2": 0.0,
  "animations": []
}
```

## Hosting / Running the Server

### Prerequisites

- Rust toolchain (`rustup`, `cargo`) — install from https://rustup.rs
- (Optional) PostgreSQL or SQLite for persistent rankings

### Build

```bash
cargo build --release --bin server
```

### Run

```bash
# Default: listen on 0.0.0.0:9001
./target/release/server

# Custom port
./target/release/server --port 9001

# With a database URL for persistent rankings
DATABASE_URL=sqlite://tetris.db ./target/release/server
```

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `--port` | `9001` | Port to listen for client WebSocket connections |
| `--renderer-port` | `9002` | Port to push draw instructions to the Renderer |
| `DATABASE_URL` | in-memory | Persistence backend for rankings |
| `--tick-rate` | `60` | Game ticks per second |

### Startup Order

Start the Server **before** the Client or Renderer — both connect to it.

```bash
./target/release/server &
```

### Running in Production

Use a process manager to keep the server alive:

```bash
# systemd unit (Linux)
[Unit]
Description=Multiplayer Tetris Server

[Service]
ExecStart=/path/to/server --port 9001
Restart=always

[Install]
WantedBy=multi-user.target
```

Or with Docker:

```dockerfile
FROM rust:1.78 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin server

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/server /usr/local/bin/server
EXPOSE 9001 9002
CMD ["server"]
```

### Troubleshooting

- **Port already in use:** Check `lsof -i :9001` and kill the conflicting process.
- **Rankings not saving:** Verify `DATABASE_URL` is set and the database file/server is accessible.
- **Clients not connecting:** Ensure the firewall allows inbound traffic on port 9001.
