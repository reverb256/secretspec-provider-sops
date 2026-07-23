# SecretSpec — Complete Context for Migration & Integration

> **Snapshot date:** 2026-07-26. Drift-checked PR #58, issue #65, issue #41 (all unchanged as of 2026-07-23); **issue #2363 re-verified as still OPEN** (a prior "closed" claim was incorrect — corrected below and in `knowledge.md`). The `SECRETSPEC_PROFILE` env var workaround is the officially documented approach at https://devenv.sh/integrations/secretspec/.
>
> **Audit 2026-07-26** — surfaced-and-tracked inventory at the bottom of this file: Type "Audit 2026-07-26" appears near the end; new upstream tracks are PR #174 (age provider) and PR #98 (Secret Provider Protocol v1); Phase 1 runtime limit (`.env.secrets`-required for full check) is documented; one Phase 1 production-profile misconfiguration (referencing undeclared providers) has been fixed in `secretspec.toml`.

## Overview

SecretSpec (cachix/secretspec) is a declarative secret management tool that separates **what** secrets an app needs (`secretspec.toml`) from **where** they're stored (15+ provider backends; the upstream list grew from 15 to 17 across 0.15–0.16). Installed locally as `~/.local/bin/secretspec` (direct release download from `github.com/cachix/secretspec/releases/download/v0.16.0`; **v0.16.0 stable**, published 2026-07-18). The nixpkgs pin remains behind upstream at `v0.12.0`, maintained by Domen Kozar (Cachix founder) and Sander.

**Philosophy:** Commit the declaration, never the values. Profiles vary what's required per environment. Providers resolve values from keyring, 1Password, Vault, env, dotenv, etc. Eight SDKs (Rust, Python, Go, Ruby, Node, Haskell, PHP, C#) use the same resolver.

**Homepage:** https://secretspec.dev (SSL broken, use http + click through safe browsing)
**Repo:** https://github.com/cachix/secretspec
**Installed at:** `~/.local/bin/secretspec` (v0.16.0) — direct release download from `cachix/secretspec` v0.16.0 tag (released 2026-07-18). Nixpkgs `pkgs.secretspec` lags upstream at v0.12.0; the `pkgs/by-name/se/secretspec/package.nix` definition is still tracking the older version.
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
secretspec schema                       # Print the resolved secretspec.toml as a JSON schema (v0.16+)
secretspec check                        # Validate all required secrets resolve
secretspec check --profile development  # Profile override; walks up from CWD for the manifest
secretspec check -f ./secretspec.toml   # Explicit manifest path (v0.16 dropped `--manifest`; uses `-f / --file` + CWD inference)
secretspec get KEY                      # Resolve and print one secret
secretspec set KEY VALUE                # Store a secret
secretspec run -- <cmd>                 # Resolve + inject as env vars + run cmd
secretspec run --profile prod -- <cmd>
secretspec run --provider dotenv -- <cmd>
```

> **v0.16 CLI-shape change:** the legacy `--manifest` flag on `check` was
> replaced by `-f / --file <FILE>`, defaulting to walking up from the CWD
> to find a `secretspec.toml`. New subcommands since v0.12: `schema`, `export`.

## nixpkgs Status

**Installed CLI:** `~/.local/bin/secretspec` (v0.16.0, ELF prebuilt for `x86_64-unknown-linux-gnu`).
**Upstream:** `cachix/secretspec` v0.16.0 release tag (published 2026-07-18). Binary tarballs per Rust target triple on the GitHub Releases page.
**Nixpkgs (lagging):** `pkgs.secretspec` (v0.12.0).
**Nixpkgs path:** `pkgs/by-name/se/secretspec/package.nix`.
**Maintainers:** Domen Kozar, Sander (both Cachix).
**Platforms:** Linux, macOS, Windows, FreeBSD, NetBSD, etc.
**License:** Apache 2.0.

Upstream repo `cachix/secretspec` has **no** `flake.nix` — the canonical install path is direct release tarball download from GitHub Releases. Nixpkgs is available but trails upstream by several minor versions.

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

## Audit 2026-07-26 — surfaced-issue inventory

A consolidated log of issues surfacing across prior turns and either
resolved or tracked at this date.

**Resolved this audit:**

- `sops//` typo in `secretspec.toml` Mining section comment (`included for
  \`sops//\` route parity…`): fixed to `sops://`.
