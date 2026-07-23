# SecretSpec — Complete Context for Migration & Integration

> **Snapshot date:** 2026-07-23. Drift-checked PR #58, issue #65, issue #41 (all unchanged); **issue #2363 re-verified as still OPEN** (a prior "closed" claim was incorrect — corrected below and in `knowledge.md`). The `SECRETSPEC_PROFILE` env var workaround is the officially documented approach at https://devenv.sh/integrations/secretspec/.

## Overview

SecretSpec (cachix/secretspec) is a declarative secret management tool that separates **what** secrets an app needs (`secretspec.toml`) from **where** they're stored (15+ provider backends; the upstream list grew from 15 to 17 across 0.15–0.16). It lives in nixpkgs as `secretspec-0.12.0` (latest nixpkgs pin; upstream stable is `v0.16.0`, released 2026-07-17), maintained by Domen Kozar (Cachix founder) and Sander.

**Philosophy:** Commit the declaration, never the values. Profiles vary what's required per environment. Providers resolve values from keyring, 1Password, Vault, env, dotenv, etc. Eight SDKs (Rust, Python, Go, Ruby, Node, Haskell, PHP, C#) use the same resolver.

**Homepage:** https://secretspec.dev (SSL broken, use http + click through safe browsing)
**Repo:** https://github.com/cachix/secretspec
**Nixpkgs:** `pkgs.secretspec` (v0.12.0 in nixpkgs; upstream stable `v0.16.0`), `pkgs/by-name/se/secretspec/package.nix`
**License:** Apache 2.0

## Architecture

### Three concerns (declaration → profile → provider)

```
secretspec.toml  →  Profile selected  →  Provider resolves
(what you need)     (which reqs)         (where values come from)
```

### secretspec.toml format

```toml
[project]
name = "my-app"
revision = "1.0"
extends = ["../shared/secretspec.toml"]   # Optional inheritance

[providers]
prod_vault = "onepassword://Production"
keyring = "keyring://"
env = "env://"

[profiles.default]
DATABASE_URL = { description = "PostgreSQL connection", required = true }
API_KEY = { description = "Third-party API", required = true }
LOG_LEVEL = { description = "Verbosity", required = false, default = "info" }

[profiles.development]
DATABASE_URL = { default = "sqlite://./dev.db" }

[profiles.production]
DATABASE_URL = { required = true, providers = ["prod_vault", "keyring"] }
```

### Secret variable fields

| Field | Required | Description |
|-------|----------|-------------|
| `description` | Yes (default profile) | Human-readable purpose |
| `required` | No | Default true (or false when `default` is set) |
| `default` | No | Fallback value |
| `providers` | No | Ordered provider list for this secret |
| `ref` | No | External secret coordinates: `{ item, field?, vault?, section?, version? }` |
| `as_path` | No | Write to temp file, return path |
| `type` | Conditional | `password`, `hex`, `base64`, `uuid`, `command`, `rsa_private_key` |
| `generate` | No/table | Auto-generate when missing |
| `composed` (0.16+) | No | Derive from `${UPPERCASE_NAME}` refs |

### Profile inheritance

- All profiles auto-inherit from `[profiles.default]`
- `[profiles.<name>.defaults]` applies shared settings (providers, required, etc.) to all secrets in that profile
- Precedence: secret-level > profile defaults > inherited defaults > global defaults

### Provider resolution order (per secret)

1. `--provider` CLI flag (explicit override, disables fallback)
2. `SECRETSPEC_PROVIDER` env var (explicit override, disables fallback)
3. Secret's effective `providers[]` list (tried left→right until one returns)
4. User config default provider

### Profile resolution order

1. `--profile` CLI flag
2. `SECRETSPEC_PROFILE` env var
3. User config default profile
4. `default` fallback

## 15 Providers

