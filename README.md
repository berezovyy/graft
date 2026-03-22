<p align="center">
  <img src="assets/banner.svg" width="700" alt="graft" />
</p>

<p align="center">
  <strong>Instant, zero-copy, disposable workspaces powered by OverlayFS.</strong>
</p>

<p align="center">
  No Docker. No root. Nothing is actually copied. Only what you change gets stored. <br/>
~5ms to fork any project, any size.
</p>

<p align="center">
  <a href="https://crates.io/crates/graft"><img src="https://img.shields.io/crates/v/graft?style=flat-square&color=58a6ff" alt="crates.io" /></a>
  <a href="https://github.com/berezovyy/graft/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-MIT-a371f7?style=flat-square" alt="license" /></a>
  <a href="#requirements"><img src="https://img.shields.io/badge/Linux-only-FCC624?style=flat-square&logo=linux&logoColor=black" alt="linux" /></a>
  <a href="#install"><img src="https://img.shields.io/badge/no_root-required-238636?style=flat-square" alt="no root" /></a>
  <a href="#how-it-works"><img src="https://img.shields.io/badge/fork_time-~5ms-58a6ff?style=flat-square" alt="~5ms" /></a>
</p>

<br/>

## Why graft?

`cp -r` is slow. Docker is overkill. Git worktrees don't include your `.env`, `node_modules`, or build cache.

|                         |  `cp -r`  |  Docker   | Git Worktree |   **graft**   |
| :---------------------- | :-------: | :-------: | :----------: | :-----------: |
| **Speed**               |   Slow    |   Slow    |     Fast     |   **~5ms**    |
| `.env` / `node_modules` |    Yes    |    No     |      No      |    **Yes**    |
| **Isolation**           |   Full    |   Full    |   Partial    |   **Full**    |
| **Disk usage**          | Full copy | Full copy |   Partial    | **Zero-copy** |
| **Root required**       |    No     |   Often   |      No      |    **No**     |
| **Cleanup**             |  Manual   |  Manual   |    Manual    | `graft drop`  |

<br/>

## Quick start

```bash
graft fork . --name experiment     # instant - nothing is copied
graft enter experiment             # opens a shell inside the workspace

# make changes, break things, go wild
# ...then exit the shell

graft diff experiment              # see what changed
graft diff experiment --full       # unified diff

graft merge experiment --drop      # happy? merge back
graft drop experiment              # not happy? throw it away
```

```
project/                 graft fork . --name experiment
├── src/
├── node_modules/  ───►  ~/.graft/experiment/merged/    (looks identical)
├── .env                   ├── src/                      (read from original)
└── package.json           ├── node_modules/             (read from original)
                           ├── .env                      (read from original)
                           └── src/api.rs                ← only changed file stored
```

<br/>

## Install

```bash
cargo install --path .
```

<details><summary><strong>Prerequisites</strong> - fuse-overlayfs &amp; fuse3</summary>

```bash
# Ubuntu/Debian
sudo apt install fuse-overlayfs fuse3

# Fedora
sudo dnf install fuse-overlayfs fuse3

# Arch
sudo pacman -S fuse-overlayfs fuse3
```

</details>

<br/>

## Workflows

### Parallel experiments

```bash
graft fork . --name approach-a
graft fork . --name approach-b

# two isolated workspaces, same base, zero copying
# open each in a separate terminal, editor, or agent

graft diff approach-a --stat
graft diff approach-b --stat

graft merge approach-a --commit -m "switch to Hono"
graft drop approach-b
```

<details><summary><strong>Dev server hot-swap</strong></summary>

Run dev servers in different workspaces and switch between them without restarting anything:

```bash
graft fork . --name feature-a
graft fork . --name feature-b

graft run feature-a --port 3000 -- npm run dev
graft run feature-b --port 3000 -- npm run dev

# proxy on localhost:4000 routes to the active workspace
graft switch feature-a    # localhost:4000 → feature-a
graft switch feature-b    # localhost:4000 → feature-b
```

</details>

<details><summary><strong>Stacking (fork-of-fork)</strong></summary>

```bash
graft fork . --name step-1
# ... make data model changes in step-1 ...

graft fork step-1 --name step-2
# ... build API on top of step-1's changes ...

# see everything that changed from root to step-2
graft diff step-2 --cumulative
```

</details>

<details><summary><strong>Ephemeral workspaces</strong></summary>

```bash
# auto-creates a workspace, opens a shell, destroys on exit
graft enter --ephemeral

# or run a single command
graft enter --ephemeral -- make test

# RAM-backed - vanishes on reboot, zero trace
graft enter --ephemeral --tmpfs
```

</details>

<br/>

## Commands

| Command  | Description                                                            |
| :------- | :--------------------------------------------------------------------- |
| `fork`   | Create a workspace from a directory                                    |
| `enter`  | Shell into a workspace (`--ephemeral`, `--merge-on-exit`, `--create`)  |
| `diff`   | Show changes (`--stat`, `--full`, `--files`, `--json`, `--cumulative`) |
| `merge`  | Apply changes to base (`--commit`, `--patch`, `--drop`)                |
| `drop`   | Remove a workspace (`--force`, `--all`, `--glob`)                      |
| `ls`     | List workspaces                                                        |
| `run`    | Start a dev server with automatic proxy setup                          |
| `switch` | Hot-swap which workspace the proxy routes to                           |
| `nuke`   | Remove everything                                                      |

<br/>

## How it works

Graft uses [fuse-overlayfs](https://github.com/containers/fuse-overlayfs) to create a union mount:

```
merged/    ← you work here (reads from both layers)
  │
  ├── upper/   ← your changes live here (copy-on-write)
  └── lower/   ← original project (read-only, untouched)
```

When you read a file, it comes from the original. When you write, the file is copied to upper first (copy-on-write), then modified there. The original never changes. Deletes are tracked as whiteout markers. The upper directory _is_ the diff - no diffing algorithm needed.

Forking is instant (~5ms) because nothing is copied. Merging moves files from upper back to the base. Dropping unmounts and deletes the upper.

<br/>

## Under the hood

> **No daemon. No runtime. Just syscalls and a state file.**

| Primitive           | Role                                                              |
| :------------------ | :---------------------------------------------------------------- |
| **OverlayFS**       | Copy-on-write file isolation - the kernel does the hard work      |
| **flock()**         | File-level locking so parallel graft commands don't corrupt state |
| **setsid()**        | Dev servers run as independent sessions, survive your terminal    |
| **kill() + pgid**   | Stopping a server kills the entire process tree                   |
| **Atomic writes**   | write → fsync → rename - state file can't corrupt on crash        |
| **User namespaces** | Unprivileged operation - no root needed, ever                     |

<br/>

## Requirements

- Linux (OverlayFS is a Linux kernel feature)
- `fuse-overlayfs` and `fuse3` installed
- No root/sudo needed

## License

MIT

<p align="center">
  <a href="https://github.com/berezovyy/graft/issues">Report Bug</a> &middot;
  <a href="https://github.com/berezovyy/graft/issues">Request Feature</a>
</p>
