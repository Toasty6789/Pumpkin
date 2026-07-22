# Changelog

All notable changes to Pumpkin will be documented in this file.

## [1.0.0] - 2026-07-21

### Initial Stable Release

This is the first stable release of Pumpkin, a fast and efficient Minecraft server
written entirely in Rust. After years of development, Pumpkin 1.0.0 is ready for
production use.

### Networking & Protocol

- Full Java Edition protocol support (1.21 - 1.26)
- Bedrock Edition support (W.I.P)
- AES/CFB8 encryption with RSA-1024 handshake
- Packet compression (Zlib/Deflate)
- RakNet base protocol for Bedrock
- Protocol fuzzing and unit tests for Java Edition

### World

- World loading and saving (Overworld, Nether, End)
- Chunk loading with multiple strategies (Vanilla, Linear, Pump)
- Chunk generation with vanilla parity
- Flat world generation
- Lighting engine
- Liquid physics (Water, Lava)
- World borders and world time
- Redstone engine (core logic, power sensors, logic/timing, mechanisms)
- Entity spawning

### Player Features

- Player skins, teleport, movement, and animations
- Inventory management with off-hand support
- Combat system (melee, bows, crossbows, shields, tridents)
- Critical hits, damage invulnerability frames, sweep attacks
- Enchantment and status effect integration
- Experience, hunger, and eating systems
- Advancements and bossbars
- Scoreboard and player tab-list

### Entities

- Non-living entities (Minecarts, projectiles, etc.)
- Entity effects and status effects
- Mob AI with A* pathfinding and goal selector system
- Animals, villagers, and bosses (W.I.P)
- Entity saving and persistence

### Commands

- Brigadier-based command system
- 80+ vanilla commands implemented
- Custom `/pumpkin` command
- Console, RCON, and player command senders
- Per-command permissions and configurability

### Plugin API

- Native plugin loading via libloading
- Plugin discovery and dependency resolution
- Plugin lifecycle management (on_load, on_enable, on_disable)
- Command registration API
- Event system with internal dispatcher
- Centralized logging API for plugins
- Async/sync task scheduler
- WASM plugin support via wasmtime

### Server Features

- Configuration via TOML
- Server status/ping
- Query and RCON support
- Chat system
- Permission system
- Translations
- BungeeCord and Velocity proxy support
- Particles system

### Performance

- Multi-threaded architecture leveraging Rayon and Tokio
- LTO and stripped release builds
- Memory-efficient data structures
- Tick loop designed for 20 TPS

### Supported Platforms

- Windows (10+)
- Linux
- macOS
- FreeBSD
- Android (API level 21)

### Supported Architectures

- x86_64
- ARM64

---

## Remaining Items (Post-1.0.0 Roadmap)

See [Issue #449](https://github.com/Pumpkin-MC/Pumpkin/issues/449) for the full roadmap.
The following items are planned for 1.0.x patch releases:

- Stress testing (1000+ simulated players)
- Memory leak audit
- Tick loop consistency under load
- Cross-platform verification on all guaranteed OSes
- Removal of unwrap/expect from critical paths
- Community test run / private alpha testing
