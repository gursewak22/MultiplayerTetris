# Running Multiplayer Tetris

## Prerequisites

- Rust toolchain: https://rustup.rs
- 4 terminal windows

## Build

```bash
cd MultiplayerTetris
cargo build
```

## Start Order

Open 4 terminals in the project root. Always start in this order:

### Terminal 1 — Server
```bash
cargo run --bin server
```
Listens on:
- `:9001` — client connections
- `:9001/render` — renderer connection

### Terminal 2 — Renderer
```bash
cargo run --bin renderer
```
Connects to server on `:9001`, serves frames to clients on `:9002`.

### Terminal 3 — Player 1
```bash
cargo run --bin client
```

### Terminal 4 — Player 2
```bash
cargo run --bin client
```

## Controls

| Key | Action |
|-----|--------|
| `←` / `→` | Move left / right |
| `↑` or `Z` | Rotate clockwise / counter-clockwise |
| `Space` | Hard drop |
| `↓` | Soft drop |
| `C` | Hold / swap piece (10s cooldown) |
| `Q` / `Ctrl+C` | Quit |

## Lobby Commands

After connecting, type your name and press Enter to join the lobby. Then:

| Command | Action |
|---------|--------|
| `challenge <name>` | Challenge another player |
| `accept <name>` | Accept an incoming challenge |
| `decline <name>` | Decline an incoming challenge |

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `Connection refused` | Start the server before the renderer and clients |
| `Address already in use` | Kill any leftover process: `lsof -i :9001` then `kill <pid>` |
| Garbled display | Make sure your terminal supports ANSI colors and is at least 80 columns wide |
| Client shows nothing | Confirm the renderer started successfully in Terminal 2 |
