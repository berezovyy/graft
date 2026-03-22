# Claude Code Integration

The agent story — auto-sandboxing, sub-agent isolation, session lifecycle, and parallel experiments.

## The principle

The agent doesn't know it's in an overlay. It sees a normal directory. All orchestration happens outside the agent, at the graft level. Any agent with bash access — Claude Code, Cursor, Aider, custom scripts — can orchestrate workspaces by calling `graft` commands directly. No SDK, no server, no protocol.

## Three integration models

### Model 1: Human orchestrates (manual)

The human creates workspaces and points Claude Code at them:

```bash
# Terminal 1 — task A
graft fork . --name add-auth
claude --cwd $(graft path add-auth)
# "Add JWT authentication to the API"

# Terminal 2 — task B
graft fork . --name fix-layout
claude --cwd $(graft path fix-layout)
# "Fix the grid layout on the dashboard page"

# Terminal 3 — review
graft diff add-auth
graft diff fix-layout

graft merge add-auth --commit -m "add JWT auth"
graft merge fix-layout --commit -m "fix dashboard grid"
```

Each session sees a normal directory. Zero integration code needed.

### Model 2: Agent self-orchestrates (CLI)

The agent uses its bash tool to call graft commands directly:

```
User: "Try both Tailwind and CSS modules, compare them"

Agent runs: graft fork . --name try-tailwind
Agent runs: graft fork . --name try-css-modules

[Agent edits files in each workspace]

Agent runs: graft diff try-tailwind --json
Agent runs: graft diff try-css-modules --json

Agent to user: "Both pass tests. Tailwind: 3 files, 45 lines.
               CSS modules: 5 files, 72 lines. Which do you prefer?"

User: "Go with Tailwind"

Agent runs: graft merge try-tailwind --commit --drop
Agent runs: graft drop try-css-modules
```

### Model 3: Sub-agent sandboxing (automatic)

Every sub-agent automatically gets its own overlay:

```
Main Claude Code session (PID 1000)
  working in: /home/user/my-project (real directory)
  │
  ├── Agent "research API options" (PID 1001)
  │     working in: ~/.graft/agent-1001/merged (overlay)
  │     → reads freely, writes are isolated
  │     → when done, workspace auto-dropped (read-only research)
  │
  ├── Agent "implement feature X" (PID 1002)
  │     working in: ~/.graft/agent-1002/merged (overlay)
  │     → makes real code changes, isolated from main session
  │     → parent reviews diff, merges if good
  │
  └── Agent "implement feature Y" (PID 1003)
        working in: ~/.graft/agent-1003/merged (overlay)
        → runs in parallel with 1002, zero conflicts
        → parent merges after 1002 is done
```

## What's missing

### Auto-sandbox via hooks

Claude Code supports hooks — shell commands that run on events. Graft integrates for fully automatic sandboxing:

```json
// ~/.claude/settings.json
{
  "hooks": {
    "session_start": [
      {
        "command": "graft fork . --name session-$CLAUDE_SESSION_ID --session $CLAUDE_SESSION_ID",
        "description": "Auto-create graft workspace for every session"
      }
    ],
    "session_end": [
      {
        "command": "graft diff session-$CLAUDE_SESSION_ID && echo 'Review changes. Run: graft merge session-$CLAUDE_SESSION_ID --commit'",
        "description": "Show diff on session end"
      }
    ]
  }
}
```

Every Claude Code session starts in its own overlay without the user or agent doing anything. Session binding happens automatically via `GRAFT_SESSION` env var.

### Sub-agent isolation

Parent agent orchestrates sub-agents using graft fork + `--cwd`:

```
Today (no isolation):
  Agent(prompt="do X")
  → subprocess works in same directory
  → can conflict with parent and other sub-agents

With graft:
  Parent runs: graft fork . --name agent-<task>
  Parent runs: claude --cwd $(graft path agent-<task>) "do X"
  → subprocess works in its own overlay
  → fully isolated from parent and other sub-agents
  → on return: parent reviews diff, merges or drops

  Parent runs: graft diff agent-<task>
  Parent runs: graft merge agent-<task> --commit  # if good
  Parent runs: graft drop agent-<task>            # if bad
```

100x faster than git worktree isolation — overlays are instant, no copying, no checkout.

### Session lifecycle

```
┌─────────────────────────────────────────────────────┐
│  1. Session starts                                  │
│     └── graft fork . --name session-<id>            │
│         └── overlay mount (instant)                 │
│         └── fanotify watcher starts                 │
│         └── change log begins recording             │
│                                                     │
│  2. Agent works                                     │
│     └── all file writes go to upper layer           │
│     └── every write logged with session ID + PID    │
│     └── auto-snapshots every 30s or 10 changes      │
│                                                     │
│  3. Agent spawns sub-agents                         │
│     └── each sub-agent gets nested overlay          │
│     └── sub-agent changes isolated from parent      │
│     └── parent merges sub-agent results selectively │
│                                                     │
│  4. Session ends                                    │
│     └── final diff presented to user                │
│     └── user reviews: merge / partial merge / drop  │
│     └── workspace cleaned up                        │
│     └── change log archived (linked to session)     │
└─────────────────────────────────────────────────────┘
```

