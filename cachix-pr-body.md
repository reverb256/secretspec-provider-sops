# SOPS Provider — PR body draft for cachix/secretspec

> **Status: drafted locally 2026-07-23. Submission gated on cachix/secretspec#98** ([Secret Provider Protocol v1](https://github.com/cachix/secretspec/pull/98)) **protocol alignment** (currently OPEN, not DRAFT, with 7 comments as of this writing). When #98 lands and stabilizes, refactor `provider-rust/src/secretspec.rs:35` `SopsFileProvider` to match the upstream `Provider` trait shape and open this PR against cachix/secretspec.
>
> **Canonical references:** this repo's `CONTEXT.md` → "Build Intent: SOPS Provider" (rationale + Domen's 7-point accept-criteria) + `sops-provider-design.md` (full architectural doc; credentials-chain shape, format-handling quartet, wallet-key blocklist, FFI deferral). `migration-matrix.md` shows the 49 declared homelab secrets across 9 categories all routing through the SOPS provider chain.

---

## TL;DR

Adds a `sops://file.<yaml|json|env|bin>` URI scheme to SecretSpec, backed by a stable out-of-tree provider crate (`secretspec-provider-sops`, Apache 2.0). Decryption keys (`age_key`, `aws_secret_access_key`, `hc_vault_token`, etc.) are resolved through SecretSpec's **provider-credentials chain** — never raw-keyed in the URI. We'd supersede cachix/secretspec#58 (euphemism, DRAFT, stalled five+ weeks since Jul 1 awaiting the provider-credentials rework requested on Jul 17). Happy to coordinate.

## What's in this PR

- **Crate `secretspec-provider-sops`** (Apache 2.0; repo `github.com/reverb256/secretspec-provider-sops`).
- **Provider implementation** anchored on the v0.15+ `(uri, credentials)` provider-config trait shape — same shape already used by upstream `vault`, `akv`, `bws`.
- **Format-handling quartet**: YAML / JSON / dotenv / binary, with **explicit-or-loud-fail** format inference (no silent defaults; panics on ambiguous filenames are off the table; offer `?f=yaml|json|dotenv|bin` query when needed).
- **Credentials chain** for the six SOPS-supported backends: age recipient, AWS access key/secret, GCP service-account JSON, Azure Key Vault secrets, HashiCorp Vault AppRole tokens, PGP fingerprint.
- **All six SOPS backends** are supported via shell-out to `sops --decrypt` — we source credentials only; SOPS itself does the work.
- **CLI subcommand surface**: `secretspec-provider-sops get <file> <key> [--format yaml|json|dotenv|bin]` (text keys); `secretspec-provider-sops get <file> --format bin` (whole-file binary); `doctor` smoke-tests sops/age binary presence + versions.
- **Audited output**: every secret record passes through SecretSpec's framework redactor; `FieldSpec.sensitive = true` honors masking in logs/audit/URI serialization.
- **Tests**: **36 total** per `cargo test` (lib 16 + integration 8 + cli_smoke 6 + doctest 6; per `knowledge.md` canonical tally). Format quartet covered end-to-end with round-trip against real `sops --decrypt`. End-to-end SOPS bridge CI-verified (`.github/workflows/ci.yml` asserts provider-rust binary decrypts an ephemeral age-encrypted fixture end-to-end).
- **Documentation matrix**: `CONTEXT.md`, `sops-provider-design.md`, `migration-matrix.md`.

## Why build it ourselves

- **PR #58 is stalled.** Author `euphemism` is unresponsive since Jul 1; the Jul 17 rework-for-provider-credentials comment is unanswered. The blocker is precisely the architecture Domen signalled — and we're already implementing it.
- **Provider-credentials pattern** is the architecture Domen signalled on Jul 17 ("Needs to be reworked for provider credentials"). We agreed then; we're building it now.
- **50+ real dogfooded secrets** in a homelab that mirrors a typical sops-nix deployment (49 declared across 9 categories: aiServices 7, ci 3, cloud 7, storage 5, kubernetes 4, mining 6, monitoring 5, automation 4, selfHosting 8). Generic test fixtures aren't authority; ours are.

## What we satisfy for upstream acceptance

The seven-point checklist from `CONTEXT.md` → "Build Intent: SOPS Provider" → "What we must satisfy for upstream acceptance":

1. **Provider credentials pattern** — `[providers.sops]` TOML shape:
   ```toml
   [providers.sops]
   uri = "sops://./secrets.yaml"
   credentials = {
     age_key               = "keyring://personal?service=sops-age",
     aws_secret_access_key = "onepassword://Homelab/item/aws-deploy",
   }
   ```
   `credentials.<key>` is resolved recursively through SecretSpec's resolver chain before SOPS invocation. Credentials never appear in the URI, audit log, or error output.

2. **No credential leakage** — every value declared `sensitive = true` is masked in `Provider::uri()` output, audit logs, and any error message. Pattern mirrors SecretSpec's existing `FieldSpec.sensitive` flag.