| Provider | URI Pattern | Read | Write | Encrypted |
|----------|-------------|------|-------|-----------|
| keyring | `keyring://` | ✅ | ✅ | ✅ |
| dotenv | `dotenv://.env` | ✅ | ✅ | ❌ |
| env | `env://` | ✅ | ❌ | ❌ |
| onepassword | `onepassword://Vault` | ✅ | ✅ | ✅ |
| lastpass | `lastpass://` | ✅ | ✅ | ✅ |
| pass | `pass://` | ✅ | ✅ | ✅ |
| gopass (0.15+) | `gopass://` | ✅ | ✅ | ✅ |
| protonpass | `protonpass://` | ✅ | ✅ | ✅ |
| vault/openbao | `vault://secret/mount?auth=approle` | ✅ | ✅ | ✅ |
| bws | `bws://<uuid>` | ✅ | ✅ | ✅ |
| gcsm | `gcsm://` | ✅ | ✅ | ✅ |
| awssm | `awssm://` | ✅ | ✅ | ✅ |
| akv (0.15+) | `akv://` | ✅ | ✅ | ✅ |
| infisical (0.16+) | `infisical://` | ✅ | ✅ | ✅ |
| **sops** (PR #58, draft) | `sops://file.yaml` | ✅ (CLI) | — | N/A |

## CLI Quick Reference

```bash
secretspec init                         # Create secretspec.toml (--from dotenv)
secretspec config init                  # User config (provider, profile)
secretspec check                        # Validate all required secrets resolve
secretspec get KEY                      # Resolve and print one secret
secretspec set KEY VALUE                # Store a secret
secretspec run -- <cmd>                 # Resolve + inject as env vars + run cmd
secretspec run --profile prod -- <cmd>
secretspec run --provider dotenv -- <cmd>
```

## nixpkgs Status

**Package:** `pkgs.secretspec` (v0.12.0)
**Path:** `pkgs/by-name/se/secretspec/package.nix`
**Maintainers:** Domen Kozar, Sander (both Cachix)
**Platforms:** Linux, macOS, Windows, FreeBSD, NetBSD, etc.
**License:** Apache 2.0

No flake.nix exists upstream — users install via nixpkgs or the curl installer.

## NixOS Integration Status (ISSUE #65)

**GitHub:** https://github.com/cachix/secretspec/issues/65

**STATUS: NO NixOS MODULE EXISTS.** There is no equivalent of `sops-nix`'s `sops.secrets.<name>.path` that wires decrypted secrets to files at boot. Issue #65 tracks the feature request.

### What the community actually does (from issue #65 thread)

The dominant pattern for using SecretSpec with NixOS **today**:

```nix
# In a devenv script / deploy script:
deploy-creds.exec = let
  emplaceSecret = file: name:
    "mkdir -p /etc/credstore && systemd-creds encrypt - /etc/credstore/${file} --name ${name}";
in ''
  secretspec run -- bash -lc '
    cat "$CLOUDFLARE_CERT_PEM" | ssh ${host} "${emplaceSecret "cert.pem" "cert.pem"}"
    printf "ADMIN_TOKEN=***" "$TOKEN" | ssh ${host} "${emplaceSecret "admin.env" "admin-token"}"
  '
'';

# On the target NixOS host:
systemd.services.cloudflared-tunnel = {
  serviceConfig = {
    LoadCredentialEncrypted = [
      "cert.pem:/etc/credstore/cloudflared-cert.pem"
    ];
  };
};
```

**Key insight:** SecretSpec is used LOCALLY to fetch secrets → piped over SSH → encrypted at rest with `systemd-creds` → consumed by systemd services via `LoadCredentialEncrypted=`. This avoids the need for a NixOS module entirely.

### Feature request: systemd-creds provider (#41)

https://github.com/cachix/secretspec/issues/41

If SecretSpec gains a `systemd-creds` provider, the loop completes: declare → resolve → store via systemd-creds → serve to systemd services. This is the community's preferred path over a sops-nix-style NixOS module.

### Current community sentiment (from discourse & GitHub)

- "If you use sops or agenix this will be a **supplementary addition** rather than a swap" — it's additive, not a replacement
- No vimjoyer video on NixOS integration (he covered only the basics)
- The devenv integration works but has limitations (global profile only, issue #2363 still open); use `SECRETSPEC_PROFILE` env var per official devenv docs
- Users consistently choose `systemd-creds` + SSH pipe over a NixOS module pattern

## SOPS Provider (PR #58)

**GitHub:** https://github.com/cachix/secretspec/pull/58
**Closes:** Issue #5 ("Support Sops")
**State:** OPEN, DRAFT, NOT MERGEABLE (conflicts)

### Timeline
| Date | Event |
|------|-------|
| Feb 27 | PR opened (FFI-based Rust→Go SOPS lib) |
| May 28 | Domen: "Could we start by using sops CLI?" |
| May 29 | Author agrees, pivots to CLI-based approach |
| Jun 4 | Author mentions Domen's podcast comments on FFI approach |
| Jun 4 | Domen: "FFI is right long-term, but I want to maintain it separately. Maybe a Cachix org repo?" |
| Jun 4 | Author: "Parked FFI impl, reworking to use sops CLI" |
| Jun 19 | Author: "Ready for review" |
| Jun 20 | Domen review: credential leak in audit log (`uri()` emits age_key, aws keys, vault token in query string) |
| Jun 20 | Author fixes: adds `sensitive` field to `FieldSpec`, adds 2 tests |
| Jul 1 | CI fixes (Windows action ref) |
| Jul 2 | Domen deep review: 6 blocking issues (dotenv format broken both read/write, format inference panic on common filenames, binary mode semantics) |
| **Jul 17** | **Domen: "Needs to be reworked for provider credentials"** |
| Jul 20 | Label `provider` added, still draft, no author response yet |

### Current blockers

1. **Provider credentials pattern** — Domen wants the sops provider to use SecretSpec's credential-chaining (resolve `age_key` from keyring via credentials block), not pass raw keys in URI
2. **6 inline review issues** — format inference panic, dotenv broken, binary mode semantics
3. **Merge conflicts** — PR has conflicts with main
4. **Author responsiveness** — No response since Jul 1 (last author comment), Domen's Jul 17 comment unanswered

### Author's forked branches
- FFI approach (parked): `euphemism/secretspec/tree/sops-integration-via-ffi`
- CLI approach (current PR): `euphemism/secretspec/tree/add-sops-support`

### Maintainer's preferred approach
Domen seems to want:
1. Short-term: sops CLI-based provider (what PR currently does)
2. Long-term: sops-ffi as an externally maintained crate, then used by SecretSpec

## Migration Path: sops-nix → SecretSpec

### Inventory of current secrets

`sops-secrets-registry.nix` has ~50 secrets across categories:
- `aiServices` — NVIDIA API key, OpenAI keys, Anthropic key, etc.
- `ci` — GitHub token, GitHub runner PAT
- `cloud` — Cloudflare tunnel credentials, Tailscale auth
- `storage` — Garage S3 keys, backup encryption keys
- `kubernetes` — K3s cluster token, kubeconfigs
- `mining` — Wallet keys, pool credentials
- `monitoring` — Grafana/Prometheus API keys (if any)
- `automation` — n8n API keys, webhook tokens
- `selfHosting` — Vaultwarden admin token, mail server creds

### Phase 1: Declare (no storage change)

Create `secretspec.toml` at repo root declaring every secret. Use `dotenv` or `env` providers initially — same storage as today. This adds the declaration layer without changing anything.

### Phase 2: SOPS provider (when PR #58 merges)

Once the SOPS provider lands, point SecretSpec at existing `.age`-encrypted sops files. Each secret resolves from the same encrypted file it uses today. This is a drop-in provider change.

### Phase 3: Per-secret provider migration

Move individual secrets from `sops` provider to more appropriate providers:
- **Personal dev:** `keyring://` (macOS Keychain)
- **Team:** `onepassword://Shared` or `bws://<uuid>` or `vault://`
- **CI:** `env://` (GitHub Secrets / GitLab CI vars)
- **K8s:** Script that runs `secretspec get KEY && kubectl create secret generic`

### Phase 4: NixOS runtime (when issue #65 or systemd-creds #41 lands)

Replace `sops-nix` `sops.secrets.<name>.path` references with:
- `secretspec run -- <service>` wrappers, OR
- `systemd-creds` encrypted at deploy time + `LoadCredentialEncrypted=` in services
- Or the official NixOS module once it exists

### Blockers for full migration

| Issue | Impact | Workaround |
|-------|--------|------------|
| No NixOS module (#65) | Can't declare secrets at system level | `systemd-creds` deploy pipe |
| No K8s integration | Can't auto-provision K8s secrets | Script: `secretspec get → kubectl create secret` |
| sops provider not merged (PR #58) | Can't read existing encrypted files | Keep sops-nix as fallback, use `dotenv`/`env` for new secrets |
| Provider credentials chicken-egg | Bootstrap provider needs auth token | Bootstrap with `keyring` or hardcoded fallback |
| No systemd-creds provider (#41) | Can't natively store via systemd-creds | SSH pipe + `systemd-creds encrypt` manually |

## devenv Integration

`devenv.yaml`:
```yaml
secretspec:
  enable: true
  provider: keyring
  profile: default
```

`devenv.nix`:
```nix
{ config, ... }: {
  env.DATABASE_URL = config.secretspec.secrets.DATABASE_URL or "";
}
```

**Known limitation (issue #2363, open as of 2026-07-23):** The secretspec profile in `devenv.yaml` is **global** — it doesn't switch across devenv profiles. `devenv --profile backend shell` does not change which secretspec profile is selected. Per the official devenv docs (https://devenv.sh/integrations/secretspec/), the recommended workaround is `SECRETSPEC_PROFILE=backend` as a shell env var (it overrides what `devenv.yaml` selects).

## Vimjoyer Video Reference

**Title:** "Secure Declarative Secrets With SecretSpec | dotenv Files On Steroids"
**URL:** https://www.youtube.com/watch?v=dII4uMU-5R8
**Published:** Jul 12, 2026
**Duration:** 7:10
**Views:** 25.8k
**Notes:** Covers basics only (init, config, profiles, run). No NixOS content. Vimjoyer's own comment confirms he's aware of PR #58.

## Key Links

| Resource | URL |
|----------|-----|
| Documentation | https://secretspec.dev/ |
| GitHub repo | https://github.com/cachix/secretspec |
| NixOS integration issue | https://github.com/cachix/secretspec/issues/65 |
| SOPS provider PR | https://github.com/cachix/secretspec/pull/58 |
| systemd-creds provider issue | https://github.com/cachix/secretspec/issues/41 |
| devenv integration docs | https://devenv.sh/integrations/secretspec/ |
| Discourse announcement | https://discourse.nixos.org/t/announcing-secretspec-declarative-secrets-management/67021 |
| Vimjoyer video | https://www.youtube.com/watch?v=dII4uMU-5R8 |
| Blog: Secrets Don't Belong in Config | https://secretspec.dev/blog/secrets-dont-belong-in-config/ |
| Nixpkgs package | `pkgs.secretspec` (v0.12.0) |

## Build Intent: SOPS Provider

We are building a SOPS provider from scratch, following Domen's architecture requirements, dogfooding it on our stack, and upstreaming it to cachix/secretspec.

### Why build ourselves

- The existing PR (#58) has been stalled since Feb. Author hasn't addressed Domen's Jul 17 rework request in 5+ days.
- Domen has signalled exactly what he wants: **provider credentials pattern**, not raw keys in URI
- We have 50+ real encrypted secrets to validate against — genuine dogfooding
- Domen offered FFI-level collaboration: "maybe we create a repo on cachix"

### What we must satisfy for upstream acceptance

1. **Provider credentials pattern** (Domen's Jul 17 requirement) — decryption keys resolved through SecretSpec's credential-chaining, not raw URI params. `age_key`, `aws_secret_access_key`, `hc_vault_token`, etc. must come from a `credentials` block reading from keyring/1Password/etc.
2. **No credential leakage** — `uri()` must not emit sensitive fields. Follow the `FieldSpec.sensitive` pattern from the existing PR review.
3. **sops CLI invocation** — the expedient path Domen endorsed ("Could we for starters use sops CLI?")
4. **All SOPS backends** — age, PGP, AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault
5. **Format handling** — YAML, JSON, dotenv, binary, with correct format inference (the existing PR had a panic on common filenames)
6. **Audit compliance** — SecretSpec records every access; the provider must not bypass that
7. **Tests** — unit tests for each backend, integration tests against real sops files

### Related Projects

- **sops-nix** (Mic92) — the current standard for NixOS secrets. NO need to replace immediately. SecretSpec is additive.
- **varlock** — newer project, also focuses on secret injection. Adds redaction from console/log output. Mentioned in comments as complementary.

## astral-key Integration

**astral-key** (github.com/reverb256/astral-key) is a Web3/FIDO2/Passkey authentication microservice with Vaultwarden backend (Rust/Axum, NixOS module). SecretSpec integration points:

### Decision: Vault-compatible endpoint (Option A)

Chosen approach after brainstorming. Issue: https://github.com/reverb256/astral-key/issues/16

astral-key will expose a `GET /api/v1/secret/data/<path>` endpoint returning
the Vault KV v2 JSON shape. SecretSpec's existing `vault://` provider already
parses this format — **no SecretSpec fork required.**

```toml
[providers]
astral = "vault://http://astral-key:8080/v1/secret"

[profiles.production]
DATABASE_URL = { providers = ["astral"] }
```

**Why this over other options:**

| Option | Verdict | Reason |
|--------|---------|--------|
| **A: Vault endpoint** | ✅ **Do this first** | No SecretSpec changes. Rust stdlib. Unblocks everything else. |
| B: dotenv export | 📝 Document only | Works today, no build needed. Single-shot, no auth. |
| C: Keyring provider | ⏳ Defer | Per-machine, needs D-Bus (missing in many NixOS containers). |
| D: MCP agent tokens | 🔜 After A | Depends on the Vault endpoint existing to issue tokens against. |

### Integration architecture

```
User authenticates  ──>  JWT session token
                              │
                              ▼
Client resolves secrets:  GET /api/v1/secret/data/JWT_SECRET
                              │
                              ▼
SecretSpec vault provider:  vault://http://astral-key:8080/v1/secret
                              │
                              ▼
SOPS provider chain (Phase 2; pending cachix/secretspec PR #58 OR our crate build): age_key resolved from astral-key → decrypt sops files
```

### Provider credentials chain (mirrors SOPS provider architecture)

The same pattern used by the SOPS provider (being built) applies to astral-key:

```toml
[providers]
keyring  = "keyring://"
astral_vault = {
  uri = "vault://http://astral-key:8080/v1/secret?auth=approle",
  credentials = { role_id = "keyring" },
}

[profiles.production]
JWT_SECRET   = { providers = ["astral_vault"] }
DATABASE_URL = { providers = ["astral_vault"] }
```

> *Notes:*
> 1. **`[providers.astral_vault]` as an inline table** with `uri` + `credentials`
>    matches SecretSpec's documented provider-config trait as of v0.15+ —
>    `vault`, `akv`, and `bws` upstream all use the same shape
>    (`uri = …, credentials = { … }`). Labelled illustrative in earlier
>    drafts because CONTEXT.md didn't have the v0.15 spec; that's now
>    resolved — the TOML on disk is confidence-checked. The `vault://` URI
>    itself is real (CONTEXT.md providers table).
>    providers table).
> 2. **Per-secret vs. profile-wide shared defaults.** This block uses
>    *per-secret* overrides (`JWT_SECRET = { providers = ["astral_vault"] }`
>    — one rule per secret). For *profile-wide shared* settings (apply
>    `astral_vault` to every secret in production automatically), the
>    pattern is `[profiles.production.defaults] providers = ["astral_vault", ...]`
>    — the actual `secretspec.toml` in this repo uses that pattern. A reader
>    copying this illustration verbatim would diverge from the file's own
>    production profile shape; don't.
> 3. **The `vault://…/v1/secret` URI here is generic** — the `/data/<path>`
>    suffix Vault KV v2 requires for a single-secret read is appended from
>    the secret name when SecretSpec resolves the value (e.g., `JWT_SECRET`
>    → `/v1/secret/data/JWT_SECRET`). By contrast, the credentials block in
>    `sops-provider-design.md` uses an *explicit path* (`…/data/age_key`)
>    because it specifies which credential gets fetched — different
>    abstraction level, same Vault KV v2 primitive.

This is architecturally identical to how the SOPS provider resolves `age_key`
from keyring → decrypts sops files. Both use SecretSpec's provider credentials
pattern (Domen Kozar's requirement for upstream acceptance).

### Full docs

See astral-key tracking issue for architectural discussion:
https://github.com/reverb256/astral-key/issues/16

(`docs/secretspec.md` in the astral-key repo is the upstream-facing companion
doc and will be linked here once it ships — kept out of this file to avoid a
dangling cross-repo link.)