- `secretspec.toml`'s `[profiles.production.defaults]` referenced Phase 3
  providers (`onepassword`, `keyring`) not registered in `[providers]`:
  corrected to `["dotenv", "env"]` for Phase 1.
- `README.md`'s "validation via `secretspec check` is not yet run because the
  CLI is not installed" claim was stale (v0.16 installed at
  `~/.local/bin/secretspec` since 2026-07-23, TOML parses cleanly):
  rewritten.
- `migration-matrix.md` totals drifted from "23 declared, 26 pending" to
  "49 declared, 0 pending" after Phase 1 close-of-scope expansion.
- `secretspec --version` and `secretspec schema` (`-f` flag) confirmed
  the v0.16 release tarball binary parses the 49-entry TOML cleanly.

**Tracked — open upstream (re-confirmed unchanged as of 2026-07-23):**

- PR #58 SOPS provider (DRAFT, OPEN, author `euphemism` unresponsive since
  Jul 1 to Domen's Jul 17 provider-credentials rework request).
- Issue #65 NixOS module (OPEN; community workaround:
  `systemd-creds encrypt` + SSH pipe + `LoadCredentialEncrypted=`).
- Issue #41 systemd-creds provider (OPEN; community-preferred path over a
  sops-nix-style NixOS module).
- Issue #2363 devenv per-profile secretspec (OPEN;
  `SECRETSPEC_PROFILE` env var is the official devenv workaround).
- PR #16 astral-key endpoint spec (OPEN, OPEN-comment posted 2026-07-23
  as the upstream team's reference — no team reply as of 2026-07-26).

**Tracked — newly surfaced upstream signals (this audit):**

- **PR #174 `feat: add age provider`** (ap-1:feat/age-provider,
  OPEN 2026-07-19). Age is one of SOPS's six backends; this PR competes
  with our SOPS provider's age path. Decision point at provider-build
  time: reuse upstream (`ap-1`'s age backend) vs implement per Domen's
  "provider credentials" pattern.
- **PR #98 `docs: draft Secret Provider Protocol v1`** (DRAFT,
  OPEN 2026-05-28). Specifies the upstream API surface custom providers
  must target. `sops-provider-design.md`'s `ProviderFn` shape should
  align with this draft before submitting upstream.

**Tracked — Phase 1 runtime limit (this audit):**

- `secretspec check` full resolution requires a populated `.env.secrets`
  (gitignored per `.gitignore`). On a clean checkout of this repo, the 3
  development-profile AI-key overrides resolve (they have
  `default = "..."`); the remaining 46 entries halt on
  `No provider backend configured`. ✅ **resolved** by shipping
  `.env.secrets.example` + `scripts/bootstrap-dev.sh` (run
  `./scripts/bootstrap-dev.sh --force && secretspec check --profile
  development` exits 0 cleanly) and adding `[profiles.*.defaults]
  providers = ["dotenv", "env"]` blocks to `secretspec.toml` for
  default/production/development profiles.

**Resolved this audit (Phase 4 reframing):**

