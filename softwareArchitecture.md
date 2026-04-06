# HW1 — Tetris Architecture Design

## Architecture Diagram

```
┌─────────────────┐    send commands     ┌─────────────────────────┐
│                 │ ──────────────────►  │                         │
│     Client      │                      │     Server Logic        │
│   (UI / Input)  │                      │  (Game Rules / State)   │
│                 │                      │                         │
└────────┬────────┘                      └────────────┬────────────┘
         ▲                                            │
         │                                            │ compute moves /
         │                                            │ update state
         │                                            ▼
         │                               ┌────────────────────────┐
         │                               │   Graphics Renderer    │
         │◄───── frame output ──────────│  (Draw / Frame Output) │
         │                               │                        │
         │                               └────────────────────────┘
```

## Components

### Client (UI / Input)

Intentionally thin. Captures keyboard input (move left, move right, rotate, hard drop, soft drop, swap request) and forwards commands to the Server over WebSocket. Receives rendered frames from the Graphics Renderer and displays them. Contains zero game logic and zero rendering logic.

**Rust crate:** standalone binary. Uses `tungstenite` or `tokio-tungstenite` for WebSocket communication. Listens for keyboard events and serializes them as messages to the Server.

### Server Logic (Game Rules / State)

Single source of truth. Owns all game state: two 10×20 boards, current and next piece per player, score, gravity timer, swap cooldown (10s, no immediate reswap). Enforces all Tetris rules (collision, line clears, lock delay). Manages piece generation (independent 7-bag randomizer per player). Handles lobby, challenge system (invite/accept/decline), persistent rankings, spectator broadcasting, and tournament brackets (8-player single elimination with automatic progression).

**Rust crate:** async server using `tokio` + `tokio-tungstenite`. Maintains game sessions, validates every client command against game state, and pushes draw instructions to the Graphics Renderer at each tick.

### Graphics Renderer (Draw / Frame Output)

Separated from the Client to keep the Client simple. Receives draw instructions (board grid, piece positions, opponent board, score, cooldown timers, animation triggers) and produces visual frames. Contains zero game logic and zero input handling.

**Rust crate:** uses a rendering backend such as `wgpu`, `macroquad`, `sdl2`, or `crossterm` (terminal). The Renderer is a pure function of the draw instructions it receives — swapping the rendering backend does not affect the Client or Server.