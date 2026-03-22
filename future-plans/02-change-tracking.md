# Change Tracking

JJ-inspired automatic history - every file mutation tracked with process attribution and session binding.

## The idea

JJ's key insight: the working copy is always a commit. No staging, no explicit saves. Graft takes this further: we record not just WHAT changed, but WHEN, WHO (which process), and WHY (which agent session).

The overlay upper directory already IS the diff. Change tracking adds the time dimension - a log of how that diff evolved, who made each change, and which conversation triggered it.

## What we have now

- `graft diff` walks the upper directory to show current state
- `--session` flag on `graft fork` and `graft enter` sets `GRAFT_SESSION` env var
- Session stored in workspace state (`state.json`)
- No live file watching, no change log, no attribution

## What's missing

### fanotify - mount-level file watching

`fanotify` monitors an **entire mount point** with a single syscall. Unlike `inotify` (per-directory watches), fanotify sees every file operation on the overlay mount and reports the PID that made the change.

```c
fanotify_mark(fd, FAN_MARK_ADD | FAN_MARK_MOUNT,
              FAN_CREATE | FAN_MODIFY | FAN_DELETE | FAN_MOVED_FROM | FAN_MOVED_TO,
              AT_FDCWD, "/home/user/.graft/ws-a/merged")
```

From the PID:

- `/proc/<pid>/comm` → process name (`claude-code`, `node`, `bun`)
- `/proc/<pid>/environ` → environment variables (including `GRAFT_SESSION`)
- `/proc/<pid>/cwd` → working directory

This gives automatic, zero-configuration change attribution.

### inotify fallback on upper dir

fanotify requires `CAP_SYS_ADMIN` (or user namespace tricks). The pragmatic fallback: `inotify` on the upper directory directly. Since OverlayFS only writes to upper, this captures exactly the mutations with zero noise from read-through to lower.

```c
inotify_add_watch(fd, "~/.graft/ws-a/upper",
                  IN_CREATE | IN_MODIFY | IN_DELETE | IN_MOVED_FROM | IN_MOVED_TO)
```

Trade-off: inotify needs recursive watches (one per subdirectory in upper), but upper is small - only changed files - so this is fine.

### eBPF on VFS kprobes

For high-throughput scenarios (`npm install` writing thousands of files), fanotify is too chatty. Attach an eBPF program to `vfs_write` via kprobes:

```
BPF on vfs_write:
  → filter: only fire for writes inside overlay mounts
  → pre-filter events in the kernel
  → userspace reads a clean, pre-filtered event stream
```

The kernel does the filtering. Near-zero CPU cost. Complementary to fanotify: eBPF handles the fast path, fanotify handles permission events.

### Session binding

Link every file change to the agent session that made it.

```bash
graft fork . --name ws-a --session "cc-session-abc123"
# or via env var:
GRAFT_SESSION="cc-session-abc123" graft fork . --name ws-a
```

Session metadata stored in `~/.graft/ws-a/session.json`:

```json
{
  "session_id": "cc-session-abc123",
  "agent": "claude-code",
  "created": "2026-03-22T11:05:00Z",
  "context": "user asked to add dark mode support"
}
```

The fanotify watcher reads `GRAFT_SESSION` from `/proc/<pid>/environ` and tags every change event with it.

### Change log format

Append-only JSONL at `~/.graft/<name>/changes.jsonl`:

```jsonl
{"ts":"2026-03-22T11:05:32.451Z","op":"modify","path":"src/App.tsx","pid":12345,"proc":"claude-code","session":"cc-abc123"}
{"ts":"2026-03-22T11:05:32.890Z","op":"create","path":"src/DarkMode.tsx","pid":12345,"proc":"claude-code","session":"cc-abc123"}
{"ts":"2026-03-22T11:05:35.340Z","op":"modify","path":"bun.lockb","pid":14200,"proc":"bun","session":"cc-abc123"}
{"ts":"2026-03-22T11:07:15.220Z","op":"delete","path":"src/old-theme.css","pid":12345,"proc":"claude-code","session":"cc-abc123"}
```

### graft log and graft blame

Query the change log:

```bash
graft log ws-a --since 5m --path src/
#  11:05:32  modify  src/App.tsx       claude-code  cc-abc123
#  11:05:33  create  src/DarkMode.tsx  claude-code  cc-abc123
#  11:07:12  modify  src/App.tsx       claude-code  cc-abc123

graft blame ws-a src/App.tsx
# modified at 11:05:32 by claude-code session cc-abc123
# modified at 11:07:12 by claude-code session cc-abc123
```

Full traceability chain: file change → change log → session.json → agent conversation.

### Operation log (oplog.jsonl)