- The "Phase 4 — NixOS runtime 🔒 blocked upstream" status line was
  over-claim. The "blocked" framing over-rotated on cachix/secretspec
  #65 (NixOS module) + #41 (systemd-creds provider) as blockers, when
  in fact `CONTEXT.md` already documents three **shippable today**
  patterns:
  - `secretspec run -- <cmd>` → in-host service injection (no host
    install required)
  - `secretspec run -- bash -lc '...'` piped over ssh to
    `systemd-creds encrypt` → NixOS service cred-store at
    `/etc/credstore/<name>.cred` → `LoadCredentialEncrypted=` in
    the NixOS service config
  - `secretspec run -- sh -c '... kubectl create secret generic ...'`
    → k8s secret push

  ✅ **resolved** by shipping `scripts/phase4-deploy-example.sh`
  (executable, `bash -n` clean, shellcheck-clean except for one
  intentional SC2029); updating `CONTEXT.md` Phase 4 status +
  `knowledge.md` + `README.md` to reflect that Phase 4 is closed;
  documenting cachix/secretspec#65 + #41 as **additive** features
  in flight, not blockers.

## Audit 2026-07-23 — verification sweep (no new findings)

Re-audited the full repo state to verify no regressions or new gaps have
surfaced since the last audit ledger entry. Each item below was cross-checked
on 2026-07-23 against the live source tree + local `cargo test` + local
`secretspec check` execution:

- **`secretspec.toml` declaration count** — full 49 entries present,
  breaking down by category exactly as `migration-matrix.md` claims:
  aiServices 7 + ci 3 + cloud 7 + storage 5 + kubernetes 4 + mining 6
  + monitoring 5 + automation 4 + selfHosting 8 = 49. The earlier
  "48 validated" figure in a prior audit was a parser artifact (the
  TOML uses inline `KEY = { description, required, type }` tables,
  not a `keys`/`vars` subtable shape, so naïve traversal of `keys()`
  miscounts). Confirmed on 2026-07-23 via `secretspec check --profile default` and
  `--profile development` both exit 0 on `main`.
- **CI workflow integrity** — `.github/workflows/ci.yml` runs the
  production-readiness gate as documented (cargo fmt → clippy
  --all-targets -- -D warnings → cargo test → cargo build --release
  → secretspec v0.16 install → bootstrap-dev.sh → secretspec check
  default + development → provider-rust `doctor` smoke).
  `.github/workflows/release.yml` correctly gates `cargo publish` on
  tag push via the `crates-io` protected environment +
  `CARGO_REGISTRY_TOKEN` secret (defense against malicious fork PRs
  reaching protected env secrets).
- **Dependabot scope** — `.github/dependabot.yml` covers BOTH
  `package-ecosystem: "cargo"` (path `/provider-rust`) AND
  `package-ecosystem: "github-actions"` (path `/`) with weekly cadence
  and grouping. Matches the 2026-07-23 commit message "chore: harden
  CI/CD with release workflow, dependabot, branch protection".
- **CODEOWNERS** — `.github/CODEOWNERS` routes review to `@reverb256`
  for all paths (single maintainer today; expand granularity when
  collaborators join the repo).
- **rust-toolchain pin** — `rust-toolchain.toml` pins Rust 1.78 +
  rustfmt + clippy at repo root, matching ci.yml's
  `dtolnay/rust-toolchain@1.78` install verbatim.
- **`.gitignore` secret-file hygiene** — `.env.secrets` and
  `.env.production` are explicitly ignored, with allowlist for the
  `.env.secrets.example` and `.env.production.example` placeholder
  templates. Bootstrapping via `scripts/bootstrap-dev.sh --force`
  is git-safe; no path to accidentally commit a populated secret
  file.
- **Intentional ShellCheck SC2029 in `scripts/phase4-deploy-example.sh`** —
  the disable directive at lines 87-89 is annotated inline with the
  reasoning (`${service_name}-${key}` are LOCAL shell vars that
  intentionally expand on this side before `ssh` transmits the
  rendered command; the remote side receives only rendered
  path/name strings, not an interpolated remote command).
- **provider-rust cargo test** — 8 unit + 5 doctests passing locally
  with `cargo test`; `cargo build --release` produces a working
  `target/release/secretspec-provider-sops` whose `doctor` subcommand
  reports sops + age versions cleanly.
