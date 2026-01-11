# Flare Configuration Guide

## Basic Structure

Every `flare.toml` needs at least:

```toml
[app]
name = "your-app-name"
version = "1.0.0"
```

---

## App Types

### 1. Static Website
```toml
[app]
name = "my-site"
version = "1.0.0"

[web]
domain = "mysite.local"
root = "./build"
```

### 2. Backend Service
```toml
[app]
name = "my-api"
version = "1.0.0"

[run]
command = "node server.js"
port = 3000
```

### 3. App with Build Step
```toml
[app]
name = "my-rust-app"
version = "1.0.0"

[build]
command = "cargo build --release"

[run]
command = "./target/release/my-app"
port = 8080
```

### 4. App with Database
```toml
[app]
name = "blog"
version = "1.0.0"

[run]
command = "python app.py"
port = 5000

[database]
type = "postgres"
name = "blog_db"
user = "admin"
password = "secret"
port = 5432
preseed = "./db/schema.sql"
```

---

## All Sections Reference

### [app] (required)
```toml
[app]
name = "my-app"      # unique name
version = "1.0.0"    # semver
```

### [build]
```toml
[build]
command = "npm install && npm run build"
```

### [run]
```toml
[run]
command = "node server.js"
port = 3000  # optional, used for health checks
```

### [web]
```toml
[web]
domain = "mysite.local"  # creates virtual host on gateway
root = "./dist"          # folder with index.html
```

### [database]
```toml
[database]
type = "postgres"        # postgres, mysql, sqlite
name = "mydb"
user = "admin"           # optional, default: postgres/root
password = "secret"      # optional
port = 5432              # optional, auto-finds free port if busy
preseed = "./init.sql"   # optional, runs after DB created
```

### [health]
```toml
[health]
url = "http://localhost:3000/health"
timeout = 30  # seconds
```

### [isolation]
```toml
[isolation]
type = "systemd"  # systemd, chroot, or omit for none
```

### [hooks]
```toml
[hooks]
pre_deploy = "npm test"
post_deploy = "curl https://api.slack.com/notify"
```

### [env]
```toml
[env]
NODE_ENV = "production"
DATABASE_URL = "postgres://localhost/mydb"
API_KEY = "secret123"
```

---

## Advanced Sections (planned/partial support)

### [resource_limits]
```toml
[resource_limits]
memory = "512MB"
cpu = "1.0"
timeout = "300s"
```

### [secrets]
```toml
[secrets]
API_KEY = "env:MY_API_KEY"
DB_PASSWORD = "env:DATABASE_PASSWORD"
```

### [notify]
```toml
[notify]
on_success = ["mailto:admin@example.com"]
on_fail = ["telegram:@devops"]
```

### [storage]
```toml
[storage]
type = "s3"
bucket = "my-bucket"
endpoint = "https://s3.amazonaws.com"
access_key = "AKIA..."
secret_key = "..."
```

### [strategy]
```toml
[strategy]
type = "canary"      # canary, bluegreen, rolling
percent = 20
wait_time = "60s"
```

### [metrics]
```toml
[metrics]
pushgateway = "http://prometheus:9091"
collect = ["cpu", "memory", "requests"]
```

---

## Full Examples

### Node.js API with PostgreSQL
```toml
[app]
name = "api"
version = "1.0.0"

[build]
command = "npm install"

[run]
command = "node server.js"
port = 3000

[database]
type = "postgres"
name = "api_db"
preseed = "./db/schema.sql"

[health]
url = "http://localhost:3000/health"
timeout = 10

[hooks]
pre_deploy = "npm test"
post_deploy = "echo 'Deployed!'"

[env]
NODE_ENV = "production"
```

### Python Flask with MySQL (I don't try use MySQL, I'm not sure that it works 100% correctly.)
```toml
[app]
name = "flask-app"
version = "1.0.0"

[build]
command = "pip install -r requirements.txt"

[run]
command = "python app.py"
port = 5000

[database]
type = "mysql"
name = "flask_db"
user = "flask"
password = "secret"

[isolation]
type = "systemd"
```

### Static React Site
```toml
[app]
name = "react-app"
version = "1.0.0"

[build]
command = "npm install && npm run build"

[web]
domain = "myapp.local"
root = "./build"
```

### Rust Service with SQLite
```toml
[app]
name = "rust-api"
version = "1.0.0"

[build]
command = "cargo build --release"

[run]
command = "./target/release/rust-api"
port = 8080

[database]
type = "sqlite"
name = "app.db"
preseed = "./schema.sql"
```

---

## Tips

- `name` must be unique across deployments
- `port` in `[run]` is used for health checks
- `domain` in `[web]` creates virtual host on port 80
- Database ports auto-increment if busy (5432 → 5433 → ...)
- All sections except `[app]` are optional
- Use `_` instead of `/` in app names for management commands (is no longer a necessity)

---

## Quick Copy-Paste

| Type | Template |
|------|----------|
| Node.js API  | `examples/nodejs-api.toml` |
| Python Flask | `examples/python-flask.toml` |
| Rust Service | `examples/rust-service.toml` |
| Static Site  | `examples/static-site.toml` |
| Full Stack   | `examples/fullstack.toml` |
