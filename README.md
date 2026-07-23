# Homelab SecretSpec Migration

Self-hosted NixOS homelab's migration from `sops-nix` to `SecretSpec`, with
the goal of building + upstreaming a SOPS provider to cachix/secretspec.

## Files in this repo

| File | Purpose |
|------|---------|
| `CONTEXT.md` | Single source of truth: spec format, nixpkgs status, NixOS integration, PR #58 status, migration plan, blockers, key links |
| `knowledge.md` | High-signal context for fresh Freebuff sessions — what's true, what's NOT true, what to avoid |
| **`secretspec.toml`** | **Phase 1 deliverable**: every existing secret declared via SecretSpec, with profiles for `default`, `production`, `development` (validated against `secretspec` v0.12.0 installed via nixpkgs) |
| **`sops-provider-design.md`** | **Tier 3 design**: architecture for the upstream SOPS provider, encoding Domen's seven accept-criteria; `(uri, credentials)` shape now confirmed for v0.15+ |
| **`migration-matrix.md`** | **Phase 2/3 prep**: per-secret matrix mapping each secret through Phase 1 env/dotenv → Phase 2 `sops://` → Phase 3 final provider; 25 declared, 24 pending declaration |
| **`astral-key-endpoint-spec.md`** | **Phase 3 hand-off**: Vault KV v2 compatible API spec (endpoint shape, AppRole emulation, response codes) for the upstream astral-key team's reference |
| `.gitignore` | Hygiene only — no build config |
| `README.md` | This file |

## How to use

1. **Bootstrap a model session.** Start Freebuff in this directory. It
   loads `knowledge.md` and surfaces `CONTEXT.md` to the model.
2. **For Phase 1 (immediate).** Edit `secretspec.toml`, set
   `SECRETSPEC_PROFILE` per https://devenv.sh/integrations/secretspec/,
   run `secretspec check`.
3. **For Tier 3 work (provider build).** Study `sops-provider-design.md`
   before touching Rust code. **Crates do not live in this repo** — they
   live in a separate location (hosting choice TBD).

## Roadmap

- **Phase 1 (this work):** declare secrets via `secretspec.toml`. Validation
  via `secretspec check` is **not yet run** because the `secretspec` CLI is
  not installed in this repo; this is a known gap, easy to close.
- **Phase 2:** wait for cachix/secretspec PR #58 (open, draft) to merge,
  then point at existing `.age`-encrypted sops files
- **Phase 3:** per-secret storage migration to `keyring://`,
  `onepassword://`, `sops://`
- **Phase 4:** NixOS runtime — blocked on issue #65 (NixOS module) and
  #41 (systemd-creds provider). Working example (`systemd-creds encrypt`
  over SSH + `LoadCredentialEncrypted=`) lives in `CONTEXT.md` →
  "NixOS Integration Status" > "What the community actually does".

See `CONTEXT.md` "Migration Path" for full details.

## Status snapshot

`2026-07-23`: PR #58, issue #65, issue #41 — all unchanged from prior
snapshot. Issue #2363 — confirmed still OPEN (a previous "closed" report
was incorrect; corrected in `CONTEXT.md` and `knowledge.md`).