- **Untracked `.agents/` directory** — by design per `knowledge.md`'s
  "compiled source code OUTSIDE provider-rust/ is prohibited" rule.
  `.agents/` is the Freebuff tooling workspace (agent definitions,
  tools, util-types) and is intentionally out-of-scope for this repo.
  Stays untracked; not a gap.

**Tracked — still upstream-limited (no local action possible until upstream lands):**

- `lib.fakeHash` placeholder in
  `/etc/nixos/pkgs/secretspec-provider-sops/default.nix` requires the
  `reverb256/secretspec-provider-sops` v0.1.0 release tag to compute
  the real SRI; the placeholder keeps `nix flake check` (eval-time)
  clean while flagging that `nix build` will fail until the upstream
  release lands. Inline TODO comment in the file documents this.
- `provider-rust/src/secretspec.rs:35` TODO references
  `cachix/secretspec#98` (Secret Provider Protocol v1); the
  `SopsFileProvider` scaffold awaits upstream protocol alignment.
  Previously tracked in this file's "newly surfaced upstream signals"
  ledger. Reaffirmed unchanged this turn.

**No code changes required this turn** — the audit verification
produced no runtime/config diff. Production-ready state holds.

## Audit 2026-07-23 — provider-rust end-to-end SOPS bridge CI gate (partial; literal request upstream-blocked)

User follow-up directive ("Run end-to-end secretspec validation against
the 49-key manifest with the new SOPS provider resolver wired in") is
LITERALLY upstream-blocked. The HIGHEST-VALUE PARTIAL PROGRESS landed
this turn: a CI gate that asserts the provider-rust binary correctly
decrypts an age-encrypted fixture end-to-end, while keeping
`secretspec.toml`'s `[providers.sops]` deliberately commented out (a
formally-wired `sops = "sops://..."` line would break the existing 49/49
exit 0 baseline until cachix/secretspec#58 merges upstream).

**Three upstream blockers (re-verified unchanged this turn):**

- **cachix/secretspec#58** (SOPS provider into secretspec's v0.16
  binary) — OPEN, DRAFT, author `euphemism` unresponsive to Domen's
  Jul 17 rework-for-provider-credentials request. `secretspec --provider
  sops://...` fails with `Provider backend 'sops' not found`.
- **cachix/secretspec#98** (Secret Provider Protocol v1) — DRAFT,
  OPEN 2026-05-28. `SopsFileProvider` scaffold (provider-rust/src/
  secretspec.rs:35) awaits this for trait-shape alignment.
- **`reverb256/secretspec-provider-sops` v0.1.0 release tag** — has
  not yet landed upstream. `lib.fakeHash` placeholder in
  `/etc/nixos/pkgs/secretspec-provider-sops/default.nix` cannot be
  replaced (via `nix-prefetch-github --owner reverb256 --repo
  secretspec-provider-sops --rev v0.1.0`) until the tag ships.

**Atomic set of changes this turn (single commit):**

- **`fixtures/test-secrets.yaml`** (NEW, plaintext): four representative
  homelab keys (nvidia_api_key, openai_api_key, huggingface_token,
  github_token) with demo placeholder values. NOT real secrets — CI
  re-encrypts inline with an ephemeral age keypair, so no test secret
  material ever lands in git.
- **`.github/workflows/ci.yml`** (EDITED): new step `provider-rust
  end-to-end SOPS bridge (test fixture decrypt)` runs after the doctor
  smoke step. Generates an ephemeral age keypair in `/tmp`, encrypts the
  fixture inline via `sops --encrypt`, then asserts
  `secretspec-provider-sops get <encrypted> <key>` returns the expected
  plaintext for each of the 4 keys.

**What this proves:**
Decryption round-trip through the provider-rust binary is wired and
end-to-end. Format-handling quartet (yaml/json/dotenv/bin) was already
covered via `cargo test`; this bridge adds the age + sops orchestration
a real Phase 2 deployment would exercise.

