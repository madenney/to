# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Melee Stream Tool (Next) is a Tauri 2 desktop app for managing Super Smash Bros. Melee streaming setups. It integrates with start.gg tournament brackets, Slippi replays, and Dolphin emulator to manage stream overlays for single or multiple simultaneous gameplay feeds.

## Tech Stack

- **Frontend:** React 19, TypeScript, Vite 7
- **Backend:** Rust with Tauri 2, Axum (HTTP server), Tokio (async)
- **Slippi Parsing:** peppi crate for .slp replay files
- **Window Management:** x11rb for Linux X11 integration

## Commands

```bash
npm run tauri dev       # Full dev mode (React + Tauri backend)
npm run dev             # React frontend only (port 1420)
npm run build           # TypeScript compile + Vite bundle

# Rust-specific
cargo check             # Check Rust code in src-tauri/
cargo build             # Build Rust backend
cargo test              # Run Rust tests
```

## Architecture

### Data Flow
1. User edits in React UI → Tauri commands (invoke)
2. Tauri commands in `lib.rs` → persist to `overlay/state.json`
3. Overlay browsers (OBS sources) fetch `/state.json` periodically
4. Axum HTTP server serves overlays on ports 17890-17893

### Directory Structure
- `src/` - React frontend (TypeScript)
  - `hooks/` - State management (useConfig, useSetups, useStreams, useBracket)
  - `components/` - UI components (MainView, BracketView, modals)
  - `startggAdapter.ts` - Start.gg GraphQL client
- `src-tauri/src/` - Rust backend
  - `lib.rs` - Tauri commands, HTTP server setup
  - `types.rs` - Domain types (Setup, Stream, Entrant)
  - `config.rs` - Config file loading/saving
  - `dolphin.rs` - Dolphin emulator launcher
  - `replay.rs` - Slippi replay parsing
  - `startgg.rs` - Start.gg API integration
  - `startgg_sim.rs` - Tournament simulation for testing
- `overlay/` - Static HTML/CSS served by backend
  - `index.html` - Single setup overlay
  - `dual/`, `quad/` - Multi-setup overlays
  - `state.json` - Current overlay state

### Key Ports
- `1420` - Vite dev server (React UI)
- `17890` - Main overlay
- `17891` - Upcoming match overlay
- `17892` - Dual overlay
- `17893` - Quad overlay

## Configuration

Environment variables (`.env`) or `config.json`:
- `DOLPHIN_PATH` / `dolphinPath` - Slippi Playback AppImage
- `SSBM_ISO_PATH` / `ssbmIsoPath` - Melee ISO path
- `SLIPPI_APPIMAGE_PATH` / `slippiLauncherPath` - Slippi Launcher
- `SPECTATE_FOLDER_PATH` / `spectateFolderPath` - Slippi spectate folder
- `STARTGG_TOKEN` / `startggToken` - Start.gg API token

## Testing

Set `"testMode": true` in config.json to use synthetic bracket data from `test_brackets/`. Utility scripts in `scripts/` can simulate live games and sync brackets.
