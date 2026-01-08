# ⚡ Spark

[![Release](https://img.shields.io/github/v/release/Velooroo/spark)](https://github.com/Velooroo/Spark/releases)
[![License](https://img.shields.io/github/license/Velooroo/Spark)](LICENSE)

**The decentralized deployment toolchain for the edge.**

Spark replaces complex Ansible setups and Docker registries with a lightweight, single-binary ecosystem designed for IoT devices, home labs, and cyberdecks.

> **Zero Dependencies.** No Python, no Node.js, no Docker required on the target.

## ✨ Features

- **Push-to-Deploy:** Ship code from your laptop to Raspberry Pi in seconds.
- **Auto-Discovery:** Finds devices in your local network via UDP broadcast.
- **Built-in Gateway:** Automatic reverse proxy (Virtual Hosts) for web apps.
- **Secure:** TLS encryption for all data transfers.
- **Universal:** Supports GitHub, GitLab, and self-hosted Forge servers.

---

## 🚀 Quick Start

### 1. Install (Developer & Server)
Download the binary from [Releases](https://github.com/Velooroo/Spark/releases) or build from source:

```bash
cargo install --path .
```

### 2. Setup the Target Device (Daemon)
Run this on your Raspberry Pi / VPS:

```bash
# Starts TCP listener (7530) and UDP discovery (7001)
sparkle
```

### 3. Deploy from your Machine (CLI)

Add a `spark.toml` to your project and deploy:

```bash
# Auto-discover device and deploy
spark deploy --repo user/my-project
```

---

## 🛠 Configuration (`spark.toml`)

Place this file in the root of your project.

### Static Website (Auto Nginx/Gateway)

```toml
[app]
name = "landing-page"
version = "1.0.0"

[web]
domain = "landing.local"  # Site will be available at http://landing.local:8080
web_root = "./dist"       # Folder containing index.html
```

### Backend Service (Python/Rust/Node)

```toml
[app]
name = "sensor-api"
version = "0.1.0"

[build]
command = "cargo build --release"

[run]
command = "./target/release/sensor-api"
port = 3000
```

---

## 📦 How it works

1.  **Spark CLI** packs your repository (from GitHub or Forge).
2.  Connects to **Sparkle Daemon** via TLS-encrypted TCP.
3.  **Daemon** downloads, extracts, and builds the application.
4.  **Daemon** launches the process or registers the domain in the internal Gateway.

## 🔐 Security

Spark uses **TLS** for all data transfer.
- **Local Network:** Uses self-signed certificates automatically.
- **Internet/Production:** Supports Let's Encrypt / Custom certificates via env vars:
  - `SPARK_TLS_CERT_FILE`
  - `SPARK_TLS_KEY_FILE`

---

## License

MIT © [VeloroLABS](https://github.com/Velooroo)
