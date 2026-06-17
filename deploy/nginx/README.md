# Nginx Reverse Proxy for Strix

Drop-in nginx configuration that terminates TLS and proxies both Strix
listeners:

| Public hostname | Proxies to | Purpose |
|-----------------|------------|---------|
| `s3.cktechnology.io` | `127.0.0.1:9000` | S3 API (clients / SDKs) |
| `strix.cktechnology.io` | `127.0.0.1:9001` | Web console + Admin API |

Both share a single wildcard certificate for `*.cktechnology.io`.

## Files

| File | Install to |
|------|------------|
| `strix.conf` | `/etc/nginx/conf.d/strix.conf` |
| `strix-ssl-params.conf` | `/etc/nginx/snippets/strix-ssl-params.conf` |

## Setup

1. **Issue the wildcard cert** into `/etc/nginx/certs/cktechnology.io/` with
   acme.sh (DNS-01). See [`docs/guides/tls-acme.md`](../../docs/guides/tls-acme.md).

   ```
   /etc/nginx/certs/cktechnology.io/fullchain.pem
   /etc/nginx/certs/cktechnology.io/privkey.pem
   ```

2. **Bind Strix to loopback** so only nginx is exposed:

   ```bash
   strix --address 127.0.0.1:9000 --console-address 127.0.0.1:9001
   ```

   or, with Docker, publish to loopback only:

   ```yaml
   ports:
     - "127.0.0.1:9000:9000"
     - "127.0.0.1:9001:9001"
   ```

3. **Install the configs and reload:**

   ```bash
   sudo cp deploy/nginx/strix.conf            /etc/nginx/conf.d/strix.conf
   sudo cp deploy/nginx/strix-ssl-params.conf /etc/nginx/snippets/strix-ssl-params.conf
   sudo nginx -t && sudo systemctl reload nginx
   ```

4. **Point clients at the proxy** (path-style):

   ```bash
   aws --endpoint-url https://s3.cktechnology.io s3 ls
   ```

   The console is at `https://strix.cktechnology.io`.

## Adapting to your own domain

Replace the hostnames and cert paths:

- `s3.cktechnology.io` / `strix.cktechnology.io` → your hostnames
- `/etc/nginx/certs/cktechnology.io/*` → your cert/key paths

For older nginx (< 1.25.1) replace `http2 on;` with the legacy
`listen 443 ssl http2;` form.

## Why these settings matter (S3 API)

The S3 API is stricter than a typical web app:

- **`proxy_set_header Host $host;`** — AWS SigV4 signs the `Host` header. Rewrite
  it and every request fails with `SignatureDoesNotMatch`.
- **`merge_slashes off;`** — preserves object keys containing `//`.
- **`client_max_body_size 0;`** — uploads/parts can be gigabytes.
- **`proxy_request_buffering off;`** — stream large bodies instead of spooling
  to disk.

Full explanation and troubleshooting: [`docs/guides/reverse-proxy.md`](../../docs/guides/reverse-proxy.md).