Session history persists after completion:

```bash
# "What did session abc123 do?"
graft log --session cc-abc123
#  11:05:32  create   src/auth/jwt.ts
#  11:05:33  modify   src/middleware/index.ts
#  11:07:12  modify   src/auth/jwt.ts        (iteration)

# "Show snapshots from that session"
graft log --session cc-abc123 --snapshots
#  snapshot-0001  3 files changed  (initial implementation)
#  snapshot-0002  4 files changed  (added tests)
#  snapshot-0003  4 files changed  (final — tests passing)
```

### graft race — parallel A/B testing

Wrap the parallel experiment pattern into a single command:

```bash
graft race --base . --names tailwind,css-modules \
  -- "implement with Tailwind" \
  -- "implement with CSS modules"
```

Under the hood:
1. Fork N workspaces from the same base
2. Run each agent in parallel inside its own overlay
3. Wait for all to finish
4. Present diffs side-by-side
5. User picks a winner, graft merges it

### Stacked agent workflows

Agent builds incrementally with reviewable checkpoints:

```bash
graft fork . --name step-1
claude --cwd $(graft path step-1) "add the data model"
graft diff step-1                  # review step 1

graft fork step-1 --name step-2
claude --cwd $(graft path step-2) "add the API endpoints"
graft diff step-2                  # only step-2's changes
graft diff step-2 --cumulative     # everything from root
```

### Multiple sessions in parallel

3 terminal tabs, each running Claude Code on different tasks against the same project:

```
Tab 1: claude "add authentication"
  → workspace: auth-session
  → changes: src/auth/*, src/middleware/*

Tab 2: claude "redesign the dashboard"
  → workspace: dashboard-session
  → changes: src/components/Dashboard.tsx, src/styles/*

Tab 3: claude "write API tests"
  → workspace: tests-session
  → changes: tests/api/*.test.ts

All three run simultaneously, zero conflicts.
Each sees the same base project + only its own changes.
```

Merge order matters if workspaces touch the same files:

```bash
graft merge auth-session --commit         # clean
graft merge dashboard-session --commit    # clean (different files)
graft merge tests-session --commit        # clean (different files)

# If two touch the same file:
# WARNING: src/App.tsx was modified by both workspaces.
# Options: --force, --skip-conflicts, --three-way
```

## Programmatic interface

All graft commands support `--json` for structured output and use well-defined exit codes:

```bash
graft fork . --name ws-a --json    # {"name":"ws-a","path":"/...","status":"mounted"}
graft diff ws-a --json             # structured diff with additions/deletions per file
graft ls --json                    # list all workspaces

# Exit codes: 0 = success, 1 = error, 2 = invalid arguments
```

Environment variables:

| Variable | Description |
|----------|-------------|
| `GRAFT_HOME` | Base directory for workspace storage (default: `~/.graft`) |
| `GRAFT_SESSION` | Session ID for change attribution |
| `GRAFT_WORKSPACE` | Set inside workspace by `graft enter` |
| `GRAFT_BASE` | Base project path, set inside workspace |
| `GRAFT_UPPER` | Upper (changed files) path, set inside workspace |

## Failure modes

**Agent crashes mid-work:** workspace persists, `graft diff` shows partial changes, human can inspect or drop.

**Agent corrupts the workspace:** only the overlay is affected, base project untouched. `graft drop` cleans up completely.

**Multiple agents in same workspace:** don't do this. One workspace = one agent. For collaboration, use stacked workspaces.

## Dependencies

- Core workspace lifecycle — already done
- Change tracking track provides session binding, change log, `graft log`/`graft blame`
- `graft race` needs parallel fork + process management
- `--json` output needs to be added to existing commands (fork, diff, ls, merge)

## Architecture diagram

```
┌──────────────────────────────────────────────────────────┐
│                    Human                                  │
│   browser → localhost:3000 → graft proxy → active ws     │
└───────────────┬──────────────────────────┬───────────────┘
                │                          │
    ┌───────────▼──────────┐     ┌────────▼────────────┐
    │  Claude Code Tab 1   │     │  Claude Code Tab 2  │
    │  session: cc-abc123  │     │  session: cc-def456 │
    └───────────┬──────────┘     └────────┬────────────┘
                │                          │
    ┌───────────▼──────────┐     ┌────────▼────────────┐
    │  graft workspace     │     │  graft workspace    │
    │  name: auth-session  │     │  name: layout-fix   │
    │  overlay on project  │     │  overlay on project │
    │  changes: 12 files   │     │  changes: 3 files   │
    └───────────┬──────────┘     └────────┬────────────┘
                │                          │
    ┌───────────▼──────────────────────────▼───────────┐
    │          Base Project (untouched)                 │
    │          /home/user/my-project                   │
    └──────────────────────────────────────────────────┘
```
