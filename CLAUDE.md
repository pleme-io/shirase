# Shirase (知らせ) — Notification Center

## Build & Test

```bash
cargo build                    # compile
cargo test --lib               # unit tests
cargo test                     # all tests
cargo run                      # launch GUI
cargo run -- daemon            # start notification daemon
cargo run -- history           # show notification history
cargo run -- clear             # clear all notifications
cargo run -- dnd on            # enable do-not-disturb
cargo run -- dnd off           # disable do-not-disturb
```

## Architecture

### Pipeline

```
System Notifications ──subscribe──▸ NotificationSource
                                         ↓
                              Filter (blocked, DND, quiet hours)
                                         ↓
                              Display (popup queue, grouping)
                                         ↓
                              History (SQLite persistence)
```

### Configuration

Uses **shikumi** for config discovery and hot-reload:
- Config file: `~/.config/shirase/shirase.yaml`
- Env override: `$SHIRASE_CONFIG`
- Env vars: `SHIRASE_` prefix (e.g. `SHIRASE_BEHAVIOR__AUTO_DISMISS_SECS=10`)
- Hot-reload on file change (nix-darwin symlink aware)

### Platform Isolation (`src/platform/`)

| Trait | macOS Impl | Purpose |
|-------|------------|---------|
| `NotificationSource` | `MacOSNotificationSource` | Subscribe to system notifications |
| `NotificationStream` | `MacOSNotificationStream` | Stream of incoming notifications |

Linux implementations will be added under `src/platform/linux/`.

### Config Struct (`src/config.rs`)

| Section | Fields |
|---------|--------|
| `appearance` | `width`, `max_visible`, `opacity`, `position`, `animation_ms` |
| `behavior` | `auto_dismiss_secs`, `do_not_disturb`, `group_by_app`, `sound_enabled` |
| `filters` | `blocked_apps`, `priority_apps`, `quiet_hours.{start, end}` |
| `history` | `max_entries`, `retention_days` |
| `daemon` | `enable`, `listen_addr`, `socket_path` |

## File Map

| Path | Purpose |
|------|---------|
| `src/config.rs` | Config struct (uses shikumi) |
| `src/platform/mod.rs` | Platform trait definitions + `NotificationSource` |
| `src/platform/macos/mod.rs` | macOS notification source backend |
| `src/main.rs` | CLI entry point (clap subcommands) |
| `src/lib.rs` | Library root (re-exports config + platform) |
| `module/default.nix` | HM module with typed options + YAML generation |
| `flake.nix` | Nix flake (packages, overlay, HM module, devShell) |

## Design Decisions

### Configuration Language: YAML
- YAML is the primary and only configuration format
- Config file: `~/.config/shirase/shirase.yaml`
- Nix HM module generates YAML via `lib.generators.toYAML` from typed options
- `extraSettings` escape hatch for raw attrset merge

### Notification Filtering
- Blocked apps: notifications silently discarded
- Priority apps: always shown, even during do-not-disturb
- Quiet hours: time-based suppression with start/end times
- Group by app: collapse multiple notifications from same source

### Nix Integration
- Flake exports: `packages`, `overlays.default`, `homeManagerModules.default`, `devShells`
- HM module at `blackmatter.components.shirase` with fully typed options
- YAML generated via `lib.generators.toYAML`
- Cross-platform: `mkLaunchdService` (macOS) + `mkSystemdService` (Linux)
- Uses substrate's `hm-service-helpers.nix` for service generation

### Cross-Platform Strategy
- Platform-specific notification access: behind `NotificationSource` trait
- macOS: NSDistributedNotificationCenter / UNUserNotificationCenter
- Linux: (planned) D-Bus org.freedesktop.Notifications
- History storage: local SQLite database
- IPC: Unix domain socket for CLI control
