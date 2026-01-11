# Flare

[![Release](https://img.shields.io/github/v/release/Velooroo/Flare)](https://github.com/Velooroo/flare/releases)
[![License](https://img.shields.io/github/license/Velooroo/Flare)](LICENSE)

**Lightweight deployment for edge devices and homelabs.**

Flare replaces complex CI/CD pipelines with a single-binary toolchain built for IoT, Raspberry Pi, and local servers.

> **No dependencies.** No Python, Node.js, or Docker required on target devices.

![Demo](assets/demo.gif)

---

## Features

- **Push-to-Deploy:** Ship from laptop to device in seconds
- **Auto-Discovery:** Finds devices via UDP broadcast
- **Built-in Gateway:** Reverse proxy for static sites and apps
- **Secure:** TLS encryption by default
- **Works Everywhere:** GitHub, GitLab, Forge, or any other git.
- **Database Support:** Auto-setup PostgreSQL, MySQL, or SQLite
- **Rollback:** Built-in versioning and rollback system
- **Hooks:** Run custom scripts before/after deployment
- **Isolation:** SystemD or chroot process isolation

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

# With GitHub
flare deploy user/my-project --github --token ghp_xxxxx

# With custom forge
flare deploy user/my-project --forge http://{ip}
```

```markdown
## Quick Start

### 1. Install

Download from [Releases](https://github.com/Velooroo/flare/releases) or build:

```bash
cargo build --release
```

### 2. Start Daemon (on target device)

```bash
flared
# Listens on :7530 (TCP) and :7001 (UDP discovery)
```

### 3. Setup Authentication

**Git credentials** (for downloading repos):
```bash
flare login
Username: your-username
Password: your-git-(token or password)
```

**Device registration** (auto-generates secure token):
```bash
flare discover
# [0] 192.168.1.50:7530 (new)

flare sync 0
# Registering token... ✓
# Name: raspberrypi
# ✓ Saved
```

### 4. Add Config to Your Project

Create `flare.toml` in repo root:

```toml
[app]
name = "my-app"
version = "1.0.0"

[run]
command = "node server.js"
port = 3000
```

### 5. Deploy

```bash
# Deploy to saved device
flare deploy user/my-project --device 0 
## (I don't remember, it's works or not.)

# Or by name
flare deploy user/my-project --device raspberrypi
```

### 6. Manage Apps

```bash
flare start my_app      # Start application
flare stop my_app       # Stop application
flare restart my_app    # Restart application
flare rollback my_app   # Rollback to previous version
```

---

## Authentication

Flare uses dual authentication for security:

### Git Authentication
Used for downloading repositories from GitHub/Forgejo/etc.

```bash
flare login
Username: myuser
Password: ghp_xxxxx  # GitHub token or Forge password
```

Stored in `~/.flare/auth.toml`

### Daemon Authentication
Used for connecting to devices. Automatically generated during sync.

```bash
flare discover    # Find devices
flare sync 0      # Generate and register token
```

**How it works:**
1. CLI generates random 32-byte token
2. Hashes it with argon2
3. Sends hash to daemon
4. Daemon stores hash in `~/.flare/daemon_tokens.toml`
5. CLI stores plain token in `~/.flare/flare.conf`

All future deploys use this token automatically.

---

## Testing Guide

### Local Testing

```bash
# 1. Start daemon
cargo run --bin flared

# 2. Auth
cargo run --bin flare -- login
# Username: test
# Password: token123

cargo run --bin flare -- discover
cargo run --bin flare -- sync 0

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

# 5. Manage
cargo run --bin flare -- stop test_app
cargo run --bin flare -- start test_app
```

### Remote Testing

```bash
# Copy binaries to device
scp target/release/flared pi@192.168.1.100:/usr/local/bin/

# SSH and start daemon
ssh pi@192.168.1.100 "flared"

# From local machine
flare discover
flare sync 0
flare deploy user/repo --device 0
```

### Troubleshooting

**"No devices configured"**
→ Run `flare discover && flare sync 0` first

**"Invalid token"**
→ Re-sync device: `flare sync 0` and replace token

**"App not found"**
→ Use `_` in app names: `flare stop user_repo` not `user/repo` (already fix it)

**Connection refused**
→ Check firewall: `sudo ufw allow 7530/tcp && sudo ufw allow 7001/udp`

**start command fails**
→ Only works for apps with `[run]` section (static sites not supported yet, see [#1](issues))

---

## Known Issues

- [x] App names with `/` must use `_` in management commands [#2]
- [ ] `flare start` doesn't work for static sites (`[web]` only apps) [#1]
- [ ] Gateway reverse proxy not implemented (static sites only)


---

## How It Works

```
CLI (your laptop)              Daemon (target device)
     |                                |
     | 1. Package repo                |
     |------------------------------->|
     |                                | 2. Download archive
     |                                | 3. Extract to ~/.flare/apps/
     |                                | 4. Read flare.toml
     |                                | 5. Run [build] if present
     |                                | 6. Setup [database] if present
     |                                | 7. Start [run] or register [web]
     |<-------------------------------|
     | 8. Receive success/error       |
```

**File Structure:**
```
~/.flare/
├── apps/
│   └── user_repo/
│       ├── current -> versions/1234567890/
│       ├── versions/
│       │   ├── 1234567890/  # Current deployment
│       │   └── 1234567880/  # Previous (for rollback)
│       ├── state.toml       # App state (PID, status)
│       └── flare.toml       # App config
└── auth.toml                # Optional: saved credentials
```

---

## Security

Flare uses **TLS** for all connections between CLI and daemon.

### Local Network
Auto-generates self-signed certificates. No configuration needed.

### Production/Internet
Set environment variables:

```bash
FLARE_TLS_CERT=/etc/letsencrypt/live/yourdomain/fullchain.pem
FLARE_TLS_KEY=/etc/letsencrypt/live/yourdomain/privkey.pem
```

Or in `.env`:
```bash
FLARE_TLS_CERT=/path/to/cert.pem
FLARE_TLS_KEY=/path/to/key.pem
```

Daemon automatically loads certificates on startup.

---

## Built-in Gateway

Flare daemon includes an HTTP gateway on port 80 that:
- Serves static sites by domain (`[web]` section)
- Proxies to running apps (coming soon)
- Handles virtual hosts automatically

**Example:**
```toml
[web]
domain = "mysite.local"
root = "./public"
```

Access via `http://mysite.local` (add to `/etc/hosts` or use local DNS).

---

## Use Cases

- **IoT Edge Deployments:** Deploy to Raspberry Pi fleet
- **Homelab CI/CD:** Simple alternative to Jenkins/GitLab CI
- **Prototype Hosting:** Quick deploys to VPS without containers
- **Cyberdeck Development:** Build and deploy on the go
- **Education:** Learn deployment without Kubernetes complexity

---

## Roadmap

### ✅ Completed (v0.2)
- [x] Basic deploy workflow
- [x] Database auto-setup (PostgreSQL, MySQL, SQLite)
- [x] Static site gateway (HTTP :80)
- [x] Health checks (single check on deploy)
- [x] Rollback system (version backup/restore)
- [x] Start/Stop/Restart via daemon
- [x] Auto-generated secure tokens (argon2)
- [x] Dual authentication (git + daemon)
- [x] UDP discovery
- [x] Device management (sync, list, remove)
- [x] TLS encryption (self-signed + custom certs)
- [x] Deployment hooks (pre/post)
- [x] Process isolation (systemd, chroot)

### 🚧 In Progress (v0.3)
- [ ] Gateway reverse proxy for APIs ([#3])
- [ ] Fix start command for `[web]` only apps ([#1])
- [ ] Auto-normalize app names with `/` ([#2])
- [ ] Continuous health monitoring (not just on deploy)

### 📋 Planned (v0.4)
- [ ] Deploy to multiple devices (`--device all`)
- [ ] Logs command (`flare logs myapp --follow`)
- [ ] Auto health endpoint injection
- [ ] Environment variable management UI

### 🔮 Future (v1.0+)
- [ ] Web dashboard
- [ ] Metrics collection & visualization
- [ ] Canary deployments
- [ ] Blue-green deployment strategy
- [ ] Multi-instance deployments (load balancing)
- [ ] Plugin system
- [ ] Windows daemon support
- [ ] Container support (optional, for isolation)
- [ ] Notifications (email, Telegram, webhooks)
- [ ] Storage integration (S3, MinIO)
- [ ] Resource limits enforcement
- [ ] Auto-scaling
- [ ] CLI autocomplete
- [ ] Configuration validation (`flare check`)

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
- `python-flask.toml` - Flask application
- `rust-actix.toml` - Actix web server
- `static-site.toml` - React/Vue/Svelte build output
- `fullstack.toml` - App with database and build steps

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
