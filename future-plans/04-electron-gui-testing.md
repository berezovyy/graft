# Electron/GUI Testing

Headless GUI testing for agents — each workspace gets an isolated virtual display, Electron process, and CDP connection.

## The problem

A dev server is easy — reverse proxy, switch the target, done. Electron is different:
- It's a **desktop GUI process**, not an HTTP server
- It creates its own **window** (Chromium renderer)
- You can't "proxy" a window
- Multiple instances compete for GPU and eat RAM
- The agent needs to **see and interact** with the UI to verify its work

Two separate problems:
1. **Agent testing** — how does the agent verify the Electron app works?
2. **Human switching** — how does the developer flip between workspace variants?

## What we have now

- Workspace file isolation via OverlayFS
- `graft run` for starting processes in workspaces
- No display management, no CDP integration, no screenshot capability

## What's missing

### Xvfb virtual displays per workspace

`Xvfb` is a virtual X11 display server — renders everything to memory, no physical monitor needed, no GPU needed. Every Linux distro has it.

Each Electron workspace gets its own isolated virtual display:

```bash
# Workspace A
Xvfb :10 -screen 0 1920x1080x24 &
DISPLAY=:10 electron ~/.graft/ws-a/merged --remote-debugging-port=9222 &

# Workspace B
Xvfb :11 -screen 0 1920x1080x24 &
DISPLAY=:11 electron ~/.graft/ws-b/merged --remote-debugging-port=9223 &
```

Zero visual interference between workspaces. Display numbers allocated from a high range (100+) to avoid conflicts with real displays.

State stored per workspace:

```rust
pub struct DisplayState {
    pub display_number: u32,    // :10, :11, etc.
    pub pid: u32,               // Xvfb process PID
    pub width: u32,             // 1920
    pub height: u32,            // 1080
    pub depth: u32,             // 24
}
```

### Electron launch + CDP integration

Electron is Chromium under the hood. The `--remote-debugging-port` flag exposes the Chrome DevTools Protocol — the same protocol Playwright and Puppeteer use.

Through CDP, an agent can:
- **Screenshot** the app (pixel-perfect PNG)
- **Read DOM** state (query selectors, get text content)
- **Click/type** (dispatch input events)
- **Run JavaScript** in the renderer process
- **Navigate** between routes
- **Access console logs** and network requests

```
Workspace A:
  Xvfb :10 → Electron (DISPLAY=:10) → CDP on :9222
                                       ↑
                                  Agent connects here

Workspace B:
  Xvfb :11 → Electron (DISPLAY=:11) → CDP on :9223
                                       ↑
                                  Agent connects here
```

CLI commands:

```bash
# Start Electron in a workspace
graft electron ws-a --cmd "electron ."
graft electron ws-a --cmd "electron ." --width 1920 --height 1080

# JSON output for agent consumption
graft electron ws-a --json
# {"cdp_port": 9222, "display": ":10", "pid": 12345}
```

State stored per workspace:

```rust
pub struct ElectronState {
    pub pid: u32,               // Main Electron process PID
    pub cdp_port: u16,          // Chrome DevTools Protocol port (922N)
    pub command: String,        // Original command ("electron .")
}
```

### graft screenshot via Page.captureScreenshot

CDP's `Page.captureScreenshot` returns a base64-encoded PNG of the current page. Graft wraps this as a CLI command:

```bash
graft screenshot ws-a --output /tmp/screenshot.png

# JSON output
graft screenshot ws-a --json
# {"image_path": "/tmp/graft-screenshot-ws-a.png", "width": 1920, "height": 1080}
```

The agent sees the screenshot as an image (multimodal) and can verify layout, colors, text, button positions — visual things that unit tests can't catch.

### Xephyr/cage for human-visible nested displays

For developers who want to *see* the Electron apps side-by-side:

**Xephyr** (X11) — shows the virtual display as a window on your real screen:

```bash
Xephyr :10 -screen 1280x720 &
DISPLAY=:10 electron ~/.graft/ws-a/merged &

Xephyr :11 -screen 1280x720 &
DISPLAY=:11 electron ~/.graft/ws-b/merged &
```

```
┌─────────────────────────────────────────────┐
│  Your real desktop                          │
│                                             │
│  ┌──────────────────┐ ┌──────────────────┐  │
│  │  Xephyr :10      │ │  Xephyr :11      │  │
│  │  (ws-a Electron) │ │  (ws-b Electron) │  │
│  │                  │ │                  │  │
│  │  ┌────────────┐  │ │  ┌────────────┐  │  │
│  │  │ App with   │  │ │  │ App with   │  │  │
│  │  │ auth       │  │ │  │ new UI     │  │  │
│  │  │ changes    │  │ │  │ design     │  │  │
│  │  └────────────┘  │ │  └────────────┘  │  │
│  └──────────────────┘ └──────────────────┘  │
└─────────────────────────────────────────────┘
```

