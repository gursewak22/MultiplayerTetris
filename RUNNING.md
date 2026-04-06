# Running Multiplayer Tetris

## Prerequisites

- Rust toolchain: https://rustup.rs
- A modern web browser (Chrome, Firefox, Safari, Edge)
- 2 terminal windows

## Build

```bash
cd MultiplayerTetris
cargo build
```

## Start Order

Open 2 terminals in the project root. Always start in this order:

### Terminal 1 — Server
```bash
cargo run --bin server
```
Listens on:
- `:9001` — browser client connections
- `:9002` — renderer connection

### Terminal 2 — Renderer
```bash
cargo run --bin renderer
```
Connects to server on `:9002`.  
Serves:
- `http://localhost:8080` — the game UI (open this in your browser)
- `:9003` — WebSocket for live game frames

## Playing

1. Open **http://localhost:8080** in your browser
2. Enter a name and click **CONNECT**
3. Open a second tab (or a different browser/machine) at the same URL
4. Enter a different name and click **CONNECT**
5. Player 1: click **CHALLENGE** next to Player 2's name
6. Player 2: click **ACCEPT** in the challenge notification
7. Both players are now in the game!

## Controls

| Key | Action |
|-----|--------|
| `←` / `→` | Move left / right |
| `↑` | Rotate clockwise |
| `Z` | Rotate counter-clockwise |
| `Space` | Hard drop |
| `↓` | Soft drop |
| `C` | Hold / swap piece (10s cooldown) |
| `Q` | Quit / return to lobby |

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `Connection failed` | Start the server before opening the browser |
| `Address already in use` | Kill leftover processes: `lsof -i :9001` then `kill <pid>` |
| Blank game screen | Confirm the renderer started and is connected to the server |
| Controls not working | Click anywhere on the game page to ensure it has keyboard focus |

## Legacy Terminal Client

The old terminal client (`cargo run --bin client`) still compiles but is no longer the default way to play. It expects the old ANSI frame format which the renderer no longer sends.
