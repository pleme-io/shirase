# Shirase (知らせ) — GPU Notification Center

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


Crate: `shirase` | Binary: `shirase` | Config app name: `shirase`

GPU-rendered notification center with rule-based filtering, persistent history,
Do Not Disturb scheduling, and cross-platform notification interception. Acts as
both a notification daemon (popup display) and a notification center (history panel).

## Build & Test

```bash
cargo build                    # compile
cargo test --lib               # unit tests
cargo test                     # all tests
cargo run                      # launch GUI (notification center panel)
cargo run -- daemon            # start notification daemon (intercepts + displays)
cargo run -- history           # show notification history (CLI)
cargo run -- clear             # clear all notifications
cargo run -- dnd on            # enable do-not-disturb
cargo run -- dnd off           # disable do-not-disturb
cargo run -- send "Title" "Body" --urgency high  # send test notification
```

Nix build:
```bash
nix build                     # build via substrate rust-tool-release-flake
nix run                       # run
nix run .#regenerate           # regenerate Cargo.nix after Cargo.toml changes
```

## Competitive Position

| Competitor | Stack | Our advantage |
|-----------|-------|---------------|
| **dunst** | C, X11/Wayland | GPU-rendered, MCP-drivable, Rhai scripting, rich rule engine |
| **mako** | C, Wayland-only | Cross-platform (macOS + Linux), notification center, MCP |
| **swaync** | C, Wayland-only | Cross-platform, GPU rendering, Rhai automation |
| **fnott** | C, Wayland-only | Cross-platform, full notification center, MCP-drivable |

Unique value: GPU notification center with MCP for AI notification management,
Rhai rule automation, cross-platform (macOS + Linux), and persistent searchable history.

## Architecture

### Module Map

```
src/
  main.rs                      ← CLI entry point (clap: open, daemon, history, clear, dnd, send)
  lib.rs                       ← Library root (re-exports config + platform)
  config.rs                    ← ShiraseConfig via shikumi

  platform/
    mod.rs                     ← Platform trait definitions (NotificationSource, NotificationStream)
    macos/
      mod.rs                   ← macOS notification source (NSUserNotificationCenter / UNUserNotification)
    linux/                     ← (planned)
      mod.rs                   ← D-Bus org.freedesktop.Notifications server

  notifications/               ← (planned) Core notification management
    mod.rs                     ← NotificationManager: receive, queue, dispatch, history
    notification.rs            ← Notification struct (id, app, title, body, urgency, icon, actions, timestamp)
    queue.rs                   ← Display queue (FIFO, max visible, dismiss timer)
    group.rs                   ← Grouping logic (by app, by urgency, by tag)

  rules/                       ← (planned) Rule engine
    mod.rs                     ← RuleEngine: evaluate rules against incoming notifications
    rule.rs                    ← Rule struct (match criteria + action)
    criteria.rs                ← Match criteria (app name, title regex, urgency, time-of-day)
    actions.rs                 ← Rule actions (style, sound, suppress, script, urgency_override)

  history/                     ← (planned) Persistent notification history
    mod.rs                     ← HistoryStore: SQLite-backed persistent history
    schema.rs                  ← Database schema (notifications, dismissed, read_status)
    queries.rs                 ← Query API (by app, by date, search text, unread count)

  dnd/                         ← (planned) Do Not Disturb
    mod.rs                     ← DndManager: state machine, schedules, exceptions
    schedule.rs                ← Recurring DnD schedules (quiet hours, work focus)
    exceptions.rs              ← Priority app exceptions (always show even in DnD)

  popup/                       ← (planned) Notification popup rendering
    mod.rs                     ← PopupManager: position, animation, dismiss
    layout.rs                  ← Popup layout (icon, title, body, actions, progress)
    animation.rs               ← Slide-in, fade-out animations

  render/                      ← (planned) GPU rendering
    mod.rs                     ← ShiraseRenderer: madori RenderCallback
    popup.rs                   ← Popup notification GPU rendering
    center.rs                  ← Notification center panel rendering
    history_view.rs            ← History list with search and filter

  mcp/                         ← (planned) MCP server via kaname
    mod.rs                     ← ShiraseMcp server struct
    tools.rs                   ← Tool implementations

  scripting/                   ← (planned) Rhai scripting via soushi
    mod.rs                     ← Engine setup, shirase.* API registration

module/
  default.nix                  ← HM module (blackmatter.components.shirase)
```

