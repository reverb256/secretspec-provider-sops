# SOPS Provider Design

This is the architecture for a SecretSpec → SOPS provider that we will build,
dogfood against the homelab's 50+ age-encrypted SOPS files, and upstream to
cachix/secretspec. It encodes Domen Kozar's seven accept-criteria (CONTEXT.md →
"Build Intent") and the rationale the community PR #58 author has not yet
addressed.

> **Schema status (2026-07-26):** the inline-table `credentials = { … }` shape
> shown throughout this doc is no longer illustrative — it matches SecretSpec's
> v0.15+ documented provider-config trait. `vault`, `akv`, and `bws` upstream
> all use the same `(uri, credentials)` shape; v0.16 is the latest stable
> upstream release (2026-07-17) and is the target version for our crate.

## Goal

Replace `sops-nix`'s `sops.secrets.<name>.path` style with SecretSpec resolving
from existing `.age`-encrypted SOPS files. Once merged, every existing
sops-nix entry maps 1-to-1 onto a SecretSpec declaration that resolves the
same value from the same file — no decryption change, no migration risk.

## Why build our own

- **PR #58 is stalled** (open + draft, author `euphemism` unresponsive since
  Jul 1, 2026).
- Domen signalled exactly the architecture he wants on Jul 17: resolve
  decryption keys through SecretSpec's credential-chaining, not raw URI params.
- We have 50+ real encrypted secrets to validate against — genuine dogfooding
  data, not synthetic fixtures.
- Domen invited an externally-maintained crate on Jun 4
  ("maybe we create a repo on cachix?").

## Crate layout

Working name: `secretspec-provider-sops`. Lives in a separate repo
(hosting **TBD** — see "Hosting choice" below).

```
src/
  lib.rs              # provider trait impl re-exports
  provider.rs         # Sops struct + SecretSpec Provider impl
  uri.rs              # provider URI parser, honours FieldSpec.sensitive
  credentials.rs      # resolve age_key / cloud creds through SecretSpec chain
  backend/
    age.rs            # SOPS → age keypair lookup; SOPS env vars only
    pgp.rs            # PGP fingerprint sourcing
    aws_kms.rs        # AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY sourcing
    gcp_kms.rs        # GCP service-account JSON sourcing
    azure_kv.rs
    hc_vault.rs
  format/
    yaml.rs           # serde_yaml
    json.rs           # serde_json
    dotenv.rs         # dotenvy
    bin.rs            # opaque bytes; never coerce to UTF-8
cli/
  main.rs             # thin wrapper: spawn sops, capture stdout, return to SecretSpec
tests/
  fixtures/           # tiny .age-encrypted YAML/JSON/dotenv/bin files
  unit/               # backend & format handling, no real keyring calls
  integration/       # dogfood against homelab's actual sops files (.gitignored)
```

## Domen's seven requirements

### 1. Provider credentials pattern

**Domen's Jul 17 ask:** decryption keys resolved through SecretSpec's
credential-chaining, not raw URI params.

**Provider-declaration TOML (shape confirmed for v0.15+; v0.16 is the
target — `vault`, `akv`, `bws` use the same `uri + credentials` shape):**

```toml
[providers.sops]
uri = "sops://./secrets.yaml"
credentials = {
  # age_key is sourced through the astral-key vault-emulating endpoint
  # once astral-key Phase 1 ships (CONTEXT.md "astral-key Integration").
  # Until then, fall back to a local keyring-stored age recipient.
  age_key               = "vault://http://astral-key:8080/v1/secret/data/age_key?auth=approle",
  aws_secret_access_key = "onepassword://Homelab/item/aws-deploy",
}
```

If `astral-key` is not yet deployed in the environment, swap to:

```toml
age_key = "keyring://personal?service=sops-age",
```

The provider impl treats `credentials.<key>` as another SecretSpec resolution,
calling the resolver recursively before invoking `sops --decrypt`. Credentials
never appear in the URI, the audit log, or error output.

### 2. No credential leakage

**CONTEXT.md flag:** the PR #58 author was dinged for emitting `age_key`,
`aws_secret_access_key`, and `hc_vault_token` in `Provider::uri()` output
(Domen review Jun 20).

**Our pattern:** mirror SecretSpec's existing `FieldSpec.sensitive` flag.
Every value declared `sensitive = true` in the provider schema is masked
in `uri()`, audit logs, and any error message. Public values
(`public_age_recipient`, vault mount path) stay visible.

### 3. sops CLI invocation

**Domen endorsement May 28:** "Could we start by using sops CLI?"

**Our approach:** a thin CLI shim that spawns
`sops --decrypt --output <tmp> <file>` for v0. FFI is a later optimization
(Phase 2); the CLI shim covers the entire SOPS feature surface without
reimplementing keytree plumbing.

### 4. All SOPS backends

