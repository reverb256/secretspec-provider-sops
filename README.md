# Homelab SecretSpec Migration

Self-hosted NixOS homelab's migration from `sops-nix` to `SecretSpec`, with
the goal of building + upstreaming a SOPS provider to cachix/secretspec.

## Files in this repo

| File | Purpose |
|------|---------|
| `CONTEXT.md` | Single source of truth: spec format, nixpkgs status, NixOS integration, PR #58 status, migration plan, blockers, key links |
| `knowledge.md` | High-signal context for fresh Freebuff sessions — what's true, what's NOT true, what to avoid |
| **`secretspec.toml`** | **Phase 1 deliverable**: every existing secret declared via SecretSpec, with profiles for `default`, `production`, `development` (validated against `secretspec` v0.16.0, the direct release download at `~/.local/bin/secretspec`; nixpkgs `pkgs.secretspec` lags at v0.12.0) |
| **`sops-provider-design.md`** | **Tier 3 design**: architecture for the upstream SOPS provider, encoding Domen's seven accept-criteria; `(uri, credentials)` shape now confirmed for v0.15+ |
| **`migration-matrix.md`** | **Phase 2/3 prep**: per-secret matrix mapping each secret through Phase 1 env/dotenv → Phase 2 `sops://` → Phase 3 final provider; 49 declared, 0 pending declaration |
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

- **Phase 1 (this work):** declare all 49 secrets via `secretspec.toml`.
  Manifest parses cleanly against `secretspec` v0.16.0 (installed at
  `~/.local/bin/secretspec`); full resolution requires a populated
  `.env.secrets` (gitignored). See `CONTEXT.md` Audit 2026-07-26 §
  "Phase 1 runtime limit" for the runtime behavior on a clean checkout.
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

`2026-07-26` end-to-end closure: all four migration phases' in-scope
work is now resolved at the actionable level. `.github/workflows/
ci.yml` machine-enforces the production-readiness gate
(`cargo fmt → cargo clippy --all-targets -- -D warnings → cargo test
→ bootstrap-dev.sh → secretspec check --profile default →
secretspec check --profile development → release-build binary smoke`).
`provider-rust/src/secretspec.rs` ships `SopsFileProvider` as Phase 3
scaffold awaiting cachix/secretspec#98 alignment. 36 tests pass
(16 lib + 8 integration + 6 cli_smoke + 6 doctest). Migration phases:
Phase 1 ✅ closed, Phase 2 🟡 in progress (CLI shim + 4-format quartet
shipped; awaits upstream PR #58 OR Provider scaffold alignment), Phase
3 🟡 decisioned (matrix complete, decisions ready), Phase 4 🔒
blocked upstream (issue #65 + #41).