Every graft command that mutates state is logged (like JJ's operation log):

```jsonl
{"ts":"2026-03-22T11:05:00Z","op":"fork","workspace":"ws-a","base":"/home/user/project","session":"cc-abc123"}
{"ts":"2026-03-22T11:05:32Z","op":"snap","workspace":"ws-a","snapshot":"0001","trigger":"auto"}
{"ts":"2026-03-22T11:10:00Z","op":"merge","workspace":"ws-a","target":"base","files":4}
{"ts":"2026-03-22T11:10:01Z","op":"drop","workspace":"ws-a"}
```

Answers: "What workspaces existed at time T?", "When was workspace X merged?", "Which session created workspace X?"

### Auto-snapshots

Automatic checkpoints of the upper directory, triggered by:

1. **Time-based** - every 30s (configurable)
2. **Event-based** - after 10 file changes (configurable)
3. **On-demand** - `graft snap ws-a --name "before-refactor"`
4. **On idle** - when no changes for M seconds

Snapshot = copy of the upper dir. Upper is small (only changed files), so snapshots are trivially fast.

```
~/.graft/ws-a/snapshots/
├── 0001/
│   ├── .meta.json    # {"ts": "...", "trigger": "auto", "changes_since_last": 5}
│   └── upper/        # copy of upper at this point
│       └── src/App.tsx
├── 0002/
│   └── upper/
│       ├── src/App.tsx
│       └── src/DarkMode.tsx
```

Copy strategy:

- **btrfs/XFS**: `cp --reflink` - instant CoW, zero bytes copied
- **ext4**: plain copy - still sub-millisecond because upper is tiny

### io_uring for async scanning

For live diff views during rapid edits, `io_uring` batches syscalls without context switches. Combined with eBPF filtering, handles thousands of file events per second without impacting workspace performance.

### Extended attributes (kernel 6.17)

Tag overlay files with graft metadata via xattrs:

```
user.graft.workspace=ws-a
user.graft.session=cc-abc123
user.graft.modified_at=2026-03-22T11:05:32Z
```

The `file_getattr`/`file_setattr` syscalls (6.17) provide `openat(2)` semantics that work properly with directory file descriptors inside mount namespaces.

## Phases

| #   | Name                  | What it adds                                       |
| --- | --------------------- | -------------------------------------------------- |
| 1   | Change log foundation | JSONL writer, `ChangeEvent` type, append-only file |
| 2   | inotify watcher       | Watch upper directory, generate change events      |
| 3   | fanotify watcher      | Mount-level monitoring with PID attribution        |
| 4   | Process attribution   | Resolve PID → process name, session, cwd           |
| 5   | Session binding       | `--session` flag, `session.json`, `GRAFT_SESSION`  |
| 6   | `graft log`           | Query change log with filters                      |
| 7   | `graft blame`         | Trace file changes to sessions                     |
| 8   | Operation log         | `oplog.jsonl` records every graft command          |
| 9   | Auto-snapshots        | Time/event-based automatic snapshots               |
| 10  | eBPF acceleration     | Optional VFS kprobes for near-zero CPU watching    |
| 11  | Polish                | .gitignore filtering, debouncing, xattrs           |

## Noise filtering

Raw fanotify/inotify includes everything - editor temp files, `.swp`, `node_modules` writes. Filter by:

1. **.gitignore rules** - skip `node_modules/`, `.git/`, etc.
2. **Process filtering** - optionally track only specific processes
3. **Debouncing** - coalesce rapid writes to the same file
4. **Upper-only focus** - watch upper dir, not merged (simpler, same result)

## Dependencies

- Needs core workspace lifecycle - already done
- Watcher starts as background thread inside `graft enter` shell process
- Auto-snapshots need snapshot infrastructure (currently removed from CLI, would need to re-add)
- `src/procfs.rs` (PID resolution) is separate from dev server track's `src/process.rs` (pidfd lifecycle)

## New files

```
src/changelog.rs      - ChangeEvent type, JSONL writer/reader
src/watcher.rs        - Watcher trait + inotify/fanotify implementations
src/session.rs        - Session binding, session.json
src/oplog.rs          - Operation log (oplog.jsonl)
src/procfs.rs         - PID → process name/session/cwd resolution
src/commands/log.rs   - graft log command
src/commands/blame.rs - graft blame command
```

## Linux primitives used

| Primitive                            | Purpose                                   |
| ------------------------------------ | ----------------------------------------- |
| fanotify                             | Mount-wide file event monitoring with PID |
| inotify                              | Per-directory fallback watcher            |
| eBPF on VFS kprobes                  | In-kernel file event filtering            |
| `/proc/<pid>/comm`                   | Process name resolution                   |
| `/proc/<pid>/environ`                | Session ID extraction                     |
| io_uring                             | Async file watching / diff scanning       |
| `file_getattr`/`file_setattr` (6.17) | Xattr metadata on overlay files           |
| `cp --reflink` (btrfs/XFS)           | Zero-copy snapshots via CoW clones        |