### Data Flow

```
System Notifications
    │
    ├── macOS: NSDistributedNotificationCenter / UNUserNotificationCenter
    └── Linux: D-Bus org.freedesktop.Notifications (shirase IS the server)
    │
    ▼
NotificationSource trait → NotificationStream (async stream of Notification)
    │
    ▼
RuleEngine (evaluate match criteria → determine action)
    │
    ├── Suppressed (blocked app, DnD active, quiet hours) → History only
    │
    ├── Modified (urgency override, custom sound, style change)
    │
    └── Passed through
    │
    ▼
┌───────────────────────────────────────────────┐
│              NotificationManager               │
│                                                │
│  ┌──────────┐  ┌───────────┐  ┌─────────────┐ │
│  │ Popup    │  │ History   │  │ DnD         │ │
│  │ Queue    │  │ Store     │  │ Manager     │ │
│  │ (FIFO)   │  │ (SQLite)  │  │ (schedules) │ │
│  └────┬─────┘  └───────────┘  └─────────────┘ │
│       │                                        │
│       ▼                                        │
│  GPU Render (popup + center panel)             │
└───────────────────────────────────────────────┘
```

### Dual Role: Daemon + Center

Shirase operates in two modes, often simultaneously:

1. **Daemon mode** (`shirase daemon`) — runs as a background service
   - Intercepts system notifications via `NotificationSource`
   - Applies rules, manages DnD
   - Displays popup notifications (GPU-rendered)
   - Persists to history

2. **Center mode** (`shirase` or `shirase open`) — notification center panel
   - Shows grouped notification history
   - Search and filter
   - Dismiss, clear, mark read
   - DnD controls

On macOS, the daemon is a launchd agent. On Linux, it replaces the D-Bus
notification server (shirase implements `org.freedesktop.Notifications`).

### Platform Isolation

| Trait | macOS Implementation | Linux Implementation |
|-------|---------------------|---------------------|
| `NotificationSource` | `MacOSNotificationSource` | `DbusNotificationServer` |
| `NotificationStream` | `MacOSNotificationStream` | `DbusNotificationStream` |

**macOS:** Observes `NSDistributedNotificationCenter` for notifications from other
apps. Cannot fully intercept all notifications (macOS sandboxing limits); uses
`UNUserNotificationCenter` for displaying shirase's own notifications.

**Linux (planned):** Implements the `org.freedesktop.Notifications` D-Bus interface.
Applications send notifications directly to shirase. This gives full control over
all notifications. Register as the notification server in the session.

### Current Implementation Status

**Done:**
- `config.rs` — shikumi integration with appearance/behavior/filters/history/daemon sections
- `platform/mod.rs` — Platform trait definitions (`NotificationSource`, `NotificationStream`)
- `platform/macos/mod.rs` — macOS notification source (basic structure)
- `main.rs` — CLI with open/daemon/history/clear/dnd/send subcommands
- `lib.rs` — Library root
- `module/default.nix` — HM module with typed options + daemon service
- `flake.nix` — substrate rust-tool-release-flake + HM module

**Not started:**
- GUI rendering via madori/garasu/egaku (popups + center panel)
- Notification queue and display management
- Rule engine (match criteria + actions)
- Persistent history (SQLite)
- Do Not Disturb manager (schedules, exceptions)
- Popup animations
- Linux D-Bus notification server
- MCP server via kaname
- Rhai scripting via soushi
- Hotkey system via awase

