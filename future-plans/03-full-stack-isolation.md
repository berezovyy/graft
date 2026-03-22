# Full Stack Isolation

Network namespaces, database forking, and service orchestration - each workspace gets its own ports, database, and running services.

## The problem

OverlayFS isolates files. But a real project isn't just code - it's code + ports + databases + running services. Two workspaces can't both bind to `:3000` on the host. Two workspaces writing to the same database corrupt each other's state.

Full stack isolation gives each workspace a complete "parallel universe" of the running application.

## What we have now

- OverlayFS workspace isolation (files only)
- `graft run` starts a process in a workspace with port allocation (5501–5600 range)
- TCP proxy for switching between workspaces
- No network isolation, no database forking, no service orchestration

## The three isolation layers

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: FILES          OverlayFS                  │   ✓ done
│  Code, configs, .env     Copy-on-write              │
├─────────────────────────────────────────────────────┤
│  Layer 2: NETWORK        Network namespace          │   planned
│  Ports, inter-service    Each workspace gets :3000  │
│  communication           No port conflicts          │
├─────────────────────────────────────────────────────┤
│  Layer 3: DATA           Database forking           │   planned
│  PostgreSQL state        Template DB or FS snapshot  │
└─────────────────────────────────────────────────────┘
```

## What's missing

### Network namespaces (CLONE_NEWNET)

A network namespace gives a process its own network stack. Inside the namespace, `:3000` is a different `:3000` than the host's. Every workspace uses the same ports. Zero config changes.

```bash
# Create namespace
ip netns add graft-ws-a

# Create virtual ethernet pair (host ↔ namespace)
ip link add veth-ws-a type veth peer name veth0 netns graft-ws-a

# Assign IPs
ip addr add 10.0.1.1/24 dev veth-ws-a                           # host side
ip netns exec graft-ws-a ip addr add 10.0.1.2/24 dev veth0      # namespace side
ip link set veth-ws-a up
ip netns exec graft-ws-a ip link set veth0 up
ip netns exec graft-ws-a ip link set lo up

# Run services INSIDE the namespace
ip netns exec graft-ws-a bash -c '
  cd ~/.graft/ws-a/merged
  next dev --port 3000 &     # :3000 inside namespace
  wrangler dev --port 8787 & # :8787 inside namespace
'
```

What services see inside the namespace:

```
Workspace A (10.0.1.2):
  Next.js       → :3000          (normal port, no offsetting)
  Worker        → :8787          (normal port)
  Worker calls  → localhost:3000  (reaches its OWN Next.js)

Workspace B (10.0.2.2):
  Next.js       → :3000          (different :3000!)
  Worker        → :8787          (different :8787!)
  Worker calls  → localhost:3000  (reaches its OWN Next.js)
```

Inter-service communication via `localhost` works because all services in the same workspace share the same namespace. No URL rewriting. The code doesn't change.

### veth pairs + iptables/nftables for namespace-to-host routing

PostgreSQL runs on the host. The namespace needs to reach it:

```bash
# Route from namespace to host
ip netns exec graft-ws-a ip route add default via 10.0.1.1

# Enable forwarding on host
sysctl -w net.ipv4.ip_forward=1
iptables -t nat -A POSTROUTING -s 10.0.0.0/16 -j MASQUERADE
```

Now processes inside the namespace reach the host's PostgreSQL at `10.0.1.1:5432`.

Subnet ID allocation: scan `state.workspaces.*.network.subnet_id` to find the next free ID, handling gaps from dropped workspaces.

### Database forking

Each workspace needs its own database state. PostgreSQL's `CREATE DATABASE ... TEMPLATE` does a server-side file-level copy:

```sql
-- Base snapshot (run migrations, seed data)
CREATE DATABASE myapp_base;

-- Fork for workspace A (~1-3s for typical dev DB)
CREATE DATABASE myapp_ws_a TEMPLATE myapp_base;

-- Fork for workspace B
CREATE DATABASE myapp_ws_b TEMPLATE myapp_base;

-- Cleanup on drop
DROP DATABASE myapp_ws_a;
```

Alternative strategies:

| Strategy             | Best for                | Speed   |
| -------------------- | ----------------------- | ------- |
| Template database    | < 100MB data            | 1–3s    |
| Schema-per-workspace | Medium, ORM supports it | < 1s    |
| btrfs/ZFS snapshot   | > 100MB data            | instant |
| PGlite/SQLite        | Prototyping             | instant |

### graft.toml project config

Optional per-project config that defines the full stack:

```toml
[project]
base = "."

[database]
type = "postgresql"
strategy = "template"
base_url = "postgresql://user:pass@localhost:5432/myapp"
template = "myapp_base"

[[services]]
name = "web"
cwd = "apps/web"
command = "bun run dev --port 3000"
port = 3000
health = "http://localhost:3000/api/health"

[[services]]
name = "worker"
cwd = "apps/worker"
command = "wrangler dev --port 8787"
port = 8787
depends_on = ["web"]

[env]
DATABASE_URL = "postgresql://user:pass@{host_ip}:5432/{db_name}"
NEXT_PUBLIC_WORKER_URL = "http://localhost:8787"
```

When present, `graft up` reads this to orchestrate the full stack. When absent, graft works as a simple overlay tool (files only).

### graft up / graft down

```bash
graft up ws-a
#   ✓ Network namespace created (10.0.1.2)
#   ✓ Database forked: myapp_ws_a (1.2s)
#   ✓ .env generated with workspace config
#   ✓ web: Next.js started on :3000 (health check passed)
#   ✓ worker: wrangler dev started on :8787
#   Stack ready.