**cage** (Wayland) — nested compositor, same concept for Wayland desktops:

```bash
cage -- electron ~/.graft/ws-a/merged &
cage -- electron ~/.graft/ws-b/merged &
```

**SIGSTOP/SIGCONT + window raise** — all Electron apps on the real display, only one active:

```bash
graft switch ws-b
# 1. SIGSTOP ws-a's Electron process tree
# 2. SIGCONT ws-b's Electron process tree
# 3. xdotool windowactivate $(xdotool search --pid $PID_ELECTRON_B)
```

### Cgroup resource limits for Electron's multi-process tree

Electron spawns multiple processes: main, renderer, GPU, utility. Individual `SIGSTOP` would miss new children. cgroup.freeze handles the entire tree atomically.

```
/sys/fs/cgroup/graft/ws-a/
├── memory.max     = 2147483648  (2GB)
├── cpu.max        = 100000 100000  (1 CPU)
└── cgroup.procs   = 12345 12346 12347 12348  (Electron + Xvfb + node)
```

Resource profile for multiple Electron workspaces:

```
ws-a: active    →  ~300MB RAM, ~15% CPU
ws-b: frozen    →  ~300MB RAM (swappable), 0% CPU
ws-c: frozen    →  ~300MB RAM (swappable), 0% CPU
ws-d: frozen    →  ~300MB RAM (swappable), 0% CPU
```

On NUMA systems, kernel 6.17's per-NUMA proactive reclaim pushes sleeping workspace memory to remote NUMA nodes first. With sched_ext cgroup bandwidth, active workspaces get CPU priority before freezing others.

Sandboxing: each Electron workspace can be restricted with Landlock (only access own overlay) + seccomp-bpf (block mount, ptrace, module_load).

## Full agent testing flow

```
1. graft fork . --name electron-fix

2. [agent edits code in workspace]

3. graft electron electron-fix --cmd "electron ."
   → Xvfb :10 starts
   → Electron starts on :10 with CDP on :9222

4. graft screenshot electron-fix
   → agent sees the app UI as an image
   → "The button is in the wrong position, let me fix..."

5. [agent clicks, types via CDP]
   → graft screenshot electron-fix
   → "Form submits correctly, success message shows"

6. graft merge electron-fix --commit --drop
```

## Phases

| # | Name | What it adds |
|---|------|-------------|
| 1 | Xvfb management | Display allocation, virtual framebuffer lifecycle |
| 2 | Electron launch | `graft electron` command, DISPLAY setup, process tracking |
| 3 | CDP integration | Connect to Chrome DevTools Protocol, basic queries |
| 4 | Screenshot command | `graft screenshot` via CDP `Page.captureScreenshot` |
| 5 | CLI tools | `--json` output for agent consumption |
| 6 | Human switching | Xephyr nested displays, window management, cage |
| 7 | Resource management | Cgroup limits for Electron, GPU process handling |
| 8 | Polish | Multi-window, Wayland support, error recovery, cleanup |

MVP: phases 1-5 (agent-ready). Full experience: add phase 6 (human switching) and 7 (resource management).

## Dependencies

- Core workspace lifecycle — already done
- Dev server track provides cgroup management (`src/cgroup.rs`) and process tracking (`src/process.rs`)
- `graft drop` must kill Electron + stop Xvfb before unmounting overlay
- `graft switch` for Electron extends devserver track's switch command

## New files

```
src/display.rs            — Xvfb display allocation and lifecycle
src/cdp.rs                — Chrome DevTools Protocol client
src/commands/electron.rs  — Start Electron in workspace
src/commands/screenshot.rs — Capture screenshot via CDP
```

## Linux primitives used

| Primitive | Purpose |
|-----------|---------|
| Xvfb | Virtual display for headless Electron |
| Xephyr | Nested display as window (human side-by-side) |
| cage (Wayland) | Nested compositor for Wayland desktops |
| Chrome DevTools Protocol | Agent interaction with Electron UI |
| cgroups v2 | Memory/CPU limits per workspace |
| cgroup.freeze | Atomic freeze of Electron's multi-process tree |
| pidfd | Safe process handles for Electron's process tree |
| Landlock | Filesystem sandboxing (agent can't escape overlay) |
| seccomp-bpf | Syscall filtering (block mount, ptrace) |
| xdotool / wmctrl | Window focus management (X11) |
| Wayland IPC | Window focus management (Wayland) |