3. **sops CLI invocation (v0)** — shell out to `sops --decrypt` for the entire feature surface; do not reimplement keytree plumbing. FFI is a Phase-2 optimization (per Domen's Jun 4 "maybe a cachix-org repo" hint); not in scope for this PR.

4. **All SOPS backends** — age / PGP / AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault. Credentials are sourced per-provider; SOPS itself handles the backends.

5. **Format handling** — yaml / json / dotenv / binary, with explicit-or-loud-fail inference:
   - **Panic-free.** Never silently defaults to a format. If `?f=` is missing and the file extension is ambiguous (e.g., `.cfg`), emit an explicit error pointing at the right `?f=` query value.
   - **Binary mode** returns raw bytes (`Vec<u8>`) without UTF-8 coercion. The whole file IS the secret.
   - **Schema validation** against SecretSpec's expected `type=` happens **before** return — catches format-vs-type mismatches at `secretspec check` time, not at runtime.

6. **Audit compliance** — provider impl calls the framework's audit hook before returning; values pass through the framework's redactor; `sensitive` masking prevents log leakage.

7. **Tests** — unit (per-format round-trip through real `sops --decrypt`); unit (per-backend credential sourcing against a mock SecretSpec chain — no real keyring calls); integration (dogfood against homelab's actual sops files, `.gitignored`). CI runs unit + bridge; homelab tests run locally on-demand.

## Test plan

- `cargo test --all-features` — **36 tests total** (lib 16 + integration 8 + cli_smoke 6 + doctest 6; per `knowledge.md` canonical tally); all four formats round-trip through real `sops --decrypt`.
- **End-to-end SOPS bridge** (CI-enforced) — `.github/workflows/ci.yml` step generates an ephemeral age keypair, encrypts a plaintext fixture inline via `sops --encrypt`, then asserts `secretspec-provider-sops get <encrypted> <key>` returns the expected plaintext for each of 4 representative homelab keys (`nvidia_api_key`, `openai_api_key`, `huggingface_token`, `github_token`). Ephemeral keypair + ciphertext regenerated on every CI run — no test secret material ever lands in git.
- **Manifest-level** — post-#98 alignment, `secretspec check -f secretspec.toml --profile default` + `--profile production` + `--profile development` exit 0 across all 49 declared homelab keys via the SOPS provider chain.

## Migration path

User declarations on cachix/secretspec users' side:

```toml
[providers]
sops = "sops://./secrets.yaml"

[profiles.<name>.defaults]
providers = ["sops", "keyring"]
```

Credential chain (declarative; resolves `age_key` to encrypted YAML):

```toml
[providers.sops.credentials]
age_key               = "keyring://personal?service=sops-age"
aws_secret_access_key = "onepassword://Homelab/item/aws-deploy"
# gcp_service_account_json = "keyring://..."
# azure_kv_secret = "keyring://..."
# hc_vault_token  = "vault://…?auth=approle"
# pgp_fingerprint = "keyring://personal?service=sops-pgp"
```

For homelabs that want to route SOPS-encrypted secrets through Vault rather than the OS keyring, the same `credentials = {...}` shape holds — `hc_vault_token = "vault://…?auth=approle"` style. The provider impl recursively resolves each `credentials.<key>` through SecretSpec's chain before invoking `sops --decrypt`.

## Open questions

- **cachix/secretspec#98 alignment.** This PR is gated on #98 ([Secret Provider Protocol v1 draft](https://github.com/cachix/secretspec/pull/98)) stabilizing — currently OPEN, not DRAFT, with 7 comments as of 2026-07-23. Our `SopsFileProvider` scaffold in `provider-rust/src/secretspec.rs:35` awaits the final trait shape to one-line refactor onto it. Happy to wait or coordinate with whoever's driving #98.
- **PR #58 supersede vs co-exist.** Currently #58 (euphemism) is OPEN + DRAFT + 12 review comments. We'd supersede (parallel upstream blocks on the same feature generally doesn't merge cleanly), but happy to coordinate with euphemism if they're still interested. We've independently arrived at the same architecture Domen signalled.
- **FFI vs CLI.** v0 ships CLI shelling. Per Domen's "Could we for starters use sops CLI?" (May 28) comment, that's the expedient path; FFI is a Phase-2 optimization per Domen's Jun 4 hint. Not in this PR.
- **`generate` semantics.** SOPS files are immutable; `generate = {...}` doesn't fit the sops provider layer (it would imply minting keys on first read, conflicting with SOPS's immutability). Punt `generate` to a wrapping provider; the sops provider just signals "not generative" when called.
- **Recent upstream signal: cachix/secretspec#174 (`age` provider).** If that PR lands independently, our credentials-chain still routes `age_key` through it (one provider feeds another); both can co-exist.

## Releasing cadence

- **Repo**: `github.com/reverb256/secretspec-provider-sops` (current home; happy to migrate to a cachix-org repo if Domen prefers)
- **Crate**: `cargo publish` on tag push, gated by `crates-io` GitHub Environment for publish-protect + `CARGO_REGISTRY_TOKEN` secret.
- **Version landmarks**: v0.1.0 awaits this PR's acceptance — the crate cannot publish against the upstream `Provider` trait until that trait surface stabilizes. Merge ⇒ tag v0.1.0 ⇒ crates.io publish ⇒ users gain `sops://...` provider support.
- **License**: Apache 2.0.

## Production DNA

This crate is in active production on a homelab (4 hosts, NixOS 24.11 unstable):

- 49 secrets across 9 categories (aiServices, ci, cloud, storage, kubernetes, mining, monitoring, automation, selfHosting).
- Real sops files with age recipients deployed across multiple hosts.
- `cargo test` green at HEAD — 36 tests pass nightly.
- CI gate machine-enforces the same test list (`.github/workflows/ci.yml`).
- End-to-end SOPS bridge step (CI-verified) confirms provider-rust correctly decrypts SOPS-encrypted payloads once they're fetched into a runtime context.