graft down ws-a
#   ✓ Services stopped
#   ✓ Network namespace deleted
#   (database preserved for later resume)

graft drop ws-a
#   ✓ Services stopped
#   ✓ Network namespace deleted
#   ✓ Database dropped: myapp_ws_a
#   ✓ Overlay unmounted
```

`graft up` starts in dependency order with health checks. `graft down` stops services but preserves the database for resume. `graft drop` cleans up everything.

### memfd_create for shared coordination state

The proxy uses `memfd_create` for coordination: a sealed anonymous file in RAM contains the active workspace routing table. When the proxy updates the memfd on switch, all connected services see the change instantly - no files on disk, no sockets, no serialization overhead.

### pidfd_getfd for socket handoff

For true zero-downtime switching: `pidfd_getfd()` steals the listening socket from workspace A's dev server and hands it to workspace B's. The browser doesn't even need to reconnect. No WebSocket close, no HMR flicker.

```
graft switch ws-b:
  1. pidfd_getfd() → steal :3000 socket from ws-a
  2. Hand socket to ws-b's process
  3. cgroup.freeze ws-a
  4. cgroup.thaw ws-b
  5. ws-b starts serving on the same socket
  → browser sees zero interruption
```

### Switching the full stack

All ports switch atomically via the multi-port proxy:

```
localhost:3000  ──► active workspace's :3000  (Next.js)
localhost:8787  ──► active workspace's :8787  (Worker)

graft switch ws-b:
  → freeze ws-a's cgroup (atomic)
  → thaw ws-b's cgroup
  → all proxy targets update atomically
  → browser reconnects via HMR
```

Database doesn't need switching - each workspace's `.env` already points to its own database.

## Environment generation

On `graft up`, generate `.env` in the overlay upper layer with workspace-specific values:

```
DATABASE_URL=postgresql://user:pass@10.0.1.1:5432/myapp_ws_a
NEXT_PUBLIC_WORKER_URL=http://localhost:8787
```

Template variables from `graft.toml`:

- `{host_ip}` → host side of veth pair (e.g., `10.0.1.1`)
- `{db_name}` → workspace database name (e.g., `myapp_ws_a`)
- `{namespace_ip}` → namespace side IP (e.g., `10.0.1.2`)

## Phases

| #   | Name                       | What it adds                                                 |
| --- | -------------------------- | ------------------------------------------------------------ |
| 1   | Config parsing             | Parse `graft.toml`, validate, expose to commands             |
| 2   | Network namespace creation | Create netns, veth pairs, assign IPs                         |
| 3   | Network routing            | NAT, iptables/nftables, host connectivity                    |
| 4   | Database forking           | `CREATE DATABASE TEMPLATE`, connection management            |
| 5   | Service orchestration      | `graft up` starts services in order inside namespace         |
| 6   | Environment generation     | `.env` in upper layer with workspace-specific values         |
| 7   | Graceful shutdown          | `graft down` stops services, deletes namespace, preserves DB |
| 8   | Polish                     | Health checks, multi-port proxy, error recovery              |

Parallel track: database forking (phase 4) only needs config parser (phase 1), can run in parallel with network namespace work (phases 2–3). Both converge at phase 6 (environment generation needs both IPs and DB names).

## State extensions

```rust
pub struct Workspace {
    // ... existing fields ...
    pub network: Option<NetworkState>,
    pub database: Option<DatabaseState>,
    pub services: Vec<ServiceState>,
}

pub struct NetworkState {
    pub namespace: String,       // "graft-ws-a"
    pub ip: String,              // "10.0.1.2"
    pub veth_host: String,       // "veth-ws-a"
    pub veth_ns: String,         // "veth0"
    pub subnet_id: u8,           // <n> in 10.0.<n>.0/24
}

pub struct DatabaseState {
    pub name: String,            // "myapp_ws_a"
    pub template: String,        // "myapp_base"
    pub url: String,             // full connection string
}
```

All new fields are `Option`/`Vec` with `serde(default)` - existing workspaces deserialize without error.

## Dependencies

- Core workspace lifecycle - already done
- Dev server track provides `ServiceState` type, cgroup management, proxy routing
- `graft drop` cleanup order: stop services → delete namespace → drop database → unmount overlay

## New files

```
src/config.rs           - graft.toml parsing and validation
src/netns.rs            - network namespace creation and management
src/database.rs         - database forking strategies
src/commands/up.rs      - graft up command
src/commands/down.rs    - graft down command
```

## Linux primitives used

| Primitive                             | Purpose                                 |
| ------------------------------------- | --------------------------------------- |
| Network namespace (`CLONE_NEWNET`)    | Port isolation per workspace            |
| `veth` pairs                          | Connect namespace to host network       |
| `iptables` / `nftables`               | Route namespace → host (for DB access)  |
| `CREATE DATABASE TEMPLATE`            | Per-workspace database state            |
| `memfd_create` + file sealing         | Shared coordination state               |
| `pidfd_getfd`                         | Socket handoff for zero-downtime switch |
| `sched_ext` + cgroup bandwidth (6.17) | CPU priority for active workspace       |
| Per-NUMA proactive reclaim (6.17)     | Memory tiering for sleeping workspaces  |
