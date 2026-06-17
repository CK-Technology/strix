# TLS Certificates with acme.sh (DNS-01)

Strix sits behind a TLS-terminating reverse proxy ([nginx](../getting-started/nginx.md)).
This guide issues a **wildcard certificate** for `*.cktechnology.io` with
[acme.sh](https://github.com/acmesh-official/acme.sh) using **DNS-01**
validation, so a single cert covers both `s3.cktechnology.io` and
`strix.cktechnology.io`.

We use **acme.sh, not certbot** — it is a single dependency-free shell script
with first-class DNS API support for Cloudflare, Azure, and ~150 other
providers, and it installs its own auto-renew cron/timer.

## Why DNS-01 (and a wildcard)

| | HTTP-01 | DNS-01 |
|---|---------|--------|
| Validation | nginx serves a token on `:80` | A TXT record in your DNS zone |
| Wildcards | Not supported | **Supported** (`*.cktechnology.io`) |
| Port 80 reachable? | Required | Not required |
| Best for | A single public host | Multiple subdomains / one cert for S3 + console |

One wildcard cert for `*.cktechnology.io` means you never re-issue when adding
another subdomain, and nginx only loads one cert/key pair.

## Install acme.sh

```bash
curl https://get.acme.sh | sh -s email=admin@cktechnology.io
source ~/.bashrc   # or re-open the shell so the `acme.sh` alias is available
```

Use Let's Encrypt as the CA (acme.sh defaults to ZeroSSL):

```bash
acme.sh --set-default-ca --server letsencrypt
```

## Option A — Cloudflare DNS

### 1. Create a scoped API token

In the Cloudflare dashboard → **My Profile → API Tokens → Create Token**, use
the **Edit zone DNS** template:

- **Permissions:** `Zone → DNS → Edit`
- **Zone Resources:** `Include → Specific zone → cktechnology.io`

Record the token. Also note the **Account ID** (zone Overview page).

### 2. Export credentials and issue

```bash
export CF_Token="cloudflare-api-token"
export CF_Account_ID="cloudflare-account-id"

acme.sh --issue --dns dns_cf \
  -d cktechnology.io \
  -d '*.cktechnology.io'
```

acme.sh stores these credentials in `~/.acme.sh/account.conf` and reuses them on
renewal — you only export them once.

## Option B — Azure DNS

### 1. Create a service principal scoped to the DNS zone

```bash
# Create the app registration / service principal
az ad sp create-for-rbac --name "acme-cktechnology" --skip-assignment

# Output gives appId (AZUREDNS_APPID), password (AZUREDNS_CLIENTSECRET),
# and tenant (AZUREDNS_TENANTID).

# Grant it "DNS Zone Contributor" on just the zone
az role assignment create \
  --assignee "<appId>" \
  --role "DNS Zone Contributor" \
  --scope "/subscriptions/<sub-id>/resourceGroups/<rg>/providers/Microsoft.Network/dnszones/cktechnology.io"
```

### 2. Export credentials and issue

```bash
export AZUREDNS_SUBSCRIPTIONID="subscription-id"
export AZUREDNS_TENANTID="tenant-id"
export AZUREDNS_APPID="app-id"
export AZUREDNS_CLIENTSECRET="client-secret"

acme.sh --issue --dns dns_azuredns \
  -d cktechnology.io \
  -d '*.cktechnology.io'
```

> Running acme.sh on an Azure VM with a managed identity? Skip the service
> principal and use `export AZUREDNS_MANAGEDIDENTITY=true` instead.

## Install the Cert for nginx

Do **not** point nginx at the files under `~/.acme.sh/` directly. Use
`--install-cert` so acme.sh copies the cert into place and reloads nginx on
every renewal:

```bash
sudo mkdir -p /etc/nginx/certs/cktechnology.io

acme.sh --install-cert -d cktechnology.io \
  --key-file       /etc/nginx/certs/cktechnology.io/privkey.pem \
  --fullchain-file /etc/nginx/certs/cktechnology.io/fullchain.pem \
  --reloadcmd      "systemctl reload nginx"
```

These are exactly the paths the shipped config
[`deploy/nginx/strix.conf`](https://github.com/CK-Technology/strix/blob/main/deploy/nginx/strix.conf)
expects:

```nginx
ssl_certificate     /etc/nginx/certs/cktechnology.io/fullchain.pem;
ssl_certificate_key /etc/nginx/certs/cktechnology.io/privkey.pem;
```

## Auto-Renewal

The installer adds a daily cron entry (or systemd timer) that renews certs
~30 days before expiry and runs the `--reloadcmd`. Verify and dry-run:

```bash
acme.sh --list                 # show certs and next renewal date
acme.sh --renew -d cktechnology.io --force   # test a renewal now
```

Because validation is DNS-01, renewals need the same `CF_*` / `AZUREDNS_*`
credentials — acme.sh persisted them in `~/.acme.sh/account.conf`, so renewal is
hands-off.

## Verify

```bash
# Cert chain and SAN list
echo | openssl s_client -connect s3.cktechnology.io:443 -servername s3.cktechnology.io 2>/dev/null \
  | openssl x509 -noout -subject -ext subjectAltName -dates

# End-to-end through the proxy
aws --endpoint-url https://s3.cktechnology.io s3 ls
curl https://strix.cktechnology.io/health/ready
```

## Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| `invalid domain` / TXT not found | DNS API credentials wrong or the SP/token lacks edit rights on the zone |
| Wildcard not in cert | You must issue `-d '*.cktechnology.io'` (quote it so the shell doesn't glob) |
| nginx still serves old cert | `--install-cert`/`--reloadcmd` not configured; nginx loaded `~/.acme.sh` paths directly |
| Rate-limited by Let's Encrypt | Use `--staging` while testing, then re-issue without it |

## Next Steps

- [Nginx Setup](../getting-started/nginx.md) — wire the cert into the proxy
- [Reverse Proxy & TLS](reverse-proxy.md) — full proxy reference and S3 deep-dive
