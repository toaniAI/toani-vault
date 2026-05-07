# Health Endpoint Proxy Checklist

Use this checklist when `/health`, `/ready`, or `/health/detail` are routed through an external ingress, load balancer, or gateway outside this repository.

## Required routing rules

- `GET /health` must proxy to the backend `/health` handler unchanged.
- `GET /ready` must proxy to the backend `/ready` handler unchanged.
- `GET /health/detail` must proxy to the backend `/health/detail` handler unchanged.
- None of the three paths may be rewritten to a frontend SPA shell.
- None of the three paths may be replaced with a proxy-local `200 healthy` plain-text response.

## Expected backend contract

- `/health`
  - Status: `200 OK`
  - Content-Type: `application/json`
  - Body shape:

```json
{
  "status": "alive",
  "ready": true,
  "version": "1.0.0",
  "timestamp": 1741702800
}
```

- `/ready` and `/health/detail`
  - Status: `200 OK` when ready, `503 Service Unavailable` when not ready
  - Content-Type: `application/json`
  - Body shape:

```json
{
  "status": "ready",
  "live": true,
  "ready": true,
  "version": "1.0.0",
  "timestamp": 1741702800,
  "components": {
    "vault": "healthy",
    "enclave": "healthy",
    "audit_log": "healthy",
    "attestation": "ready"
  }
}
```

## Verification commands

Run these checks against the externally exposed endpoint after each proxy change:

```bash
curl -i https://your-api.example.com/health
curl -i https://your-api.example.com/ready
curl -i https://your-api.example.com/health/detail
```

Reject the proxy change if any response is plain text, HTML, or a frontend document shell.
