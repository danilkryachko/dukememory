# Security policy

## Supported versions

Security fixes are provided for the latest published DukeMemory release and
the current `main` branch. Older releases may be used to reproduce an issue,
but are not maintained independently.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's
[private vulnerability reporting](https://github.com/danilkryachko/dukememory/security/advisories/new)
so reports, proof-of-concept material, and remediation work stay private until
a coordinated disclosure is ready.

Please include:

- the affected version or commit;
- the deployment mode (local CLI, HTTP, MCP, sync, or model provider);
- reproduction steps and the expected security boundary;
- impact, prerequisites, and any suggested mitigation;
- whether the report or proof of concept may be credited publicly.

The maintainer will acknowledge a complete report within three business days,
triage severity and scope, and coordinate a fix and disclosure timeline. Avoid
accessing data that is not yours, disrupting services, or publishing details
before the fix is available.

## Security boundaries

DukeMemory is local-first. Filesystem permissions and encrypted host storage
remain part of the trust boundary. The SQLite audit chain detects accidental
or unauthorized modification after a trusted checkpoint, but it is not a
substitute for an externally witnessed signature. Remote HTTP/MCP deployments
must terminate TLS at a trusted proxy and configure authentication, origin,
host, egress, and resource-limit policies as documented in
`docs/production-deployment.md`.
