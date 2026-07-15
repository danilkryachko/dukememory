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
```

The service also accepts `DUKEMEMORY_SYNC_PASSPHRASE_FILE` here when encrypted
sync is automated. Keep that file outside the repository with mode `600`.

Local SQLite files are not application-level encrypted. DukeMemory enforces
mode `600` on the database and WAL/SHM sidecars, creates new database directories
with mode `700`, and enables SQLite `secure_delete=FAST`; production hosts should
still use encrypted storage (for example LUKS or FileVault) when memory content
is sensitive. Remote/VDS bundles should use the built-in authenticated age
encryption.

Validate the effective profile before starting or releasing the service:

```bash
dukememory deployment-profile \
  --mode reverse-proxy \
  --host 127.0.0.1 \
  --auth-token-file /etc/dukememory/http-token \
  --public-origin https://memory.example.com \
  --json
```

The report fails closed on public binds, missing bearer authentication, a
non-HTTPS origin, origin-policy mismatches, unsafe token files, and an encrypted
sync target without a valid passphrase. It also states the current limits
explicitly: SQLite at-rest encryption is host-managed and native OTLP export is
not implemented; production observability is JSON access logs, request IDs,
`/metrics`, and journal/collector ingestion.

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
dukememory trusts `Host` for origin enforcement but does not use forwarded
addresses for authorization.

## 5. Verify and operate

```bash
curl --fail https://memory.example.com/health
curl --fail \
  -H "Authorization: Bearer $(sudo cat /etc/dukememory/http-token)" \
  https://memory.example.com/metrics
journalctl -u dukememory -f
dukememory --db /var/lib/dukememory/.agent/memory.db schema verify
dukememory --db /var/lib/dukememory/.agent/memory.db integrity --json
dukememory --db /var/lib/dukememory/.agent/memory.db vec-validate --backend sqlite-vec
dukememory --db /var/lib/dukememory/.agent/memory.db vec-index --json
```

The first request verifies TLS and the unauthenticated liveness endpoint. The
second verifies bearer authentication. Access logs are one-line JSON on stderr
and therefore appear in the systemd journal; every response/access event shares
an `X-Request-Id`. `ops-status --json` reports byte quotas, over-quota areas,
retention readiness, and `ok`/`warn`/`critical` pressure. Backup policy enforces
both `--keep` and `DUKEMEMORY_BACKUP_QUOTA_BYTES`. Rotate the HTTP token by replacing
the file atomically and restarting the service.

For encrypted VDS sync, monitor `sync status --json`. A healthy status has
`verified: true`, `corrupt: false`, `stale_remote: false`, and no active lock.
The writer preserves a previous verified generation. If the current bundle is
damaged, inspect status and run `sync recover TARGET --json`; do not use
`sync push --force` until the generation mismatch or corruption is understood.
