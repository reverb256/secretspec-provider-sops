# Project knowledge

This is a **research and planning workspace** for migrating from `sops-nix` to `SecretSpec` (https://secretspec.dev). It is **not a software project** — there is no code to build, run, or test. The reason the repo exists at all is so it can be handed to a top-tier model to continue the migration planning.

## Quickstart
- Setup: none (docs-only directory)
- Dev: edit `CONTEXT.md` to capture new research; update this file when the migration plan changes materially
- Test: not applicable

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
