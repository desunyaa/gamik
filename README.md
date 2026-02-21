# gamik

A multiplayer roguelike game built with [egui](https://github.com/emilk/egui/) and [iroh](https://github.com/n0-computer/iroh) peer-to-peer networking.

## Architecture

The codebase is split into four independent layers:

| Module | Responsibility |
|--------|----------------|
| **`game`** | Pure, deterministic game state and logic — no UI or networking dependencies. All mutations go through a single `apply()` function for replay-ability. |
| **`net`** | Iroh-based peer-to-peer networking. Defines the wire protocol, server state, and client/server async loops. |
| **`ui`** | Rendering helpers that read `GameState` and produce `egui` visuals. No game logic lives here. |
| **`app`** | Application shell that wires the other three layers together. Manages screens (menus, character/world selection, gameplay) and input handling. |

### Key design choices

- **Deterministic core** — `game::apply()` is the only way to mutate `GameState`. Given identical inputs it always produces identical outputs, making state easy to test and replay.
- **Entity map** — All entities live in an `FxHashMap<EntityID, Entity>`. A spatial index is built per-frame for O(1) rendering lookups.
- **P2P networking** — Uses iroh's encrypted QUIC connections. The server ticks at 50 ms, broadcasting the full entity map to all connected clients.
- **Persistence** — Worlds are serialized with [bitcode](https://github.com/SoftbearStudios/bitcode) and saved as `.world` files.

## Running

### Native

```sh
cargo run --release
```

On Linux you may need:

```sh
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

### Web (WASM)

Requires [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve           # dev server at http://127.0.0.1:8080
trunk build --release # production build → dist/
```

## Controls

| Key | Action |
|-----|--------|
| `W` / `↑` | Move up |
| `A` / `←` | Move left |
| `S` / `↓` | Move down |
| `D` / `→` | Move right |
| `R` | Save world |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
