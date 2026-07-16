# Production HTTP deployment

dukememory's built-in listener deliberately serves plain HTTP. Bind it only to
loopback and terminate TLS at a maintained reverse proxy. The examples in
`deploy/` use port `8765`, preserve the public `Host` header for same-origin
checks, and keep bearer authentication enabled end to end.

## 1. Install the service account and directories

```bash
sudo useradd --system --home /var/lib/dukememory --create-home dukememory
sudo install -d -o dukememory -g dukememory -m 700 \
  /var/lib/dukememory/.agent /var/lib/dukememory/models
sudo install -d -o root -g dukememory -m 750 /etc/dukememory
sudo install -o dukememory -g dukememory -m 600 /dev/null \
  /etc/dukememory/http-token
openssl rand -hex 32 | sudo tee /etc/dukememory/http-token >/dev/null
sudo install -o dukememory -g dukememory -m 600 /dev/null \
  /etc/dukememory/http-read-token
openssl rand -hex 32 | sudo tee /etc/dukememory/http-read-token >/dev/null
```

Download the release archive and its entry in `SHA256SUMS`, verify SHA-256, then
install the binary as `/usr/local/bin/dukememory`. The release workflow already
runs `scripts/release-smoke.sh` against an installed copy of each native
artifact. Do not place the
bearer token in the unit command line, shell history, proxy configuration, or
Git. Browser users enter it in the UI; API clients send it as a bearer token.

## 2. Configure the public origin

Create `/etc/dukememory/dukememory.env`:

```ini
DUKEMEMORY_HTTP_ALLOWED_ORIGINS=https://memory.example.com
DUKEMEMORY_AGENT_QUOTA_BYTES=536870912
DUKEMEMORY_BACKUP_QUOTA_BYTES=268435456
DUKEMEMORY_ROLLBACK_QUOTA_BYTES=134217728
DUKEMEMORY_INSTALL_BACKUP_QUOTA_BYTES=536870912
DUKEMEMORY_DEPLOYMENT_MODE=reverse-proxy
DUKEMEMORY_HTTP_HOST=127.0.0.1
DUKEMEMORY_PUBLIC_ORIGIN=https://memory.example.com
DUKEMEMORY_HTTP_READ_TOKEN_FILE=/etc/dukememory/http-read-token
DUKEMEMORY_HTTP_RATE_LIMIT_PER_MINUTE=600
DUKEMEMORY_HTTP_RATE_LIMIT_MAX_CLIENTS=2048
DUKEMEMORY_HTTP_MAX_CONCURRENT_REQUESTS=4
DUKEMEMORY_HTTP_MAX_CONCURRENT_PER_CLIENT=4
DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS=32
DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS_PER_OWNER=4
DUKEMEMORY_SQLITE_DURABILITY=strict
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
OTEL_EXPORTER_OTLP_PROTOCOL=http/json
OTEL_EXPORTER_OTLP_TIMEOUT=10000
```

The service also accepts `DUKEMEMORY_SYNC_PASSPHRASE_FILE` here when encrypted
sync is automated. Keep that file outside the repository with mode `600`.
The read-only bearer is optional and must differ from the full token. It can
read dashboards and MCP read tools but cannot invoke cataloged writes. The
listener rejects invalid/zero limits at startup. The rate-limit identity table,
simultaneous HTTP work, and asynchronous MCP task workers all have independent
hard bounds; task admission is also bounded per authenticated owner.

### OAuth/OIDC authentication gateway

Native static bearer tokens remain the simplest deployment. For OAuth 2.1 or
OIDC, put DukeMemory behind an authentication gateway that validates token
signature, issuer, expiration, audience/resource, and scopes before forwarding.
DukeMemory deliberately does not parse or pass through the external access
token. The gateway must remove every client-supplied `Authorization`,
`X-DukeMemory-Principal`, `X-DukeMemory-Scopes`, and `X-Forwarded-For` header,
then inject a stable principal and the validated `memory:read` and/or
`memory:write` scopes. Enable this mode only with an explicit gateway CIDR:

```ini
DUKEMEMORY_HTTP_TRUSTED_PROXY_AUTH=true
DUKEMEMORY_HTTP_TRUSTED_PROXY_CIDRS=127.0.0.1/32,::1/128
DUKEMEMORY_PUBLIC_ORIGIN=https://memory.example.com
DUKEMEMORY_OAUTH_AUTHORIZATION_SERVERS=https://id.example.com/tenant
```

Requests carrying identity headers from any address outside those CIDRs are
unauthorized. The same trusted chain determines the rate-limit client from
`X-Forwarded-For`; malformed chains fail closed. DukeMemory publishes protected
resource metadata at `/.well-known/oauth-protected-resource` and includes its
URL in `WWW-Authenticate` challenges. This follows RFC 9728 discovery expected
by MCP HTTP clients; the gateway remains responsible for full token validation
and RFC 8707 audience/resource binding.

