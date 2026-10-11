# Flare

[![Release](https://img.shields.io/github/v/release/Velooroo/Flare)](https://github.com/Velooroo/flare/releases)
[![License](https://img.shields.io/github/license/Velooroo/Flare)](LICENSE)

**Lightweight deployment for edge devices and homelabs.**

Flare replaces complex CI/CD pipelines with a single-binary toolchain built for IoT, Raspberry Pi, and local servers.

> **No runtime dependencies.** No Python, Node.js, or Docker required to run the
> toolchain itself (Docker is only needed if you ask Flare to provision
> PostgreSQL/MySQL containers).

![Demo](assets/demo.gif)

---

## Features

- **Push-to-Deploy:** Ship from laptop to device in seconds
- **Auto-Discovery:** Finds devices via UDP broadcast
- **Built-in Gateway:** Static sites + reverse proxy for apps (HTTP or HTTPS)
- **Secure:** TLS by default; device tokens required for deploy **and** management
- **Works Everywhere:** GitHub, Gitea/Forgejo, or any other git — pick the branch
- **Database Support:** Auto-setup PostgreSQL, MySQL, or SQLite
- **Rollback:** Versioned deploys with one-command rollback (keeps the last 4)
- **Hooks:** Run custom scripts before/after deployment
- **Isolation:** Process groups, systemd units, or chroot
- **Windows & Unix:** Daemon builds and runs on Linux, macOS and Windows

---

## Quick Start

### 1. Install

Download from [Releases](https://github.com/Velooroo/Flare/releases) or build:

```bash
cargo build --release
```

### 2. Start Daemon (on target device)

Run on your Raspberry Pi, VPS, or homelab server:

```bash
flared
# Listens on :7530 (TCP) and :7001 (UDP discovery)
```

### 3. Add Config to Your Project

Create `flare.toml` in your repo root:

```toml
[app]
name = "my-app"
version = "1.0.0"

[run]
command = "node server.js"
port = 3000
```

### 4. Deploy

```bash
# Auto-discover device in local network
flare deploy user/my-project

# Or specify host
flare deploy user/my-project --host 192.168.1.100

# Deploy a specific branch (default: main)
flare deploy user/my-project --branch develop

# With GitHub token
flare deploy user/my-project --github --token ghp_xxxxx

# With a custom forge (Gitea/Forgejo base URL)
flare deploy user/my-project --forge http://192.168.1.50:3000
```

---

## Authentication

Flare uses dual authentication for security:

### Git Authentication
Used for downloading repositories from GitHub/Forgejo/etc.

```bash
flare auth login
Username: myuser
Password: ghp_xxxxx  # GitHub token or Forge password
```

Stored in `~/.flare/auth.toml` (permissions restricted to the owner).

### Daemon Authentication
Used for connecting to devices. Generated during sync and required for every
control message (deploy, start/stop/restart/rollback, health).

```bash
flare discover          # Find devices
# On the target device, `flared` prints a one-time pairing code on first start:
#   First-time pairing code: A1B2C3
flare sync 0 --code A1B2C3
```

**How it works:**
1. CLI generates random 32-byte token
2. Hashes it with argon2
3. Sends the hash to the daemon — the *first* enrollment must also present the
   one-time pairing code; later enrollments must prove an existing token
4. Daemon stores the hash in `~/.flare/daemon_tokens.toml`
5. CLI stores the plain token in `~/.flare/flare.conf`

All future deploys **and** management commands use this token automatically when
you pass `--device <id|name>` (or when the host:port matches a saved device).

---

## Testing Guide

### Local Testing

```bash
# 1. Start daemon
cargo run --bin flared

# 2. Auth (git credentials)
cargo run --bin flare -- auth login
# Username: test
# Password: token123

# Pair with the daemon (copy its startup pairing code)
cargo run --bin flare -- discover
cargo run --bin flare -- sync 0 --code A1B2C3

# 3. Create test repo (or use existing)
mkdir test-app && cd test-app
echo 'console.log("Hello");' > index.js

cat > flare.toml << EOF
[app]
name = "test"
version = "1.0.0"

[run]
command = "node index.js"
EOF

git init && git add . && git commit -m "init"
# Push to your forge

# 4. Deploy
cargo run --bin flare -- deploy youruser/test-app --device 0

# 5. Manage (management needs the token too: pass --device)
cargo run --bin flare -- stop test_app --device 0
cargo run --bin flare -- start test_app --device 0
cargo run --bin flare -- rollback test_app --device 0
```

### Remote Testing

```bash
# Copy binaries to device
scp target/release/flared pi@192.168.1.100:/usr/local/bin/

# SSH and start daemon
ssh pi@192.168.1.100 "flared"

# From local machine
flare discover
flare sync 0 --code A1B2C3
flare deploy user/repo --device 0
```

### Troubleshooting

**"No devices configured"**
→ Run `flare discover && flare sync <range> --code <pairing-code>` first

**"Invalid token"**
→ The device either isn't paired or the token changed. Use `--device <id|name>`,
  or re-sync: `flare sync 0 --code <pairing-code>`

**"App not found"**
→ You can pass either the repo name (`user_repo`) or the `[app].name` from
  `flare.toml`; both are resolved.

**Connection refused**
→ Check firewall: `sudo ufw allow 7530/tcp && sudo ufw allow 7001/udp`

**"start" does nothing / fails**
→ `start`/`stop`/`restart`/`rollback` only apply to apps with a `[run]` section.
  Static (`[web]`-only) sites are served by the gateway directly.
→ On Windows, `docker`/`sqlite3` must be on PATH for database features.

**systemd isolation fails**
→ `[isolation] type = "systemd"` is Unix-only and needs root (writes a unit to
  `/etc/systemd/system`).

---

## Known Issues

- [x] App names with `/` are normalized, and both repo names and `[app].name`
      resolve to the same app
- [x] Reverse proxy (API apps) implemented in the gateway
- [ ] No log streaming yet (`flare logs`) [#1]
- [ ] Secrets/notify/storage/metrics sections are parsed but not acted on
- [ ] Health checks only run once on deploy (no continuous monitoring)


---

## How It Works

```
CLI (your laptop)              Daemon (target device)
     |                                |
     | 1. Send repo + branch          |
     |------------------------------->|
     |                                | 2. Download archive
     |                                | 3. Extract to versions/<ts>/
     |                                | 4. Read flare.toml
     |                                | 5. Run [build] if present
     |                                | 6. Setup [database] if present
     |                                | 7. Start [run], register [web] or systemd unit
     |<-------------------------------|
     | 8. Receive success/error       |
```

Each deploy lands in a new `versions/<timestamp>/`; `current` points at the live
one and the last 4 versions are kept, which is what makes `flare rollback` work.

**File Structure:**
```
~/.flare/
├── apps/
│   └── user_repo/
│       ├── current            # text file with the live version name
│       ├── versions/
│       │   ├── 1234567890/    # Current deployment (flare.toml lives here)
│       │   └── 1234567880/    # Previous (for rollback)
│       └── state.toml         # App state (PID, status)
└── auth.toml                  # Optional: saved credentials
```

---

## Security

Flare uses **TLS** for all connections between CLI and daemon, and **every
control message** (deploy, start/stop/restart/rollback, health) requires a valid
daemon token. The first pairing additionally needs the one-time code the daemon
prints at startup.

### Local Network
The daemon auto-generates a self-signed certificate; the CLI skips certificate
verification for `localhost`/private IPs. No configuration needed.

### Production/Internet
Use real certificates:

```bash
FLARE_TLS_CERT=/etc/letsencrypt/live/yourdomain/fullchain.pem
FLARE_TLS_KEY=/etc/letsencrypt/live/yourdomain/privkey.pem
```

The daemon loads them on startup (and the gateway serves HTTPS with them too).
Credentials and tokens are stored with owner-only permissions (`0600`).

---

## Built-in Gateway

Flare daemon includes an HTTP(S) gateway (default port 80, override with
`FLARE_GATEWAY_PORT`) that:

- Serves static sites by domain (`[web]` section)
- Reverse-proxies running apps (forwards method, headers and body)
- Handles virtual hosts automatically
- Answers a health probe at `/.flare/health`
- Serves **HTTPS** when `FLARE_TLS_CERT` / `FLARE_TLS_KEY` are set, otherwise HTTP

**Example:**
```toml
[web]
domain = "mysite.local"
root = "./public"
```

Access via `http://mysite.local` (add to `/etc/hosts` or use local DNS).

---

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FLARE_PORT` | `7530` | Daemon TCP control port |
| `FLARE_DISCOVERY_PORT` | `7001` | Daemon UDP discovery port |
| `FLARE_GATEWAY_PORT` | `80` | Gateway HTTP(S) port |
| `FLARE_APPS_DIR` | `~/.flare/apps` | Where apps are stored |
| `FLARE_TLS_CERT` / `FLARE_TLS_KEY` | *(self-signed)* | TLS certificate/key for the daemon and gateway |

---

## Use Cases

- **IoT Edge Deployments:** Deploy to Raspberry Pi fleet
- **Homelab CI/CD:** Simple alternative to Jenkins/GitLab CI
- **Prototype Hosting:** Quick deploys to VPS without containers
- **Cyberdeck Development:** Build and deploy on the go
- **Education:** Learn deployment without Kubernetes complexity

---

## Roadmap

### ✅ Completed (v0.3)
- [x] Basic deploy workflow (async download, branch selection)
- [x] Database auto-setup (PostgreSQL, MySQL, SQLite) with readiness checks
- [x] Static site gateway (HTTP :80 or HTTPS)
- [x] Gateway reverse proxy for APIs
- [x] Versioned deploys + working rollback (keeps last 4)
- [x] Start/Stop/Restart via daemon (process groups, no zombies)
- [x] Token auth on deploy **and** management, one-time pairing code
- [x] Auto-generated secure tokens (argon2)
- [x] UDP discovery + device management (sync, list, remove)
- [x] TLS encryption (self-signed + custom certs)
- [x] Deployment hooks (pre/post)
- [x] Isolation: process groups, systemd units (autostart), chroot
- [x] Windows daemon support (cross-platform via `#[cfg]`)

### 🚧 In Progress (v0.4)
- [ ] Continuous health monitoring (not just on deploy)
- [ ] Log streaming (`flare logs myapp --follow`)
- [ ] Secrets / notify / storage / metrics sections
- [ ] Per-IP rate limiting

### 📋 Planned (v0.5)
- [ ] Deploy to multiple devices (`--device all`)
- [ ] Blue-green / canary deployment strategy
- [ ] Environment variable management UI
- [ ] Configuration validation (`flare check`)

### 🔮 Future (v1.0+)
- [ ] Web dashboard
- [ ] Metrics collection & visualization
- [ ] Multi-instance deployments (load balancing)
- [ ] Plugin system
- [ ] Container support (optional, for isolation)
- [ ] Notifications (email, Telegram, webhooks)
- [ ] Storage integration (S3, MinIO)
- [ ] Resource limits enforcement
- [ ] Auto-scaling
- [ ] CLI autocomplete

### 💡 Ideas Under Discussion
- [ ] GitOps mode (watch repo for changes)
- [ ] Secrets encryption at rest
- [ ] Service mesh integration
- [ ] ARM64 optimizations
- [ ] Edge function runtime
- [ ] Built-in monitoring agent

---

## Examples

Check `examples/` directory for production-ready configs:

- `nodejs-api.toml` - Express.js API
- `python-api.toml` - Flask/Django application
- `website.toml` - Static site
- `full-example.toml` - App with database and build steps

See also `examples/CONFIG.md` for the full `flare.toml` reference.

---

## Contributing

Contributions welcome! Please:
1. Fork the repo
2. Create feature branch
3. Test locally
4. Submit PR

---

## License

Apache-2.0 © [VeloroLABS](https://github.com/Velooroo)