Age, PGP, AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault. Our approach
is **do not reimplement** — shell out to `sops`, which already supports all
these. We only need to wire credentials into SOPS's expected env vars
(`SOPS_AGE_KEY_FILE`, `SOPS_PGP_FP`, `AWS_ACCESS_KEY_ID`,
`AWS_SECRET_ACCESS_KEY`, `GOOGLE_CREDENTIALS`, `AZURE_*`, `HC_VAULT_ADDR`,
`HC_VAULT_TOKEN`). The snake_case `backend/*.rs` files in our crate are
about *credential sourcing*, not backend implementation.

### 5. Format handling

CONTEXT.md flags from PR #58's review: dotenv broken both read/write,
format inference panics on common filenames, binary mode semantics unclear.

**Our approach:**

- **Panic-free, explicit-preferred inference.** Provider accepts `?f=yaml|json|dotenv|bin`
  when the user wants explicit. When the query is *missing*, infer from the file
  extension (`.yaml` → yaml, `.dotenv`/`.env` → dotenv) — but never panic on
  unknown filenames. PR #58 inferred and panicked; we either infer successfully
  or fail loudly with a clear error pointing at the right query value. We do
  *not* silently default to a format.
- **`NoneType` for `?f=bin`.** Treat output as opaque bytes; never coerce
  to UTF-8. Returning bytes through SecretSpec's redaction layer is the
  caller's responsibility.
- **Schema validation.** Before returning, parse against SecretSpec's
  expected type (e.g., `type="password"`) and reject anything that doesn't
  fit. Catch format-vs-type mismatches at `check`, not at runtime.

### 6. Audit compliance

SecretSpec records every read. **Our approach:** the provider impl calls
the framework's audit hook before returning a value (the framework does
this; we don't bypass it). The resolved value passes through the framework's
redaction layer; if `sensitive` is set it never reaches log output.

### 7. Tests

- **Unit (per format):** fixture-driven YAML / JSON / dotenv / bin each
  round-trip through `format::*`. Especially the dotenv read+write symmetry,
  which PR #58 got wrong.
- **Unit (per backend):** fixture-driven credentials resolved against a
  mock SecretSpec chain (no real keyring calls; mock via
  `provider = "test://..."`).
- **Integration (dogfood):** `tests/integration/` references the homelab's
  actual sops files (`.gitignored`). CI runs against fresh-cloned fixtures
  so unit tests don't depend on real keys. The homelab's real tests run
  locally, on-demand.

## Wallet key blocklist

The OS keyring is *not* safe for high-value secrets — local malware can
exfiltrate it. **Internal convention: never store wallet keys (ETH/XMR) or
master age encryption keys in `keyring://`.** `secretspec.toml` comments
the relevant rows accordingly. Migration plan routes these to
`onepassword://` for daily use or `sops://` for cold storage.

## Hosting choice (open)

Two paths, both viable:

| Option | Pros | Cons |
|--------|------|------|
| **Personal fork** (euphemism-style) | Faster iteration, no org permission needed for repo creation | Upstream PR author ≠ origin; friction at merge time |
| **Cachix-org repo** (Domen's Jun 4 invite) | Upstream-aligned from day one | Requires cachix org to provision; slower start; needs Domen buy-in |

**Recommendation:** start in a personal fork for the credential-chain + CLI
shim implementation. Once the API surface is stable, ask Domen whether a
cachix-org repo is worth provisioning for the long-term home; migrate then.

## Phasing

1. **Phase 1 (this design doc + CLI shim).** Prove the credentials pattern.
   Use `--decrypt` against a single fixture file. Fits in one weekend.
2. **Phase 2 (format handling).** Add YAML / JSON / dotenv / bin modules.
   Test against the homelab's real files.
3. **Phase 3 (FFI).** Replace CLI shim with `sops-ffi` bindings when the
   external `sops-ffi` crate matures (per Domen's Jun 4 hint).
4. **Phase 4 (upstream PR).** Open the PR. Surface code in small PRs
   (credentials first, dotenv fix second) per the lesson from PR #58's
   review pain.

## Open questions

1. **Key addressing.** Direct SOPS YAML keys (e.g., `dockerhub_password`)
   vs. `ref` blocks (`{item, field, vault, ...}`). Leaning direct in v0,
   `ref` blocks later for multi-key files.
2. **`generate`.** Some secrets (per-scope GitHub tokens) want
   `generate = { … }` semantics. SOPS doesn't generate — adding `generate`
   to the sops provider means minting keys on first read, which conflicts
   with SOPS's immutability. Decision: punt `generate` to a *wrapping*
   provider, not the sops layer.
3. **Multi-account / per-host.** SOPS files are typically one YAML per
   environment. Per-host overrides: teach SecretSpec about file selection
   vs. leave as `uri = "sops://./secrets-${host}.yaml"`. Punted to v0.1.
4. **Composable secrets.** `composed = { … }` (CONTEXT.md; ≥0.16) lets a
   secret be derived from `${OTHER_SECRET}`. The sops provider should pass
   through composition transparently (the framework handles it). Confirm
   during Phase 1.