## Configuration

Uses **shikumi** for config discovery and hot-reload:
- Config file: `~/.config/shirase/shirase.yaml`
- Env override: `$SHIRASE_CONFIG`
- Env prefix: `SHIRASE_` (e.g., `SHIRASE_BEHAVIOR__AUTO_DISMISS_SECS=10`)
- Hot-reload on file change (nix-darwin symlink aware)

### Config Schema

```yaml
appearance:
  width: 400                         # popup width
  max_visible: 5                     # max simultaneous popups
  opacity: 0.95
  position: "top-right"              # top-right | top-left | bottom-right | bottom-left | top-center
  animation_ms: 200                  # slide-in/fade-out duration
  font_size: 13.0
  icon_size: 48

behavior:
  auto_dismiss_secs: 5               # auto-dismiss after N seconds (0 = manual only)
  do_not_disturb: false              # global DnD toggle
  group_by_app: true                 # group notifications from same app
  sound_enabled: true                # play sound on notification
  show_count_badge: true             # show unread count

filters:
  blocked_apps:                      # silently discard notifications from these apps
    - "Finder"
  priority_apps:                     # always show, even during DnD
    - "Calendar"
    - "Messages"
  quiet_hours:
    start: "22:00"                   # suppress from 10 PM
    end: "07:00"                     # until 7 AM

rules:                               # custom rules (evaluated in order, first match wins)
  - name: "Urgent emails"
    match:
      app: "Mail"
      title_regex: "URGENT|ACTION REQUIRED"
    action:
      urgency: "critical"
      sound: "alert"
      bypass_dnd: true

  - name: "Silence Slack threads"
    match:
      app: "Slack"
      body_regex: "thread reply"
    action:
      suppress: true

history:
  max_entries: 10000                 # max history entries (FIFO eviction)
  retention_days: 30                 # auto-delete after N days
  database_path: "~/.local/share/shirase/history.db"

daemon:
  enable: false
  listen_addr: "127.0.0.1:9300"
  socket_path: "/tmp/shirase.sock"   # Unix socket for CLI control
```

## Shared Library Integration

| Library | Usage |
|---------|-------|
| **shikumi** | Config discovery + hot-reload (`ShiraseConfig`) |
| **tsuuchi** | Notification framework (backend trait, dispatch, rate limiting) |
| **garasu** | GPU rendering for popups and notification center panel |
| **madori** | App framework (event loop for popup display) |
| **egaku** | Widgets (list view for history, text for notification content, modal for detail) |
| **irodzuki** | Theme: base16 to GPU uniforms (urgency colors, backgrounds) |
| **tsunagu** | Daemon mode (PID lifecycle, Unix socket for CLI control) |
| **kaname** | MCP server framework |
| **soushi** | Rhai scripting engine (rule actions, custom processing) |
| **awase** | Hotkey system for notification center navigation |
| **hasami** | Clipboard (copy notification content) |

## MCP Server (kaname)

Standard tools: `status`, `config_get`, `config_set`, `version`

App-specific tools:
- `list_notifications(limit?, app?)` — current notifications in queue
- `dismiss(id)` — dismiss a specific notification
- `dismiss_all()` — dismiss all visible notifications
- `get_history(limit?, app?, since?)` — query notification history
- `search_history(query)` — full-text search in history
- `set_dnd(enabled, duration_minutes?)` — set Do Not Disturb
- `get_dnd()` — current DnD status
- `create_rule(name, match, action)` — add a filter/action rule
- `list_rules()` — list active rules
- `delete_rule(name)` — remove a rule
- `clear_history(app?)` — clear history (optionally for specific app)
- `send(title, body, urgency?, app?)` — send a test notification

## Rhai Scripting (soushi)

Scripts from `~/.config/shirase/scripts/*.rhai`

