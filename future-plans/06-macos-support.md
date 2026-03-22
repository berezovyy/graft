# macOS Support

Cross-platform backend — achieving the same observable behavior with different internals.

## The problem

Graft's core is built on Linux primitives that don't exist on macOS:

| Linux primitive | Used for | macOS equivalent |
|----------------|----------|-----------------|
| OverlayFS | Zero-copy workspace isolation | Nothing native |
| cgroups v2 | Memory/CPU limits | Nothing native |
| cgroup.freeze | Atomic process tree freeze | Nothing native |
| Network namespaces | Port isolation | Nothing native |
| Xvfb / Xephyr | Virtual displays | Nothing native |
| fanotify | Mount-level file monitoring | FSEvents (different API, similar result) |
| SIGSTOP/SIGCONT | Process freeze/resume | Works on macOS |

macOS has no union filesystem, no container primitives, no namespace isolation. But it has APFS — and APFS has tricks nobody is using for this.

## Strategy: platform-native fast paths

Don't emulate OverlayFS on macOS — build a macOS-native backend that achieves the same observable behavior with different internals. Same as how Bun picks the fastest syscall on each platform.

## Three approaches

### Approach 1: APFS clones (cp -c) — native, no dependencies

macOS has `clonefile()` — a dedicated syscall that creates an instant CoW copy on APFS. This is what Bun uses for package installs.

```c
// Single syscall, instant CoW on APFS, no data copied
clonefile("/base/src/App.tsx", "/workspace/src/App.tsx", 0);
```

**Fork implementation:**
1. Walk base directory tree (skip node_modules, .git, build caches via .gitignore)
2. `mkdir -p` for each subdirectory in workspace
3. Parallel dispatch `clonefile()` for every source file via rayon/GCD
4. Symlink large read-only dirs: `node_modules → base/node_modules`

**Expected performance:**

| Project size | Files cloned | Fork time |
|-------------|-------------|-----------|
| Small (200 files) | 200 | ~10-20ms |
| Medium (2,000 files) | 2,000 | ~50-100ms |
| Large (10,000 files) | 10,000 | ~200-500ms |

vs Linux OverlayFS: ~5ms regardless of project size.

Not as fast, but "instant" to a human and fast enough for agent loops.

**Diff via FSEvents change log:**

On Linux, the upper directory IS the diff. On macOS, there's no upper directory. Build one via FSEvents:

```
Linux:   OverlayFS upper dir → list files → that's the diff
macOS:   FSEvents watcher → change log → that's the diff
```

Start an FSEvents watcher at fork time. Every file write inside the workspace gets recorded to a JSONL change log. `graft diff` reads the log, not the filesystem. Same speed as Linux.

**Selective cloning (do less work):**

```
Full project:    50,000 files (with node_modules)
git-tracked:      2,000 files
Agent will touch:    ~20 files
```

Clone only git-tracked files. Symlink everything in .gitignore. Fork clones 2K files instead of 50K — 10-25x faster.

If the agent needs to modify a symlinked dir (e.g., `bun install`), graft detects the write, breaks the symlink, copies the dir, and records it in the change log.

**Content-addressed store (optional dedup):**

For many parallel workspaces, use a content-addressed store with hardlinks:

```
~/.graft/store/
  ab/cdef1234...  → file contents (deduplicated by hash)

~/.graft/workspace-a/
  src/App.tsx     → hardlink to store/ab/cdef1234...

~/.graft/workspace-b/
  src/App.tsx     → hardlink to store/ff/9988aabb...  (different version)
  src/utils.ts    → hardlink to store/98/7654abcd...  (shared, same as ws-a)
```

Storage proportional to unique content, not workspace count. Opt-in: `graft fork --dedup`.

### Approach 2: unionfs-fuse via macFUSE — full overlay semantics

macFUSE provides FUSE on macOS. `unionfs-fuse` (or similar) gives actual overlay behavior with upper/lower dirs, like Linux OverlayFS.

```bash
brew install macfuse
# then use unionfs-fuse for overlay mounts
```

**Pros:** Full overlay semantics, upper dir IS the diff (same as Linux), no FSEvents watcher needed.
**Cons:** Requires macFUSE installation (kernel extension), slower than native APFS, macFUSE has had stability/compatibility issues on newer macOS versions.

### Approach 3: Lima/colima VM — full Linux inside

Run graft inside a lightweight Linux VM. Everything works with 100% feature parity.

```bash
brew install orbstack   # or lima, colima
# graft runs inside Linux VM with real OverlayFS, cgroups, namespaces
# OrbStack's virtiofs shares files with macOS at near-native speed
```

**Pros:** 100% feature parity — OverlayFS, cgroups, network namespaces, Xvfb, everything.
**Cons:** Requires VM runtime, slight overhead, extra setup.

Recommended for heavy parallel workloads (10+ workspaces, Electron testing, full-stack isolation). For lighter use (2-3 workspaces, web dev servers), the native APFS backend is sufficient.

## What works on macOS without extra effort

Platform-independent features that work identically:

- Port auto-assignment (`PORT` env var / `--port` flag injection)
- SIGSTOP / SIGCONT process freeze/resume
- Reverse proxy for dev server switching
- `graft.toml` service wiring
- Stacking (fork from fork — nested clonefile trees)
- CLI agent interface (all commands, different backend)
- CDP-based agent testing (headless Chromium works on macOS)

## What macOS loses

| Feature | Linux | macOS (native) | Impact |
|---------|-------|----------------|--------|
| Fork speed | ~5ms | ~50-500ms | Acceptable for dev, slower for tight agent loops |
| Upper dir = diff | Free (kernel) | FSEvents log (userspace) | Same diff speed, but extra watcher per workspace |
| tmpfs overlays | RAM-backed upper | RAM disk possible but clunky | Lose the elegant `--tmpfs` flag |
| Memory limits | cgroups | Not available | Can't cap workspace memory |
| Atomic tree freeze | cgroup.freeze | SIGSTOP per-process | Must signal each child manually |
| Network isolation | Network namespaces | Not available | Port conflicts if services hardcode ports |
| Virtual displays | Xvfb / Xephyr | Not available | Electron instances are visible windows |

## Architecture: Backend trait

Abstract over the platform:

```rust
trait Backend {
    fn fork(&self, base: &Path, name: &str, opts: ForkOpts) -> Result<Workspace>;
    fn drop(&self, name: &str) -> Result<()>;
    fn changes(&self, name: &str) -> Result<Vec<FileChange>>;
    fn merge(&self, name: &str, opts: MergeOpts) -> Result<()>;
    fn snapshot(&self, name: &str, label: &str) -> Result<Snapshot>;
    fn restore(&self, name: &str, snapshot: &str) -> Result<()>;
}

// Linux: OverlayFS mount/umount, walk upper dir
// macOS: clonefile + FSEvents, read change log
```

The CLI doesn't know which backend is active. Same commands, same JSON output, same UX.

## Recommendation

| User profile | Recommended approach |
|-------------|---------------------|
| Developer at laptop, 2-3 workspaces | APFS clones (native, zero deps) |
| Heavy parallel workloads, 10+ workspaces | OrbStack/Lima VM (full Linux) |
| Needs exact Linux parity | OrbStack/Lima VM |
| CI/CD | Run on Linux (most AI agent workloads do anyway) |

Build the `Backend` trait from day one. Ship Linux-first. Add the macOS backend when demand justifies it.