Local SQLite files are not application-level encrypted. DukeMemory enforces
mode `600` on the database and WAL/SHM sidecars, creates new database directories
with mode `700`, and enables SQLite `secure_delete=FAST`; production hosts should
still use encrypted storage (for example LUKS or FileVault) when memory content
is sensitive. Remote/VDS bundles should use the built-in authenticated age
encryption.
The bundled SQLite version is release-gated at 3.51.3 or newer. Strict
durability enables `synchronous=FULL`, full-fsync, and checkpoint-fsync; run
`dukememory audit --verify --json` as an operational integrity check.

Validate the effective profile before starting or releasing the service:

```bash
dukememory deployment-profile \
  --mode reverse-proxy \
  --host 127.0.0.1 \
  --auth-token-file /etc/dukememory/http-token \
  --public-origin https://memory.example.com \
  --json
```

The report fails closed on public binds, missing bearer or trusted-proxy
authentication, incomplete OAuth discovery/trust configuration, a
non-HTTPS origin, origin-policy mismatches, unsafe token files, and an encrypted
sync target without a valid passphrase. Invalid HTTP/MCP admission limits and
per-owner limits above their global bounds are also blockers. It states the current limits
explicitly: SQLite at-rest encryption is host-managed. Production
observability includes JSON access logs, request IDs, `/metrics`, and a bounded
batched OTLP/HTTP JSON logs exporter. A signal-specific
`OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` is used as-is; the generic endpoint gets the
standard `/v1/logs` suffix. Global and logs-specific OTLP headers/timeouts are
supported. Outbound collector connections use the same pinned-DNS egress
policy as model providers, and private non-loopback collectors require their
exact hostname in `DUKEMEMORY_EGRESS_ALLOW_HOSTS`.

## 3. Start the systemd service

```bash
sudo install -o root -g root -m 644 \
  deploy/systemd/dukememory.service /etc/systemd/system/dukememory.service
sudo systemctl daemon-reload
sudo systemctl enable --now dukememory
curl --fail http://127.0.0.1:8765/health
```

The unit writes only under `/var/lib/dukememory`, restarts on failures, and gives
the server up to 30 seconds to drain workers after SIGTERM.

## 4. Terminate TLS

Choose one proxy:

- Caddy: replace `memory.example.com` in `deploy/caddy/Caddyfile`, install it as
  `/etc/caddy/Caddyfile`, validate with `caddy validate --config
  /etc/caddy/Caddyfile`, then reload Caddy. Caddy obtains and renews certificates.
- nginx: replace the hostname and certificate paths in
  `deploy/nginx/dukememory.conf`, install it under `/etc/nginx/conf.d/`, run
  `nginx -t`, then reload nginx. Provision certificates separately, for example
  with Certbot.

Keep port `8765` closed at the firewall; only ports `80` and `443` should be
public. Both templates forward the original `Host`, scheme, and client address.
dukememory does not trust `Host` by itself: the public hostname must also come
from `DUKEMEMORY_HTTP_ALLOWED_ORIGINS` or `DUKEMEMORY_PUBLIC_ORIGIN`, and browser
origins are matched exactly. Forwarded addresses and identity headers are
ignored unless the direct peer belongs to `DUKEMEMORY_HTTP_TRUSTED_PROXY_CIDRS`.

## 5. Verify and operate

```bash
curl --fail https://memory.example.com/health
curl --fail \
  -H "Authorization: Bearer $(sudo cat /etc/dukememory/http-token)" \
  https://memory.example.com/metrics
jq -nc '{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"deployment-check",version:"1"}}}' | \
  curl --fail --include \
    -H "Authorization: Bearer $(sudo cat /etc/dukememory/http-token)" \
    -H 'Content-Type: application/json' \
    --data-binary @- \
    https://memory.example.com/mcp
journalctl -u dukememory -f
dukememory --db /var/lib/dukememory/.agent/memory.db schema verify
dukememory --db /var/lib/dukememory/.agent/memory.db integrity --json
dukememory --db /var/lib/dukememory/.agent/memory.db vec-validate --backend sqlite-vec
dukememory --db /var/lib/dukememory/.agent/memory.db vec-index --json
```

The first request verifies TLS and the unauthenticated liveness endpoint. The
second verifies bearer authentication. The third verifies the MCP Streamable
HTTP endpoint and should return an `MCP-Session-Id`; keep that identifier secret
and send it only to the same origin. Access logs are one-line JSON on stderr
and therefore appear in the systemd journal; every response/access event shares
an `X-Request-Id`. When OTLP is configured, the same event is sent
asynchronously in batches of at most 64 records; a bounded queue prevents a
slow collector from applying unbounded memory pressure. `ops-status --json` reports byte quotas, over-quota areas,
retention readiness, and `ok`/`warn`/`critical` pressure. Backup policy enforces
both `--keep` and `DUKEMEMORY_BACKUP_QUOTA_BYTES`. Rotate the HTTP token by replacing
the file atomically and restarting the service.

For encrypted VDS sync, monitor `sync status --json`. A healthy status has
`verified: true`, `corrupt: false`, `stale_remote: false`, and no active lock.
The writer preserves a previous verified generation. If the current bundle is
damaged, inspect status and run `sync recover TARGET --json`; do not use
`sync push --force` until the generation mismatch or corruption is understood.
