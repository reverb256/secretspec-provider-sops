# Project knowledge

This is a **research and planning workspace** for migrating from `sops-nix` to `SecretSpec` (https://secretspec.dev). The directory is now a real git repo with validated Phase 1 deliverables + design docs. Reason for the repo: to capture the migration plan, dogfood on real secrets, and upstream a SOPS provider to cachix/secretspec.

## Quickstart
- Setup: `nix profile install 'nixpkgs#secretspec'` (validated; v0.12.0 in nixpkgs)
- Validate Phase 1: `secretspec check --profile development` (parses cleanly; full resolution requires a populated `.env.secrets`)
- Dev: edit `CONTEXT.md` to capture new research; update this file when the migration plan changes materially
- Test: not applicable

## Status snapshot (2026-07-26)
- SecretSpec upstream stable: **v0.16.0** (2026-07-17); nixpkgs pin: v0.12.0
- cachix/secretspec PR #58: OPEN, DRAFT, author `euphemism` unresponsive since Jul 1 — Domen's Jul 17 rework request for provider credentials still unaddressed
- cachix/secretspec issue #65 (NixOS module): open; community workaround (`systemd-creds` + SSH pipe) documented in `CONTEXT.md`
- cachix/secretspec issue #41 (systemd-creds provider): open
- cachix/devenv issue #2363 (per-profile secretspec config): open; `SECRETSPEC_PROFILE` env var is the official workaround (`https://devenv.sh/integrations/secretspec/`)
- DOFLD `astral-key` endpoint (this repo's `astral-key-endpoint-spec.md`): upstream issue #16 still open

## Architecture

`CONTEXT.md` is the single source of truth. It covers:
- SecretSpec spec format (`secretspec.toml`, profiles, providers, secret fields, inheritance)
- All 15 provider backends and their capabilities (read/write/encrypted)
- nixpkgs integration status (`pkgs.secretspec` v0.12.0, no flake upstream)
- NixOS integration status — **no module exists** (issue #65); community pattern is `systemd-creds` + SSH pipe
- SOPS provider PR #58 — open, draft, conflicts, author unresponsive since Jul 1
- Migration phases from sops-nix, plus an inventory of ~50 current secrets
- Key links and community sentiment

## Migration plan (phases)

1. **Declare** every existing secret in a root `secretspec.toml` (no storage change). Start with `dotenv://` or `env://`.
2. **Wait for SOPS provider** (PR #58) to merge before pointing at existing `.age` files. Until then, keep sops-nix as fallback.
3. **Migrate per-secret** to better providers: `keyring://` (dev), `onepassword://` / `bws://` / `vault://` (team), `env://` (CI).

## Conventions
- **Scope (revised 2026-07-23):** This repo holds **migration deliverables** for sops-nix → SecretSpec, not pure docs. Allowed: markdown, `secretspec.toml` (the Phase 1 declaration), design docs (*e.g.,* `sops-provider-design.md`), README index, `.gitignore`. **Prohibited:** source code that gets compiled (Rust/Cargo, Go modules, etc.), NixOS modules, flake.nix, build/Makefile/CI configs. The SOPS provider Rust crate lives in a *separate* repo (hosting TBD).
- Update `CONTEXT.md` rather than starting parallel documents. Append links/notes under existing sections when possible.
- Use http (not https) for `secretspec.dev` — SSL is broken, use http + safe browsing.
- Quote versions explicitly — SecretSpec moves fast (v0.12.0 in nixpkgs as of mid-2026).

## Things to avoid
- **Don't recommend a full sops-nix replacement yet.** Community treats SecretSpec as additive, not a swap.
- **Don't claim a NixOS module exists** — issue #65 is open; the canonical workaround is `systemd-creds encrypt` + `LoadCredentialEncrypted=`.
- **Don't promise the SOPS provider is available** — PR #58 is draft/conflicting/unmaintained-for-now.
- **Don't refer to "vimjoyer covered NixOS integration"** — his video covers basics only (init, config, profiles, run).
- **Don't assume devenv per-profile secretspec works** — issue #2363 is still open; the secretspec profile in `devenv.yaml` remains global. Set `SECRETSPEC_PROFILE` env var explicitly per https://devenv.sh/integrations/secretspec/.
- **Don't mix known blockers into the phase plan as if they were solved** — see the blockers table in `CONTEXT.md` for current workarounds.

## Related projects worth knowing
- `sops-nix` (Mic92) — current NixOS standard; used via `sops.secrets.<name>.path`
- `varlock` — newer, complementary (not a competitor); adds log redaction
- `astral-key` (reverb256/astral-key) — Web3/FIDO2/Passkey microservice slated to act as SecretSpec's `vault://` backend by emulating Vault's AppRole auth flow. Tracking issue: https://github.com/reverb256/astral-key/issues/16
