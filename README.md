# graft

Fork your project directory. Instant, zero-copy, disposable.

```
project/                graft fork . --name experiment
├── src/
├── node_modules/  ──►  ~/.graft/experiment/merged/   (looks identical)
├── .env                  ├── src/                     (read from original)
└── package.json          ├── node_modules/            (read from original)
                          ├── .env                     (read from original)
                          └── src/api.rs               ← only this file was changed
                                                         only this file is stored
```

One command gives you an isolated copy of any directory. No actual copying happens - it uses OverlayFS (the same thing Docker uses internally) to layer a thin writable surface over your existing files. Only what you change gets stored.

## Why

AI coding agents need to experiment. Try approach A, try approach B, run tests, compare, pick a winner. Current options are bad:

- **Working in-place** - no isolation, can't run parallel experiments
- **Copying the project** - slow, especially with `node_modules` (500MB+)
- **Docker** - overkill for "let me try something real quick"
- **Git worktrees** - still copies the working tree, doesn't capture build artifacts or configs

There was no simple tool that says: give me a cheap parallel universe of this directory.

## Install

```bash
# requires: fuse-overlayfs, fusermount3 (fuse3)
# Ubuntu/Debian
sudo apt install fuse-overlayfs fuse3

# Fedora
sudo dnf install fuse-overlayfs fuse3

# Arch
sudo pacman -S fuse-overlayfs fuse3

# then
cargo install --path .
```

If mounting fails, graft tells you exactly what's wrong and how to fix it.

## Usage

```bash
# fork a project directory
graft fork . --name experiment

# enter it - opens a shell inside the overlay
graft enter experiment

# make changes, break things, go wild
# ...then exit the shell

# see what changed
graft diff experiment
graft diff experiment --full    # unified diff

# happy with it? merge back
graft merge experiment --drop

# not happy? throw it away
graft drop experiment
```

### Parallel experiments

```bash
graft fork . --name approach-a
graft fork . --name approach-b

# two agents, two isolated workspaces, same base
claude-code --cwd $(graft path approach-a) "rewrite API with Hono" &
claude-code --cwd $(graft path approach-b) "rewrite API with tRPC" &
wait

graft diff approach-a --stat
graft diff approach-b --stat

graft merge approach-a --commit -m "switch to Hono"
graft drop approach-b
```

### Stacking (fork-of-fork)

```bash
graft fork . --name step-1
# ... make data model changes in step-1 ...

graft fork step-1 --name step-2
# ... build API on top of step-1 ...

graft tree
# main
# └── step-1
#     └── step-2
```

### Ephemeral workspaces

```bash
# auto-creates a workspace, opens a shell, destroys on exit
graft enter --ephemeral

# or run a single command
graft enter --ephemeral -- make test
```

### Snapshots

```bash
graft snap experiment create                  # checkpoint
graft snap experiment create --name before-refactor
graft snap experiment list
graft snap experiment restore before-refactor # rollback
graft snap experiment diff before-refactor    # what changed since
```

## All commands

```
fork       create a workspace from a directory
drop       remove a workspace (supports glob: "graft drop 'exp-*'")
ls         list workspaces
path       print the merged directory path
diff       show changes (--stat, --full, --files, --json, --cumulative)
enter      shell into a workspace (--ephemeral, --merge-on-exit)
merge      apply changes to base (--commit, --patch, --drop)
snap       snapshots: create, list, restore, diff, delete
tree       show workspace hierarchy
collapse   flatten a stack into one layer
nuke       remove everything
```

## How it works

graft uses [fuse-overlayfs](https://github.com/containers/fuse-overlayfs) to create a union mount:

```
merged/    ← you work here (reads from both layers)
  │
  ├── upper/   ← your changes live here (copy-on-write)
  └── lower/   ← original project (read-only, untouched)
```

When you read a file, it comes from the original. When you write, the file is copied to upper first (copy-on-write), then modified there. The original never changes. Deletes are tracked as whiteout markers. The upper directory _is_ the diff - no diffing algorithm needed.

Forking is instant (~5ms) because nothing is copied. Merging moves files from upper back to the base. Dropping unmounts and deletes the upper.

## Requirements

- Linux (OverlayFS is a Linux kernel feature)
- `fuse-overlayfs` and `fuse3` installed
- No root/sudo needed

Now let's create a separate folder, with multi file and multi phases plan to implement Network isolation │ Designed, not implemented │

## License

MIT