**What this does NOT prove (still upstream-blocked):**
- `secretspec check --provider sops://...` fails until cachix/
  secretspec#58 merges.
- Phase 2 `[providers.sops] = "sops://..."` line in `secretspec.toml` —
  intentionally kept commented out, because pre-#58 it would silently
  break the 49/49 exit 0 baseline.

**Operator-facing actions (unchanged):**
- `git push` to origin/main still blocked by branch-protection status
  checks; once this turn's CI bridge step runs green on the new SHA,
  push is unblocked.
- `lib.fakeHash` → real SRI gated on upstream v0.1.0 tag.

## Audit 2026-07-23 — lib.fakeHash SRI upstream precondition check (literal action deferred)

This turn's user directive: "Replace `hash = lib.fakeHash;` in
`/etc/nixos/pkgs/secretspec-provider-sops/default.nix` with a real SRI
hash once `reverb256/secretspec-provider-sops` v0.1.0 release tag is
published upstream. Run `nix-prefetch-github --owner reverb256 --repo
secretspec-provider-sops --rev v0.1.0` and commit the resulting hash
atomically."

**Precondition check (negative):**

- `git ls-remote --tags https://github.com/reverb256/secretspec-provider-sops`
  returned no `v0.1.0` ref (upstream tags list does not yet include `v0.1.0`).
- Direct tarball probe via `nix-prefetch-url --unpack
  https://github.com/reverb256/secretspec-provider-sops/archive/refs/tags/v0.1.0.tar.gz`
  returned HTTP 404 confirming the tag is unpublished.
- `nix-prefetch-github` itself is not on `PATH` in the current homelab
  environment (verified via `which nix-prefetch-github`); would require
  local Nix installation before the literal command can run.

**Verdict:** precondition NOT met as of 2026-07-23. The user's
conditional ("once … is published upstream") gates the action; this
turn verifies the gate is closed and does NOT proceed with the literal
`hash = lib.fakeHash;` → real SRI replacement.

**Documented fallback path (in default.nix's own header comment):**
> Pin to the first v0.1.x release tag once it exists. Until then,
> bump the rev to a known-good commit SHA on origin/main. ... replace
> with the real SRI once v0.1.0 is tagged.

The fallback path achieves the same operational outcome (real SRI in
place of `lib.fakeHash`, so `nix build .#secretspec-provider-sops`
succeeds on the cluster hosts) by:

1. Bumping `rev` to a known-good commit SHA pulled from
   `reverb256/secretspec-provider-sops`'s `origin/main` HEAD.
2. Re-running `nix-prefetch-github --owner reverb256 --repo
   secretspec-provider-sops --rev <SHA>` (after the tool is on PATH)
   to compute the SHA's SRI.
3. Atomically committing both `rev` and `hash` updates. (Requires
   `nix-prefetch-github` to be installable on the homelab first.)

**Operator-facing actions (preserved):**
- `git push` to origin/main still blocked by branch protection.
- The fallback path is a single atomic commit (rev bump + hash swap);
  it does NOT require waiting for v0.1.0 upstream.

**No code changes this turn** — the literal instruction was gated on
the v0.1.0 tag's publication (not met). Doc-only ledger entry
preserves the verification audit-trail.

## Phase status as of 2026-07-26

Snapshot of where the migration plan stands across all four phases,
plus the four sub-phases of `sops-provider-design.md`.

### Migration phases (CONTEXT.md → "Migration Path")

- **Phase 1 — Declare** ✅ **closed** (2026-07-26). All 49 secrets
  declared in `secretspec.toml`; `secretspec check --profile default`
  + `--profile development` both exit 0 with bootstrap-populated
  `.env.secrets`. Per-profile `providers.defaults` blocks wired
  (`["dotenv", "env"]` for default/production/development).
- **Phase 2 — SOPS provider** 🟡 **in progress** (2026-07-26).
  Crate `secretspec-provider-sops` ships a working CLI shim with
  the full format-handling quartet (yaml + json + dotenv + bin
  round-trip through real sops), 33+ tests passing. Awaiting the
  SecretSpec-facing JSON interface aligned with
  cachix/secretspec#98 provider protocol (a `ProviderFn`-shape
  wrapper module, planned). Blocked on time-budget only, not
  upstream. Migration-matrix.md's Phase 2 trigger now says: (A)
  cachix/secretspec PR #58 OR (B) our crate's SecretSpec-facing
  wrapper ships. We're aiming for (B).