```rhai
// Available API:
shirase.dismiss(42)                  // dismiss notification by ID
shirase.dismiss_all()                // dismiss all visible notifications
shirase.history()                    // -> [{id, app, title, body, timestamp, urgency}]
shirase.history_search("meeting")    // -> matching history entries
shirase.dnd(true)                    // enable do-not-disturb
shirase.dnd(false)                   // disable do-not-disturb
shirase.dnd_for(60)                  // DnD for 60 minutes
shirase.rule_add(#{
    name: "Mute builds",
    match: #{ app: "CI", title_regex: "Build.*passed" },
    action: #{ suppress: true },
})
shirase.send("Reminder", "Stand up and stretch!", "low")
shirase.unread_count()               // -> number of unread notifications
shirase.mark_read(42)                // mark notification as read
```

Event hooks: `on_startup`, `on_shutdown`, `on_notification(notification)`,
`on_dismiss(notification)`, `on_dnd_change(enabled)`

Example: auto-DnD during meetings (when calendar event is active):
```rhai
fn on_notification(n) {
    if n.app == "Calendar" && n.title.contains("Meeting started") {
        shirase.dnd(true);
    }
    if n.app == "Calendar" && n.title.contains("Meeting ended") {
        shirase.dnd(false);
    }
}
```

Example: aggregate rapid notifications from same app:
```rhai
let last_app = "";
let count = 0;

fn on_notification(n) {
    if n.app == last_app && count > 3 {
        shirase.dismiss(n.id);
        // The first notification stays, subsequent ones are suppressed
        return;
    }
    if n.app != last_app {
        last_app = n.app;
        count = 0;
    }
    count += 1;
}
```

## Hotkey System (awase)

### Modes

**Normal** (notification center panel):
| Key | Action |
|-----|--------|
| `j/k` | Navigate notifications |
| `Enter` | Expand notification (show full body) |
| `d` | Dismiss notification |
| `D` | Dismiss all notifications |
| `n` | Toggle Do Not Disturb |
| `c` | Clear all history |
| `f` | Filter by app |
| `/` | Search history |
| `r` | Mark as read |
| `R` | Mark all as read |
| `Tab` | Toggle between current/history view |
| `q` | Close notification center |
| `:` | Command mode |

**History** (history view):
| Key | Action |
|-----|--------|
| `j/k` | Navigate history entries |
| `Enter` | Expand entry |
| `/` | Search |
| `f` | Filter by app |
| `u` | Filter by urgency |
| `d` | Delete entry from history |
| `c` | Clear visible (filtered) history |
| `Esc` | Back to current notifications |

**Command** (`:` prefix):
- `:dnd on|off` — toggle Do Not Disturb
- `:dnd 60` — DnD for 60 minutes
- `:clear` — clear all history
- `:clear <app>` — clear history for specific app
- `:rule add <app> suppress|critical|low` — quick rule creation
- `:rule list` — list active rules
- `:rule delete <name>` — remove rule
- `:send <title> <body>` — send test notification
- `:search <query>` — search history

## Nix Integration

### Flake Exports
- Multi-platform packages via substrate `rust-tool-release-flake.nix`
- `overlays.default` — `pkgs.shirase`
- `homeManagerModules.default` — `blackmatter.components.shirase`
- `devShells` — dev environment

### HM Module

Namespace: `blackmatter.components.shirase`

Fully implemented with typed options:
- `enable` — install package + generate config
- `package` — override package
- `appearance.{width, max_visible, opacity, position, animation_ms}`
- `behavior.{auto_dismiss_secs, do_not_disturb, group_by_app, sound_enabled}`
- `filters.{blocked_apps, priority_apps, quiet_hours}`
- `rules` — typed rule list (match criteria + action)
- `history.{max_entries, retention_days}`
- `daemon.{enable, listen_addr, socket_path}` — launchd/systemd service
- `extraSettings` — raw attrset escape hatch

