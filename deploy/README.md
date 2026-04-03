# Strix Deployment

This directory contains deployment configurations for Strix.

## Quick Start

```bash
cd deploy

# Copy and configure environment
cp .env.example .env
# Edit .env with your settings (at minimum, set STRIX_ROOT_PASSWORD)

# Start Strix
docker compose up -d

# View logs
docker compose logs -f strix

# Stop
docker compose down
```

## Files

| File | Description |
|------|-------------|
| `docker-compose.yml` | Main compose file for development/testing |
| `docker-compose.prod.yml` | Production overrides (resource limits, logging) |
| `.env.example` | Template with all configuration options |
| `prometheus.yml` | Prometheus scrape config for metrics |

## Access Points

| Service | URL | Description |
|---------|-----|-------------|
| S3 API | http://localhost:9000 | S3-compatible object storage |
| Console | http://localhost:9001 | Web UI and Admin API |
| Metrics | http://localhost:9090/metrics | Prometheus metrics |

## With Monitoring

Start with Prometheus and Grafana:

```bash
docker compose --profile monitoring up -d
```

Access Grafana at http://localhost:3000 (admin/admin by default).

## Production Deployment

```bash
# Using production overrides
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d

# Or with pre-built image
STRIX_IMAGE=ghcr.io/your-org/strix:v1.0.0 \
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

## Environment Variables

See `.env.example` for all available configuration options with documentation.

Key variables:
- `STRIX_ROOT_USER` - Admin username (default: admin)
- `STRIX_ROOT_PASSWORD` - Admin password (required)
- `STRIX_JWT_SECRET` - Session signing key (recommended for production)
- `STRIX_LOG_LEVEL` - Logging verbosity (trace/debug/info/warn/error)
