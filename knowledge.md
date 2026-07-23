# Project knowledge

This is a **research and planning workspace** for migrating from `sops-nix` to `SecretSpec` (https://secretspec.dev). The directory is now a real git repo with validated Phase 1 deliverables + design docs. Reason for the repo: to capture the migration plan, dogfood on real secrets, and upstream a SOPS provider to cachix/secretspec.

## Quickstart
- Setup: `~/.local/bin/secretspec` (v0.16.0) — direct release tarball download from `github.com/cachix/secretspec/releases/download/v0.16.0/secretspec-x86_64-unknown-linux-gnu.tar.xz` (the nixpkgs pin lags at v0.12.0; do not use that for validation in this repo).
- Validate Phase 1: `secretspec check --profile development` (parses cleanly; full resolution requires a populated `.env.secrets`)
- Dev: edit `CONTEXT.md` to capture new research; update this file when the migration plan changes materially
- Test: not applicable

## Status snapshot (2026-07-26)
- SecretSpec installed: **v0.16.0** (direct release from cachix/secretspec v0.16.0 tag, 2026-07-18); nixpkgs `pkgs.secretspec` pin lags at v0.12.0
- cachix/secretspec PR #58: OPEN, DRAFT, author `euphemism` unresponsive since Jul 1 — Domen's Jul 17 rework request for provider credentials still unaddressed
- **NEW** cachix/secretspec PR #174 (`feat: add age provider`): OPEN 2026-07-19 — competes/complements the SOPS provider's age backend (decision deferred to provider-build time; see `CONTEXT.md` Audit 2026-07-26)
- **NEW** cachix/secretspec PR #98 (`docs: draft Secret Provider Protocol v1`): OPEN 2026-05-28 — confirmed via gh API; `SopsFileProvider` scaffold (`provider-rust/src/secretspec.rs`) awaits alignment
- cachix/secretspec issue #65 (NixOS module): open; community workaround (`systemd-creds` + SSH pipe) documented in `CONTEXT.md`
- cachix/secretspec issue #41 (systemd-creds provider): open
- cachix/devenv issue #2363 (per-profile secretspec config): open; `SECRETSPEC_PROFILE` env var is the official workaround (`https://devenv.sh/integrations/secretspec/`)
- `astral-key` endpoint (this repo's `astral-key-endpoint-spec.md`): upstream issue #16 still open, our reference comment posted 2026-07-23 (no team reply as of 2026-07-26)
- ✅ **resolved** Phase 1 runtime limit (Audit 2026-07-26 tracked item) via per-profile `[profiles.*.defaults] providers = ["dotenv", "env"]` blocks + `.env.secrets.example` + `scripts/bootstrap-dev.sh`
- ✅ **closed this turn** `.github/workflows/ci.yml` production-readiness gate (cargo fmt → cargo clippy --all-targets -- -D warnings → cargo test → boot bootstrap + secretspec check default+dev → release-build binary smoke)
- ✅ **closed this turn** Phase 3 / Phase 2.5 Provider scaffold in `provider-rust/src/secretspec.rs` (`SopsFileProvider` — closes Phase 3 scaffold surface; awaits cachix/secretspec#98 alignment for closure)
- 36 tests passing (lib 16 + integration 8 + cli_smoke 6 + doctest 6) per `cargo test`; CI gate machine-enforces the same pass-list

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
- **Scope (revised 2026-07-26):** This repo holds **migration deliverables** for sops-nix → SecretSpec and *implementation* of the upstreamable SOPS provider crate. Two new top-level subdirs are permitted:
  - `provider-rust/` — the SOPS provider Rust crate (Cargo workspace). Contents: `Cargo.toml`, `src/`, `tests/`, `README.md`. The crate name is `secretspec-provider-sops`. Phase 1 ships a CLI shim; later phases wire it as a SecretSpec provider via the framework's generic JSON interface.
  - `scripts/` — small bash/Python orchestrators (interpreted, not compiled) that run `secretspec check`, report migration status, etc.
  - **Still prohibited:**
    - NixOS modules / flake.nix / build configs.
    - Compiled source code OUTSIDE `provider-rust/` (e.g., standalone Go modules, extra Rust crates that aren't the SOPS provider).
- Update `CONTEXT.md` rather than starting parallel documents. Append links/notes under existing sections when possible.
- Use http (not https) for `secretspec.dev` — SSL is broken, use http + safe browsing.
- Quote versions explicitly — SecretSpec moves fast. This repo ships against the **v0.16.0** direct-release binary; nixpkgs pin (`pkgs.secretspec`) remains behind at v0.12.0. Always run `secretspec --version` before claiming a behavior is v0.16-specific and cross-check against the project's installed binary, not the nixpkgs pin.
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