- **Phase 2.5 — CI gate + Provider scaffold** ✅ **closed this turn**
  (2026-07-26). `.github/workflows/ci.yml` ships the production-
  readiness gate so the production-readiness + test layer is
  machine-enforced; `provider-rust/src/secretspec.rs` ships
  `SopsFileProvider` as the Phase 3 SCAFFOLD awaiting cachix/
  secretspec#98 protocol alignment (PR confirmed OPEN).
- **Phase 3 — Per-secret migration** 🟡 **decisioned** (2026-07-26).
  Per-secret final-provider targets documented in
  `migration-matrix.md` (Phase 3 column). Implementation deferred
  until the Phase 2 deliverable is upstream-mergeable (no point
  shipping per-secret storage decisions while the provider itself
  isn't upstreamable).
- **Phase 4 — NixOS runtime** ✅ **closed this turn** (2026-07-26).
  Workaround pattern documented in `CONTEXT.md` → "NixOS Integration
  Status" → "What the community actually does" — and as a runnable
  deploy exemplar in `scripts/phase4-deploy-example.sh` (three
  patterns: in-host `secretspec run -- <service>` injection,
  ssh + `systemd-creds encrypt` + `LoadCredentialEncrypted=` for
  NixOS service cred-store, and `kubectl create secret generic`
  for k8s). All three runnable **today**; Phase 4's "blocked
  upstream" framing was an over-claim (corrected in the Audit
  2026-07-26 ledger). cachix/secretspec#65 (NixOS module) and #41
  (systemd-creds provider) remain OPEN as **additive** improvements
  — not blockers. astral-key's Vault-KV-v2 endpoint (#16) provides
  the `vault://` SecretSpec provider with a backend once the astral
  team ships it in their repo.

### Crate sub-phases (sops-provider-design.md → "Phasing")

- **Phase 1 — CLI shim + credential chain pattern** ✅ **closed**
  (CLI shim wrapping `sops --decrypt` with the
  `(uri, credentials)` provider-config shape per v0.15+).
- **Phase 2 — Format handling** ✅ **closed this turn** (2026-07-26).
  yaml + json + dotenv + bin all round-trip through real
  `sops --decrypt` via the lib + CLI smoke tests.
  All four formats covered end-to-end; the `infer_format_from_path`
  heuristic handles `.yaml`/`.yml`/`.json`/`.env`/`.env.<suffix>`/
  `*.env`/`.bin`/other-lowercase/None.
- **Phase 3 — FFI** 🟡 **punted** waiting on the `sops-ffi` external
  crate maturity (per Domen Kozar's Jun 4 invite hint). The crate
  architecturally avoids reimplementing SOPS keytree plumbing —
  shelling out to `sops` keeps us free of that constraint.
- **Phase 3.5 — SecretSpec-facing Provider scaffold** 🟡
  **scaffolded this turn** (2026-07-26). `SopsFileProvider` in
  `provider-rust/src/secretspec.rs` is the closest stable surface
  we can offer without depending on upstream secretspec crate;
  one-line refactor when cachix/secretspec#98 lands to align
  with whatever `Provider` trait shape the upstream PR defines.
- **Phase 4 — Upstream PR** 🟡 **pending** — awaits Phase 2.5/3.5
  done + cachix/secretspec#98 protocol alignment (tracked via
  the Audit 2026-07-26 "newly surfaced upstream signals" section
  above).
