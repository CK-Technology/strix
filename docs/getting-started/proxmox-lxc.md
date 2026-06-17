# Proxmox LXC Deployment

Running Strix in a Proxmox VE LXC container is a lightweight alternative to a
full VM: lower overhead, fast start, and easy snapshots. This guide covers both
ways to run it inside an LXC:

- **Native binary** under systemd (recommended — leanest, no Docker layer)
- **Docker-in-LXC** (if you prefer the published image)

> Object storage is I/O- and capacity-bound. Put the data directory on a dataset
> you can grow and back up, and prefer an **unprivileged** container.

## 1. Create the Container

In the Proxmox UI (**Create CT**) or via `pct` on the host. A Debian 12 template
is a good base.

```bash
# On the Proxmox host
pveam update
pveam available | grep debian-12
pveam download local debian-12-standard_12.7-1_amd64.tar.zst

pct create 110 local:vztmpl/debian-12-standard_12.7-1_amd64.tar.zst \
  --hostname strix \
  --cores 4 --memory 4096 --swap 2048 \
  --rootfs local-lvm:16 \
  --net0 name=eth0,bridge=vmbr0,ip=dhcp \
  --unprivileged 1 \
  --features nesting=1 \
  --onboot 1
```

| Setting | Why |
|---------|-----|
| `--unprivileged 1` | Safer isolation; fine for the native binary and rootless needs |
| `--features nesting=1` | Required if you will run Docker inside the LXC |
| `--cores/--memory` | Scale to workload; 4c/4G is a sane start |

## 2. Storage for Object Data

Keep Strix's data (`/var/lib/strix`) on storage you can grow and back up
independently of the container rootfs. Add a dedicated mount point:

```bash
# 200G mount backed by a Proxmox storage, mounted at /var/lib/strix in the CT
pct set 110 --mp0 local-lvm:200,mp=/var/lib/strix
```

For ZFS-backed Proxmox, a dataset gives you snapshots and quotas:

```bash
zfs create -o quota=500G rpool/data/strix
pct set 110 --mp0 /rpool/data/strix,mp=/var/lib/strix
```

Start the container and open a shell:

```bash
pct start 110
pct enter 110
```

Everything below runs **inside the container**.

## Option A — Native Binary (recommended)

### Build or fetch the binary

The simplest path is to build on a workstation and copy the release binary in,
or build inside the container (needs Rust 1.96+ and the WASM target for the
console):

```bash
apt update && apt install -y curl build-essential pkg-config libssl-dev git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
cargo install trunk

git clone https://github.com/CK-Technology/strix.git
cd strix
( cd crates/strix-gui && trunk build --release )   # builds the embedded console
cargo build --release -p strix
install -m 0755 target/release/strix /usr/local/bin/strix
```

### Create a service user and data dir

```bash
useradd -r -s /usr/sbin/nologin strix
mkdir -p /var/lib/strix
chown strix:strix /var/lib/strix
```

### systemd unit

Create `/etc/strix.env`:

```ini
STRIX_ROOT_USER=admin
STRIX_ROOT_PASSWORD=change-me
STRIX_JWT_SECRET=replace-with-openssl-rand-base64-32
STRIX_DATA_DIR=/var/lib/strix
# Bind to loopback if nginx runs in this same container; use 0.0.0.0 if a
# separate proxy host connects over the network.
STRIX_ADDRESS=0.0.0.0:9000
STRIX_CONSOLE_ADDRESS=0.0.0.0:9001
STRIX_METRICS_ADDRESS=127.0.0.1:9090
STRIX_LOG_LEVEL=info
STRIX_LOG_JSON=true
```

Create `/etc/systemd/system/strix.service`:

```ini
[Unit]
Description=Strix S3-compatible object storage
After=network-online.target
Wants=network-online.target

[Service]
User=strix
Group=strix
EnvironmentFile=/etc/strix.env
ExecStart=/usr/local/bin/strix
Restart=on-failure
RestartSec=2
# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/strix
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
```

Enable and check:

```bash
chmod 600 /etc/strix.env
systemctl daemon-reload
systemctl enable --now strix
systemctl status strix
curl -f http://localhost:9000/health/ready
```

See [Configuration](configuration.md) for every variable.

## Option B — Docker-in-LXC

Requires `--features nesting=1` on the container (set above). For overlay/keyctl
needs you may also add `keyctl=1`:

```bash
pct set 110 --features nesting=1,keyctl=1
```

Inside the container:

```bash
apt update && apt install -y ca-certificates curl
curl -fsSL https://get.docker.com | sh

docker run -d --name strix --restart unless-stopped \
  -p 9000:9000 -p 9001:9001 \
  -e STRIX_ROOT_USER=admin \
  -e STRIX_ROOT_PASSWORD=change-me \
  -v /var/lib/strix:/var/lib/strix \
  ghcr.io/ck-technology/strix:latest
```

Mounting the LXC mount point (`/var/lib/strix`) straight into the container
keeps data on your dedicated storage. Full image/Compose details:
[Docker Deployment](docker.md).

## 3. TLS / Reverse Proxy

Terminate TLS with nginx — either inside this LXC or on a separate proxy host.
Issue a wildcard cert with acme.sh (DNS-01) and drop in the shipped config:

- [Nginx Setup](nginx.md)
- [TLS with acme.sh](../guides/tls-acme.md)
- Shipped config: `deploy/nginx/strix.conf`
  (`s3.cktechnology.io` → `:9000`, `strix.cktechnology.io` → `:9001`)

If nginx runs in the **same** container, bind Strix to `127.0.0.1` and let nginx
face the network. If nginx is on a **separate** host, bind Strix to the LXC's IP
and firewall ports 9000/9001 to the proxy only.

## 4. Backups & Snapshots

Two complementary layers:

1. **Container snapshots** — `pct snapshot 110 pre-upgrade` before upgrades; or
   ZFS snapshots of the data dataset for point-in-time recovery.
2. **Application backups** — back up `/var/lib/strix` (object blobs, SQLite
   metadata/IAM DBs, and the SSE-S3 master key). See
   [Backup and Recovery](../guides/backup-recovery.md).

> The `encryption.key` under `/var/lib/strix/meta/` is required to decrypt
> SSE-S3 objects. A backup without it cannot restore encrypted data.

## Sizing Notes

| Resource | Guidance |
|----------|----------|
| CPU | 2–4 cores handles typical small/medium workloads |
| Memory | 2–4 GB; raise for high concurrency |
| Rootfs | 16 GB is plenty — data lives on the separate mount |
| Data mount | Size to your object capacity; grow the dataset as needed |

## Next Steps

- [Docker Deployment](docker.md) — image and Compose reference
- [Nginx Setup](nginx.md) / [TLS with acme.sh](../guides/tls-acme.md)
- [Configuration](configuration.md) — all settings
- [Backup and Recovery](../guides/backup-recovery.md)
