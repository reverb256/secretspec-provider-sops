# Migration Matrix — sops-nix → SecretSpec

This matrix maps every secret in the homelab's `sops-secrets-registry.nix`
through the four phases of the migration plan in `CONTEXT.md` →
"Migration Path". Each row binds a secret name to a *current* Phase 1
provider, a *Phase 2* `sops://` target file (where the age-encrypted YAML
lives), and a *Phase 3* final-provider target — the home the secret lands
in once all upstream blockers clear.

Entries marked **🚧** would be pending declaration in `secretspec.toml`,
except the file now ships the full 49 rows (`secretspec.toml` close-of-scope
2026-07-26 — every row in this matrix is declared, dev profile respects
the AI-only placeholder convention). Entries marked **✅** are declared
and resolve through the Phase 1 provider chain.

## Per-secret matrix

| Category | Secret | Phase 1 provider | Phase 2 sops:// target | Phase 3 final provider | Status |
|----------|--------|------------------|------------------------|------------------------|--------|
| **aiServices** | `NVIDIA_API_KEY` | `env://NVIDIA_API_KEY` | `secrets.ai.yaml#nvidia_api_key` | `onepassword://Homelab/AI` | ✅ declared, dev default set |
| | `OPENAI_API_KEY` | `env://OPENAI_API_KEY` | `secrets.ai.yaml#openai_api_key` | `onepassword://Homelab/AI` | ✅ declared, dev default set |
| | `ANTHROPIC_API_KEY` | `env://ANTHROPIC_API_KEY` | `secrets.ai.yaml#anthropic_api_key` | `onepassword://Homelab/AI` | ✅ declared, dev default set |
| | `OPENAI_ORG_ID` | `env://OPENAI_ORG_ID` | `secrets.ai.yaml#openai_org_id` | `onepassword://Homelab/AI` | ✅ declared |
| | `HUGGINGFACE_TOKEN` | `env://HUGGINGFACE_TOKEN` | `secrets.ai.yaml#huggingface_token` | `onepassword://Homelab/AI` | ✅ declared |
| | `STABILITY_API_KEY` | `env://STABILITY_API_KEY` | `secrets.ai.yaml#stability_api_key` | `onepassword://Homelab/AI` | ✅ declared |
| | `REPLICATE_API_TOKEN` | `env://REPLICATE_API_TOKEN` | `secrets.ai.yaml#replicate_api_token` | `onepassword://Homelab/AI` | ✅ declared |
| **ci** | `GITHUB_TOKEN` | `env://GITHUB_TOKEN` | `secrets.ci.yaml#github_token` | `env://` (CI runtime only) | ✅ declared |
| | `GITHUB_RUNNER_PAT` | `env://GITHUB_RUNNER_PAT` | `secrets.ci.yaml#github_runner_pat` | `env://` (CI runtime only) | ✅ declared |
| | `NIX_PACKAGES_CACHE_TOKEN` | `env://NIX_PACKAGES_CACHE_TOKEN` | `secrets.ci.yaml#nix_packages_cache_token` | `env://` | ✅ declared |
| **cloud** | `CLOUDFLARE_TUNNEL_TOKEN` | `env://` or `dotenv://` | `secrets.cloud.yaml#cloudflare_tunnel_token` | `onepassword://Homelab/Cloud` | ✅ declared |
| | `CLOUDFLARE_API_KEY` | `env://` | `secrets.cloud.yaml#cloudflare_api_key` | `onepassword://Homelab/Cloud` | ✅ declared |
| | `CLOUDFLARE_EMAIL` | `env://` | `secrets.cloud.yaml#cloudflare_email` | `onepassword://Homelab/Cloud` | ✅ declared |
| | `CLOUDFLARE_ZONE_ID` | `env://` | `secrets.cloud.yaml#cloudflare_zone_id` | `onepassword://Homelab/Cloud` | ✅ declared |
| | `TAILSCALE_AUTH_KEY` | `env://` | `secrets.cloud.yaml#tailscale_auth_key` | `onepassword://Homelab/Cloud` | ✅ declared |
| | `HOMELAB_DUCKDNS_TOKEN` | `env://` | `secrets.cloud.yaml#duckdns_token` | `keyring://` (per-host) | ✅ declared |
| | `ACME_ACCOUNT_KEY` | `env://` | `secrets.cloud.yaml#acme_account_key` | `onepassword://Homelab/Cloud` | ✅ declared |
| **storage** | `GARAGE_ACCESS_KEY` | `env://` | `secrets.storage.yaml#garage_access_key` | `onepassword://Homelab/Storage` | ✅ declared |
| | `GARAGE_SECRET_KEY` | `env://` | `secrets.storage.yaml#garage_secret_key` | `onepassword://Homelab/Storage` | ✅ declared |
| | `BACKUP_ENCRYPTION_KEY` | `env://` | `secrets.storage.yaml#backup_encryption_key` | `onepassword://Homelab/Storage` | ✅ declared (age-key never in `keyring://`) |
| | `RESTIC_REPO_PASSWORD` | `env://` | `secrets.storage.yaml#restic_repo_password` | `onepassword://Homelab/Storage` | ✅ declared |
| | `RCLONE_CONFIG_PASS` | `env://` | `secrets.storage.yaml#rclone_config_passphrase` | `onepassword://Homelab/Storage` | ✅ declared |
| **kubernetes** | `K3S_CLUSTER_TOKEN` | `env://` | `secrets.kubernetes.yaml#k3s_cluster_token` | `vault://…/v1/secret/data/k3s?auth=approle` (via astral-key) | ✅ declared |
| | `KUBECONFIG_ADMIN` | `dotenv://` (base64 in env) | `secrets.kubernetes.yaml#kubeconfig_admin` (raw file via `as_path=true`) | `vault://…/v1/secret/data/kubeconfig?auth=approle` | ✅ declared (base64 now; `as_path` migration in Phase 3) |
| | `KUBEADMIN_PASSWORD` | `env://` | `secrets.kubernetes.yaml#kubeadmin_password` | `vault://` via astral-key | ✅ declared |
| | `K3S_KUBELET_TOKEN` | `env://` | `secrets.kubernetes.yaml#k3s_kubelet_token` | `vault://` via astral-key | ✅ declared |
| **mining** | `ETHEREUM_WALLET_KEY` | `env://` | `secrets.mining.yaml#ethereum_wallet_key` | `onepassword://Homelab/Mining` **OR** `sops://` cold-storage | ✅ declared (deliberately NOT in `keyring://` per wallet-key blocklist) |
| | `MONERO_WALLET_KEY` | `env://` | `secrets.mining.yaml#monero_wallet_key` | `onepassword://Homelab/Mining` **OR** `sops://` cold-storage | ✅ declared (NOT in `keyring://`) |
| | `MINING_POOL_USER` | `env://` | `secrets.mining.yaml#mining_pool_user` | `onepassword://Homelab/Mining` | ✅ declared |
| | `MINING_POOL_PASS` | `env://` | `secrets.mining.yaml#mining_pool_pass` | `onepassword://Homelab/Mining` | ✅ declared |
| | `ETHEREUM_WALLET_ADDRESS` | `env://` | `secrets.mining.yaml#ethereum_wallet_address` | `onepassword://Homelab/Mining` | ✅ declared (PUBLIC — mirrored for sops:// route parity) |
| | `MONERO_WALLET_ADDRESS` | `env://` | `secrets.mining.yaml#monero_wallet_address` | `onepassword://Homelab/Mining` | ✅ declared (PUBLIC — mirrored for sops:// route parity) |
| **monitoring** | `GRAFANA_ADMIN_PASSWORD` | `env://` | `secrets.monitoring.yaml#grafana_admin_password` | `vault://` via astral-key | ✅ declared |
| | `PROMETHEUS_REMOTE_WRITE_TOKEN` | `env://` | `secrets.monitoring.yaml#prometheus_remote_write_token` | `vault://` via astral-key | ✅ declared |
| | `LOKI_INGEST_TOKEN` | `env://` | `secrets.monitoring.yaml#loki_ingest_token` | `vault://` via astral-key | ✅ declared |
| | `UPTIME_ROBOT_API_KEY` | `env://` | `secrets.monitoring.yaml#uptime_robot_api_key` | `onepassword://Homelab/Monitoring` | ✅ declared |
| | `GRAFANA_OAUTH_CLIENT_SECRET` | `env://` | `secrets.monitoring.yaml#grafana_oauth_client_secret` | `vault://` via astral-key | ✅ declared |
| **automation** | `N8N_API_KEY` | `env://` | `secrets.automation.yaml#n8n_api_key` | `onepassword://Homelab/Automation` | ✅ declared |
| | `N8N_WEBHOOK_SECRET` | `env://` | `secrets.automation.yaml#n8n_webhook_secret` | `onepassword://Homelab/Automation` | ✅ declared (type=`hex`) |
| | `ANSIBLE_VAULT_PASSWORD` | `env://` | `secrets.automation.yaml#ansible_vault_password` | `onepassword://Homelab/Automation` | ✅ declared |
| | `HOMELAB_RUNNER_REG_TOKEN` | `env://` | `secrets.automation.yaml#act_runner_reg_token` | `env://` | ✅ declared |
| **selfHosting** | `VAULTWARDEN_ADMIN_TOKEN` | `env://` | `secrets.selfhosting.yaml#vaultwarden_admin_token` | `vault://` via astral-key (Vaultwarden IS the backend) | ✅ declared |
| | `MAIL_SERVER_PASSWORD` | `env://` | `secrets.selfhosting.yaml#mail_server_password` | `vault://` via astral-key | ✅ declared |
| | `JELLYFIN_API_KEY` | `env://` | `secrets.selfhosting.yaml#jellyfin_api_key` | `onepassword://Homelab/SelfHosting` | ✅ declared |
| | `PAPERLESS_API_TOKEN` | `env://` | `secrets.selfhosting.yaml#paperless_api_token` | `onepassword://Homelab/SelfHosting` | ✅ declared |
| | `PHOTOPRISM_ADMIN_PASSWORD` | `env://` | `secrets.selfhosting.yaml#photoprism_admin_password` | `onepassword://Homelab/SelfHosting` | ✅ declared |
| | `MEALIE_API_KEY` | `env://` | `secrets.selfhosting.yaml#mealie_api_key` | `onepassword://Homelab/SelfHosting` | ✅ declared |
| | `VAULTWARDEN_SMTP_PASSWORD` | `env://` | `secrets.selfhosting.yaml#vaultwarden_smtp_password` | `vault://` via astral-key | ✅ declared |
| | `NETWORK_WIFI_PASSWORD` | `env://` | `secrets.selfhosting.yaml#network_wifi_password` | `keyring://` (per-host, low risk) | ✅ declared |

**Totals:** 49 entries, all 49 declared in `secretspec.toml` (✅), 0
pending declaration (🚧). The 3 development-profile rows for AI API keys
(`NVIDIA_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`) reuse the
default-profile names — they're overrides, not new secrets.

By-category ✅ count: aiServices 7, ci 3, cloud 7, storage 5,
kubernetes 4, mining 6, monitoring 5, automation 4, selfHosting 8
(= 49). By-category 🚧 count: 0.

## Why per-category sops files (decision)

Five plausible mappings for the Phase 2 SOPS target file structure:

1. **One sops file per category** ← chosen. Maps 1-to-1 to the inventory
   categories in `CONTEXT.md` → "Migration Path". Worst-case file size
   ~6 secrets × multiple hosts — small, reviewable diffs, easy to scope
   PRs during the upstream-provider acceptance phase.
2. **One single sops file (`secrets.yaml`)** — friction-free to consult,
   but every host rotation touches every secret. Couples unrelated
   access patterns.
3. **One file per environment (dev / staging / prod)** — maps to
   SecretSpec profiles, but doubles the audit surface; multiplies the
   per-host rotation count.
4. **One file per host** — most granular, but the `sops-nix` pattern
   already creates hundreds of trivial host files; nothing useful
   changes here.
5. **One file per *credential concern* (workload identity)** — finer
   than (1); useful if the homelab grows past ~10 categories. Premature
   here.

**Selected: (1).** Numbered YAML keys per secret (e.g.,
`nvidia_api_key:`); the `sops://` URI in `secretspec.toml` becomes
`sops://secrets.ai.yaml#nvidia_api_key` once PR #58 ships and our crate
exposes `#<key>` as a valid query.

## Wallet-key blocklist (per `sops-provider-design.md`)

`ETHEREUM_WALLET_KEY`, `MONERO_WALLET_KEY`, and `BACKUP_ENCRYPTION_KEY`
deliberately never resolve from `keyring://` — the OS keyring is
exfiltratable by local malware. The first two are wallet private keys;
the third is a *master age encryption key* that, if leaked, would
compromise the entire fleet's backups. All three Phase 3 final
providers are `onepassword://` for daily access or `sops://`
cold-storage; in either case the encryption layer is required.

(sops-provider-design.md’s “Wallet key blocklist” section uses the
broader “wallet keys (ETH/XMR) or master age encryption keys”
formulation — confirmed in secretspec.toml’s Storage section header
comment.)

## Phase 2 trigger

Phase 2 becomes actionable the moment one of the following lands:

- **A.** cachix/secretspec PR #58 merges upstream (currently OPEN, DRAFT,
  author `euphemism` unresponsive to Domen's Jul 17 rework request — see
  `CONTEXT.md` → "SOPS Provider (PR #58)" timeline).
- **B.** Our `secretspec-provider-sops` crate (`CONTEXT.md` → "Build Intent")
  ships a working CLI shim + `auth` block, per
  `sops-provider-design.md`. This is the higher-confidence path because
  PR #58's rework is non-trivial — Domen's "provider credentials" ask
  requires a `credentials` chain (now confirmed-shape in v0.15+).

Until either (A) or (B) lands, the Phase 2 column above is forward-looking,
not actionable. Phase 1 (declared in `secretspec.toml`) is sufficient today.

## Cross-references

- `CONTEXT.md` → "Migration Path" (4-phase plan + inventory)
- `CONTEXT.md` → "astral-key Integration" (the Apollo for Phase 3 final providers)
- `sops-provider-design.md` → "Why build our own" (why `B` is more
  likely than `A` in the homelab's case)
- `secretspec.toml` (the full 49 declared entries)
