# Graphics Renderer (Draw / Frame Output)

## Role

Receives draw instructions from the Server and produces visual frames for the Client to display. Contains **zero game logic** and **zero input handling**. The Renderer is a pure function of the draw instructions it receives.

## Architecture

```
┌────────────┬────────────┐
│   Server   │            │
│  (pushes   │            │
│   draw     │            │
│   instrs)  │            │
└────────────┘            │
      │                   │
      ▼                   │
┌────────────────────────┐│
│   Graphics Renderer    ││
│  (Draw / Frame Output) ││
└────────────┬───────────┘│
             │             │
             │ frame output│
             ▼             │
      ┌─────────────┐      │
      │   Client    │◄─────┘
      │  (display)  │
      └─────────────┘
```

## Responsibilities

- Receive draw instruction payloads from the Server via WebSocket
- Render visual frames from those instructions:
  - 10×20 board grid for each player
  - Active piece positions and ghost piece
  - Opponent board (smaller, side panel)
  - Score and level display
  - Swap cooldown timers
  - Animation triggers (line clear flash, piece lock, etc.)
- Push completed frames to the Client
- Swapping the rendering backend does **not** affect the Client or Server

## Rust Crate

- **Type:** Standalone binary
- **Key dependencies:**
  - `tokio` + `tokio-tungstenite` — WebSocket communication with Server and Client
  - One rendering backend (choose one):

| Backend | Use case |
|---------|----------|
| `crossterm` | Terminal / CLI rendering |
| `macroquad` | Simple 2D window (easy setup) |
| `sdl2` | Cross-platform window with hardware acceleration |
| `wgpu` | GPU-accelerated, most flexible |

## Draw Instruction Input Format (from Server)

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

## Frame Output Format (to Client)

```json
{ "type": "frame", "data": { "pixels": [...] } }
```

For terminal backends, `data` may contain ANSI escape sequences instead of raw pixels.

## Hosting / Running the Renderer

### Prerequisites

- Rust toolchain (`rustup`, `cargo`) — install from https://rustup.rs
- A running Server instance (see `server.md`)
- For `sdl2` backend: `libsdl2-dev` on Linux / SDL2 framework on macOS
- For `wgpu` backend: a GPU with Vulkan / Metal / DX12 support

### Install System Dependencies (if using sdl2)

```bash
# Ubuntu / Debian
sudo apt install libsdl2-dev

# macOS (Homebrew)
brew install sdl2

# Windows: download SDL2 dev libraries from https://libsdl.org
```

### Build

```bash
# Terminal backend (no system deps needed)
cargo build --release --bin renderer --features crossterm

# SDL2 backend
cargo build --release --bin renderer --features sdl2

# macroquad backend
cargo build --release --bin renderer --features macroquad
```

### Run

```bash
# Connect to server and serve frames to clients
./target/release/renderer \
  --server ws://127.0.0.1:9001 \
  --listen 0.0.0.0:9002

# Terminal mode (no window)
./target/release/renderer --backend terminal --server ws://127.0.0.1:9001
```

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `--server` | `ws://127.0.0.1:9001` | WebSocket URL to receive draw instructions from |
| `--listen` | `0.0.0.0:9002` | Address to serve frame output to the Client |
| `--backend` | `crossterm` | Rendering backend: `crossterm`, `macroquad`, `sdl2`, `wgpu` |
| `--fps` | `60` | Target render frame rate |

### Startup Order

1. Start the **Server** first
2. Start the **Renderer** — it connects to the Server to receive draw instructions
3. Start the **Client** last — it connects to the Renderer to display frames

### Swapping Backends

Because the Renderer is a pure function of draw instructions, you can switch backends without touching the Server or Client:

```bash
# Switch from terminal to sdl2
./target/release/renderer --backend sdl2 --server ws://127.0.0.1:9001
```

### Troubleshooting

- **No frames rendered:** Confirm the Server is running and sending draw instructions on port 9001.
- **SDL2 not found:** Install `libsdl2-dev` (Linux) or `brew install sdl2` (macOS).
- **Blank window:** Check that `--fps` is not set to 0 and that the Server tick rate is > 0.
- **Terminal flickering:** Enable double-buffered output in the `crossterm` backend config.
