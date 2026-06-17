# Strix Documentation

S3-compatible object storage server written in Rust.

## Getting Started

- [Quickstart](getting-started/quickstart.md) - Get running in minutes
- [Docker Deployment](getting-started/docker.md) - Image, Compose stack, and all settings
- [Proxmox LXC](getting-started/proxmox-lxc.md) - Running Strix in a PVE container
- [Nginx Setup](getting-started/nginx.md) - TLS termination walkthrough
- [Configuration](getting-started/configuration.md) - All settings and environment variables

## Reference

- [CLI Reference (sx)](reference/cli.md) - Command-line tool documentation
- [Admin API](reference/admin-api.md) - REST API for server management
- [S3 API](reference/s3-api.md) - S3 protocol reference
- [S3 Compatibility](reference/s3-compatibility.md) - API compatibility matrix

## Guides

- [IAM and Policies](guides/iam-policies.md) - Users, groups, and access control
- [SSO/OIDC](guides/sso-oidc.md) - Single sign-on integration
- [Entra ID (Azure AD) SSO](guides/entra-sso.md) - Step-by-step Entra setup
- [Email Alerts & Reports](guides/email-alerts.md) - SMTP relay, alerts, and usage reports
- [Reverse Proxy & TLS](guides/reverse-proxy.md) - nginx termination for the S3 and console endpoints
- [TLS with acme.sh](guides/tls-acme.md) - Wildcard certs via Cloudflare/Azure DNS (DNS-01)
- [Private over Tailscale](guides/tailscale.md) - Internal-only deployment on a tailnet
- [Backup and Recovery](guides/backup-recovery.md) - Backing up Strix's own data
- [S3 Backup Targets](guides/backup-targets.md) - Using Strix as a backup destination (restic, rclone, Veeam)
- [Observability](guides/observability.md) - Metrics, logging, and tracing

## Internals

- [Architecture](internals/architecture.md) - System design and crate structure

## Testing

- [Tool Compatibility](testing/tool-compatibility.md) - AWS CLI, restic, rclone, s3cmd validation

## Security

- [Accepted Advisories](advisories/accepted.md) - Knowingly accepted advisories (currently none)
- [Resolved Advisories](advisories/resolved.md) - Advisories cleared by dependency updates

## Quick Links

| Port | Service |
|------|---------|
| 9000 | S3 API |
| 9001 | Console + Admin API |
| 9090 | Metrics (loopback only) |

Default credentials are set via `STRIX_ROOT_USER` and `STRIX_ROOT_PASSWORD` environment variables.
