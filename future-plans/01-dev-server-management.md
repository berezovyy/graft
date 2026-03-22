# Dev Server Management

Process lifecycle, freeze/thaw, and reverse proxy routing on top of graft workspaces.

## What we have now

- `graft run <name> -- <cmd>` starts a process in the workspace via `setsid()` (detached session)
- `graft switch <name>` hot-swaps the active workspace
- Simple TCP proxy (`src/proxy.rs`) that re-reads `state.json` per connection
- Process kill via `kill(-(pid), SIGTERM)` → 200ms → `SIGKILL` (process group signaling)
- Port allocation from range 5501–5600

## What's missing

### cgroup.freeze - atomic process tree freeze/thaw

Dev servers spawn child processes (file watchers, worker threads, transpilers). Sending `SIGSTOP` to the main process leaves children running. `cgroup.freeze` freezes the entire cgroup atomically - every process in the tree stops at once, and new children are auto-frozen too.

```
graft sleep ws-a  →  echo 1 > /sys/fs/cgroup/graft/ws-a/cgroup.freeze
graft wake ws-a   →  echo 0 > /sys/fs/cgroup/graft/ws-a/cgroup.freeze
```

Implementation:

1. On `graft run`, create a cgroup at `/sys/fs/cgroup/graft/<name>/`
2. Move the spawned process into that cgroup
3. `graft sleep` writes `1` to `cgroup.freeze`
4. `graft wake` writes `0` to `cgroup.freeze`
5. Store `cgroup_path` in workspace state

Requires cgroups v2 with delegation (most modern distros). Fallback: `SIGSTOP`/`SIGCONT` per process.

### pidfd - safe process handles

Current code uses raw PIDs (`kill(pid, signal)`). If a dev server dies and its PID is reused by another process, we'd signal the wrong process. `pidfd` gives a file descriptor handle that's tied to the exact process - no recycling bugs.

```rust
// Current (unsafe)
libc::kill(-(pid as i32), libc::SIGTERM);

// With pidfd (safe)
let pidfd = pidfd_open(pid, 0);        // FD tied to this exact process
pidfd_send_signal(pidfd, SIGTERM, ...); // race-free, can't hit wrong process
```

Key syscalls:

- `pidfd_open(pid)` - get a handle to an existing process
- `pidfd_send_signal(pidfd, sig)` - race-free signaling
- `pidfd_getfd(pidfd, targetfd)` - steal FDs from another process (for socket handoff)
- `waitid(P_PIDFD, pidfd)` - wait for process exit without PID races

Changes to `src/util.rs`:

- Replace `kill_process()` with pidfd-based signaling
- Replace `is_pid_alive()` with pidfd poll
- Store pidfd (or verify PID freshness) in state

### Full reverse proxy with HTTP + WebSocket

Current proxy (`src/proxy.rs`) is a raw TCP forwarder - it pipes bytes between client and backend. This breaks WebSocket upgrade detection, proper HTTP error pages, and health-aware routing.

Replace with an HTTP-aware proxy:

- Parse HTTP requests, forward to active workspace's port
- Detect `Upgrade: websocket` headers, proxy WebSocket frames bidirectionally
- On `graft switch`, close existing WebSocket connections to trigger browser HMR reconnect
- Serve a friendly error page when no workspace is active

```
Browser ──► Proxy (:3000) ──┬──► ws-a vite (:5501)  ← active
                            ├──► ws-b vite (:5502)  ← frozen
                            └──► ws-c vite (:5503)  ← frozen
```

Options:

- `hyper` 1.x + `tokio` - full control, ~200 lines
- `tokio` raw with HTTP/1.1 parsing - lighter but more manual
- Keep current TCP approach but add WebSocket frame detection

### Sub-100ms graft switch

The full switch flow combines cgroup freeze/thaw with proxy re-routing:

```
graft switch ws-b:
  1. cgroup.freeze ws-a        (~50μs - kernel atomic operation)
  2. cgroup.thaw ws-b          (~50μs)
  3. Update proxy target       (atomic store, ~1μs)
  4. Close WebSocket connections (triggers browser reconnect)
  5. Browser reconnects via HMR (~50-100ms)
```

Total: dominated by browser reconnect, not graft. The freeze/thaw/re-route is microseconds.

Advanced: `pidfd_getfd()` can steal the listening socket from ws-a's dev server and hand it to ws-b's - the browser doesn't even need to reconnect. True zero-downtime.

### Health checks and auto-port detection

- After `graft run`, poll the workspace's port until it responds (or timeout)
- Detect which `--port` flag the tool uses: vite (`--port`), next (`-p`), webpack (`--port`)
- Fallback: set `PORT=<n>` environment variable
- Report health status in `graft ls` output

## Phases

| #   | Name               | What it adds                                       |
| --- | ------------------ | -------------------------------------------------- |
| 1   | Cgroup foundation  | Create cgroup hierarchy, basic freeze/thaw         |
| 2   | Process tracking   | pidfd-based handles, state extensions              |
| 3   | Serve              | Start dev server in workspace with port allocation |
| 4   | Sleep/Wake         | `graft sleep`/`graft wake` via cgroup.freeze       |
| 5   | Reverse proxy core | HTTP proxy forwarding to workspace dev servers     |
| 6   | WebSocket proxying | WebSocket upgrade support for HMR                  |
| 7   | Switch             | Freeze + thaw + re-route - full flow               |
| 8   | Polish             | Health checks, auto-port detection, error recovery |

## Dependencies

- Needs core state management (`src/state.rs`) - already done
- `graft drop` must stop cgroup + kill processes before unmount
- `ServiceState` type shared with full-stack isolation track

## New files

```
src/cgroup.rs       - cgroup hierarchy creation, freeze/thaw
src/process.rs      - pidfd-based process management
src/proxy.rs        - replace current TCP proxy with HTTP+WS proxy
src/port.rs         - port allocation and tool detection
src/commands/serve.rs
src/commands/sleep.rs
src/commands/wake.rs
src/commands/switch.rs  - extend existing with cgroup integration
```

## Linux primitives used

| Primitive                             | Purpose                                   |
| ------------------------------------- | ----------------------------------------- |
| cgroups v2                            | Process grouping, resource accounting     |
| `cgroup.freeze`                       | Atomic freeze/thaw of entire process tree |
| `pidfd_open`                          | Safe process handle (no PID recycling)    |
| `pidfd_send_signal`                   | Race-free signaling                       |
| `pidfd_getfd`                         | Socket handoff for zero-downtime switch   |
| `setsid()`                            | Detached process sessions (already used)  |
| `sched_ext` + cgroup bandwidth (6.17) | CPU priority for active workspace         |