YAML generated via `lib.generators.toYAML` -> `xdg.configFile."shirase/shirase.yaml"`.
Uses substrate's `hm-service-helpers.nix` for `mkLaunchdService`/`mkSystemdService`.

## Notification Popup Design

### Popup Layout

```
┌──────────────────────────────────────┐
│ [icon]  App Name           2m ago   │
│         Title of notification       │
│         Body text, possibly         │
│         multiline...                │
│                                      │
│         [Action 1]  [Action 2]      │
└──────────────────────────────────────┘
```

- Popups slide in from the configured edge (default: top-right)
- Stack vertically, most recent at top
- Auto-dismiss after configurable timeout
- Hover pauses dismiss timer
- Click dismisses
- GPU-rendered with semi-transparent background via garasu

### Urgency Styling

Colors from irodzuki theme, mapped to urgency:
- **Low** — subtle background, no sound, auto-dismiss quickly
- **Normal** — standard background, optional sound, standard timeout
- **Critical** — accent/error background, always sound, no auto-dismiss, shown during DnD

### Notification Center Panel

```
┌─── Notifications ─── [DnD: OFF] ─── [3 unread] ───┐
│                                                      │
│  ┌─ Mail (2) ──────────────────────────────────────┐ │
│  │  New message from Alice        10:30 AM         │ │
│  │  Re: Project update            10:15 AM         │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ┌─ Slack ─────────────────────────────────────────┐ │
│  │  #general: Bob posted          10:25 AM         │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ┌─ Calendar ──────────────────────────────────────┐ │
│  │  Standup in 15 minutes         10:45 AM         │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  ─── Earlier Today ──────────────────────────────── │
│  Calendar: Daily standup          09:00 AM          │
│  Mail: Weekly digest              08:30 AM          │
└──────────────────────────────────────────────────────┘
```

- Grouped by app (configurable)
- Chronological within groups
- Expandable groups (click/Enter to show all)
- Unread indicator (dot or bold)
- DnD status and unread count in header

## Rule Engine Design

Rules are evaluated in order against incoming notifications. First matching rule wins.

### Match Criteria

| Criterion | Type | Description |
|-----------|------|-------------|
| `app` | string | Exact app name match (case-insensitive) |
| `app_regex` | regex | App name regex |
| `title_regex` | regex | Title regex |
| `body_regex` | regex | Body text regex |
| `urgency` | enum | `low`, `normal`, `critical` |
| `time_range` | range | Time-of-day range (e.g., "22:00-07:00") |

### Rule Actions

| Action | Type | Description |
|--------|------|-------------|
| `suppress` | bool | Silently discard (still logged to history) |
| `urgency` | enum | Override urgency level |
| `sound` | string | Custom sound name |
| `bypass_dnd` | bool | Show even during DnD |
| `timeout_secs` | int | Custom auto-dismiss timeout |
| `script` | string | Path to Rhai script to execute |
| `group_tag` | string | Custom grouping tag |

## Design Constraints

- **Daemon is essential** — popups only work when daemon is running; center panel works standalone for history
- **D-Bus server on Linux** — shirase IS the notification server, not a client; it implements `org.freedesktop.Notifications`
- **macOS limitations** — cannot fully intercept all system notifications due to sandbox; works best for apps that use standard notification APIs
- **History is persistent** — SQLite database, survives restarts, searchable
- **Rules are ordered** — first match wins, no cascading rule evaluation
- **DnD exceptions** — priority apps always show, even during DnD; critical urgency always shows
- **Auto-dismiss is per-urgency** — critical notifications never auto-dismiss
- **GPU rendering for popups** — popups are independent GPU windows (garasu), not system notification bubbles
- **Unix socket for CLI control** — `shirase dnd on` communicates with running daemon via Unix socket (tsunagu)
- **Sound is optional** — depends on platform audio availability; graceful fallback to silent
