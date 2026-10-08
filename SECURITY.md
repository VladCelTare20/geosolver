# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/VladCelTare20/geosolver/security/advisories/new)
rather than in a public issue. Include the version (`agstudio --version`), the
deployment route and the steps to reproduce.

Two kinds of report are equally in scope:

- **Server security** — anything that gets past the shared password, reads
  another user's history, escapes the solve worker's limits, or makes the
  `claude` CLI do more than translate the uploaded problem.
- **Soundness** — any input that GeoSolver reports as *proved* while the
  statement is false. For a prover, a false "proved" is a vulnerability.

## Supported versions

Only the latest commit of the default branch is supported; fixes are not
backported.

## Hardening already in place

The deployment guide ([deploy/DEPLOY.md](deploy/DEPLOY.md)) describes it: the
server refuses to start on a public interface without a password, per-IP rate
limits, a body limit, CSP and security headers, solves in killable worker
processes with memory and time limits, a sandboxed `claude` CLI with no tools
beyond reading the uploaded image, and hardened systemd/Docker profiles.
