---
title: "HW1 — Tetris Architecture Document"
subtitle: "CSS 553 — Handed out Session 2 of 20 — Due Wednesday, April 8"
version: "1.2"
status: draft
created_by: "Claude"
created_at: "2026-03-29"
last_modified_by: "Claude"
last_modified_at: "2026-03-29"
contributors:
  - "Dr. Marcel Gavriliu"
  - "Claude"
tags:
  - "css553"
  - "homework"
  - "week-1"
related:
  - path: "../WK01_M/ICA1 - Tetris Architecture.md"
    desc: "In-class activity where teams committed to an architecture"
  - path: "./L1e - Basic Concepts.py"
    desc: "Lecture introducing components, connectors, and NFPs used in the architecture document"
---

# HW1 - Tetris Architecture Document

**Due:** Wednesday, April 8 (WK02_W) by 5:45 PM

## Context

Your team committed to an architecture during Monday's in-class activity. Now you extend it.

**The constraint:** you must stick to the architecture you chose in class. No redesigning from scratch.

## Base Requirements (from ICA1)

- Two players, each with their own random piece sequence (no save mechanic)
- Real-time opponent board view
- Each player sees opponent's board and next piece in real time
- Race format: first player whose board overflows loses
- Piece swap (10s cooldown, no immediate reswap): your current piece and opponent's next piece trade places (you can see what you're getting)
- Challenge system (invite, accept, decline)
- Persistent ranking system
- Lobby (online players, rankings, challenge initiation)

## Extensions

Your architecture must now support two new features:

- **Spectating** — any player can watch a live match in progress
- **Tournaments** — 8-player single-elimination bracket with automatic progression

## Implementation

You are encouraged to use AI to write code. **Use a language or stack unfamiliar to all team members.** The game logic, rendering, and boilerplate are not the point — the component boundaries, communication patterns, and wiring are.

## Deliverable

**Architecture document (1-2 pages)** containing:

1. **Diagram** of the architecture committed to in class
2. **Component descriptions** — what each component does, what it depends on, what it provides
3. **Extension analysis** — how you extended for spectating and tournaments; what components were added, what connections changed
4. **Friction points** — where the architecture helped and where it fought you
5. **NFP tradeoffs** — what non-functional properties your architecture optimizes for and what it sacrifices
6. **Retrospective** — what you would change if you started over, and why
7. **Code repo** (optional) — link to repo if you have one. Running code is not required. If code diverges from the architecture document, note where and why.

## Grading

**Completion-based.** Credit is awarded for architectural reasoning, not code quality.

A team that explains clearly why spectating was impossible with their peer-to-peer design gets full credit.

**What we look at:** quality of architectural reasoning. Teams that deliver running code create an additional learning opportunity — does the code match the architecture? If not, that divergence is called **drift**, and it is worth discussing.

---

## Change Log

| Version | Date | Author | Summary |
| ------- | ---- | ------ | ------- |
| 1.2 | 2026-03-29 | Claude | Moved to WK01_W (handed out Wednesday, not Monday). Swap rules: 10s cooldown + no immediate reswap (replaces limited uses). |
| 1.1 | 2026-03-29 | Claude | Updated base requirements: race format, independent sequences, no save, no garbage rows |
| 1.0 | 2026-03-29 | Claude | Initial draft |
