# Service Wiring — How Secrets Arrive at Runtime

> Reference: `secretspec.toml` is the *declaration* of intent ("this
> secret must exist"); this document is the *delivery* contract
> ("and here's how it actually reaches the consuming service").
> Phase 1 today runs entirely on `/etc/nixos` + host-local `secretspec
> check`; Phase 2+ adds `sops://` resolution from age-encrypted YAML
> under `secrets/`; Phase 3+ adds `onepassword://` and `vault://`
> routing via astral-key.

## Two injection styles

Every consuming service gets secrets through one of two patterns.
**Pick per service based on its lifecycle**: stateless / one-shot →
`secretspec run --`; long-running daemon → systemd-creds.

### Pattern A — `secretspec run --profile <name> -- <exec>`

Injects every resolved secret as `KEY=value` env vars, then execve()s the
command. Works on any host with `secretspec` ≥ v0.16 on PATH and a
`secretspec.toml` discoverable via `-f`.

```bash
secretspec run -f /etc/secretspec.toml --profile production -- \
  /usr/bin/grafana-server --config=/etc/grafana/grafana.ini
```

Use for: cloudflared, tailscaled, garage, act_runner, one-shot cron jobs,
DuckDNS updater, ACME renewals.

### Pattern B — systemd `LoadCredential=(Encrypted)`

`secretspec get` resolves each key to bytes → hands to
`systemd-creds encrypt -` → writes an encrypted blob to
`/etc/credstore/` → service unit's `LoadCredentialEncrypted=` references
it → service reads plaintext from `$CREDENTIALS_DIRECTORY/<key>` at
runtime. The host-side decrypt key lives in the TPM (or sealed on
disk-bound to PCR state). This is the canonical community pattern from
cachix/secretspec#65.

```nix
systemd.services.grafana = {
  serviceConfig.LoadCredentialEncrypted = [
    "grafana_admin_password:/etc/credstore/grafana-admin_password.cred"
    "grafana_oauth_client_secret:/etc/credstore/grafana-oauth_client_secret.cred"
  ];
  # Grafana specific: secret is in /etc/grafana/grafana.ini admin section
  serviceConfig.ExecStartPre = [
    "${pkgs.bash}/bin/bash -c 'admin_password=$(cat $CREDENTIALS_DIRECTORY/grafana_admin_password); printf \"admin_password = %s\\n\" \"$admin_password\" >> /etc/grafana/grafana-secrets.ini'"
  ];
};
```

Use for: long-running daemons that already have systemd unit config
(Grafana, Prometheus, Loki, K3s, Vaultwarden, mail server, Jellyfin,
Paperless, PhotoPrism, Mealie).

---

## Per-service map

| Service | Manifest key(s) | Pattern | Phase 2 SOPS target | Phase 3 final home |
|---------|-----------------|---------|-------------------|-------------------|
| **Grafana** | `GRAFANA_ADMIN_PASSWORD` + `GRAFANA_OAUTH_CLIENT_SECRET` | B (load + ini snippet) | `secrets.monitoring.yaml#grafana_admin_password` | `vault://…?auth=approle` (astral-key) |
| **Prometheus** | `PROMETHEUS_REMOTE_WRITE_TOKEN` | B | `secrets.monitoring.yaml#prometheus_remote_write_token` | `vault://…?auth=approle` |
| **Loki** | `LOKI_INGEST_TOKEN` | B | `secrets.monitoring.yaml#loki_ingest_token` | `vault://…?auth=approle` |
| **K3s server** | `K3S_CLUSTER_TOKEN` + `KUBECONFIG_ADMIN` + `K3S_KUBELET_TOKEN` | B | `secrets.kubernetes.yaml#k3s_cluster_token` etc. | `vault://…?auth=approle` |
| **Kubernetes dashboard** | `KUBEADMIN_PASSWORD` | B | `secrets.kubernetes.yaml#kubeadmin_password` | `vault://…?auth=approle` |
| **Vaultwarden** | `VAULTWARDEN_ADMIN_TOKEN` + `VAULTWARDEN_SMTP_PASSWORD` | B | `secrets.selfhosting.yaml#vaultwarden_admin_token` etc. | `vault://…?auth=approle` |
| **Mail server** | `MAIL_SERVER_PASSWORD` | B | `secrets.selfhosting.yaml#mail_server_password` | `vault://…?auth=approle` |
| **n8n (API)** | `N8N_API_KEY` | A or B | `secrets.automation.yaml#n8n_api_key` | `onepassword://Homelab/Automation` |
| **n8n (webhook)** | `N8N_WEBHOOK_SECRET` (hex HMAC) | A or B | `secrets.automation.yaml#n8n_webhook_secret` | `onepassword://Homelab/Automation` |
| **Paperless-NGX** | `PAPERLESS_API_TOKEN` | B | `secrets.selfhosting.yaml#paperless_api_token` | `onepassword://Homelab/SelfHosting` |
| **PhotoPrism** | `PHOTOPRISM_ADMIN_PASSWORD` | B | `secrets.selfhosting.yaml#photoprism_admin_password` | `onepassword://Homelab/SelfHosting` |
| **Mealie** | `MEALIE_API_KEY` | B | `secrets.selfhosting.yaml#mealie_api_key` | `onepassword://Homelab/SelfHosting` |
| **Jellyfin** | `JELLYFIN_API_KEY` | B | `secrets.selfhosting.yaml#jellyfin_api_key` | `onepassword://Homelab/SelfHosting` |
| **UptimeRobot poller** | `UPTIME_ROBOT_API_KEY` | A | `secrets.monitoring.yaml#uptime_robot_api_key` | `onepassword://Homelab/Monitoring` |
| **Cloudflared** | `CLOUDFLARE_TUNNEL_TOKEN` | A | `secrets.cloud.yaml#cloudflare_tunnel_token` | `onepassword://Homelab/Cloud` |
| **Tailscale** | `TAILSCALE_AUTH_KEY` | A | `secrets.cloud.yaml#tailscale_auth_key` | `onepassword://Homelab/Cloud` |
| **Garage S3** | `GARAGE_ACCESS_KEY` + `GARAGE_SECRET_KEY` | A | `secrets.storage.yaml#garage_access_key` etc. | `onepassword://Homelab/Storage` |
| **act_runner** | `HOMELAB_RUNNER_REG_TOKEN` | A | `secrets.automation.yaml#homelab_runner_reg_token` | `env://` (CI runtime only) |
| **DuckDNS updater** | `HOMELAB_DUCKDNS_TOKEN` | A (cron) | `secrets.cloud.yaml#homelab_duckdns_token` | `keyring://` (per-host) |
| **ACME / certbot** | `ACME_ACCOUNT_KEY` (EAB) | A | `secrets.cloud.yaml#acme_account_key` | `onepassword://Homelab/Cloud` |
| **Ansible Vault** | `ANSIBLE_VAULT_PASSWORD` | A (per `ansible-playbook` invocation) | `secrets.automation.yaml#ansible_vault_password` | `onepassword://Homelab/Automation` |
| **Restic backup** | `RESTIC_REPO_PASSWORD` + `BACKUP_ENCRYPTION_KEY` | A | `secrets.storage.yaml#restic_repo_password` etc. | `onepassword://Homelab/Storage` ⚠ never `keyring://` |
| **rclone** | `RCLONE_CONFIG_PASS` | A | `secrets.storage.yaml#rclone_config_passphrase` | `onepassword://Homelab/Storage` |
| **Mining pool** | `MINING_POOL_USER` + `MINING_POOL_PASS` | A | `secrets.mining.yaml#mining_pool_user` | `onepassword://Homelab/Mining` |
| **Mining wallet cold-storage** | `ETHEREUM_WALLET_KEY` + `MONERO_WALLET_KEY` | A (cold-import only) | `secrets.mining.yaml#ethereum_wallet_key` etc. | `sops://` cold-storage ⚠ never `keyring://` |
| **Local WiFi** | `NETWORK_WIFI_PASSWORD` | A (per-host NetworkManager dispatcher) | `secrets.selfhosting.yaml#network_wifi_password` | `keyring://` (per-host, low risk) |

---

## Caller-side / dashboard-managed keys

Several declared secrets are consumed **outside** the systemd/homelab
service boundary — by browser-dashboard admin sessions, by ad-hoc
developer scripts, by CI runners, or are PUBLIC values that exist only
for route parity. These appear in `secretspec.toml` and have
`secrets.<cat>.yaml` target files, but they do **not** get a per-service
`LoadCredentialEncrypted=` wiring. Listed here for completeness so
operators know where to find them.

| Manifest key(s) | Consumer class | Notes |
|-----------------|----------------|-------|
| `CLOUDFLARE_API_KEY` + `CLOUDFLARE_EMAIL` + `CLOUDFLARE_ZONE_ID` | Cloudflare dashboard (`dash.cloudflare.com`) + terraform / cloudflare-go scripts | These three are the DNS-edit auth tuple. Manual rotation via the dashboard; script-side consumers usually re-source from a TF vars file pinning to `sops://secrets.cloud.yaml#*`. Phase 3 final home: `onepassword://Homelab/Cloud`. |
| **AI keys** (`NVIDIA_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `OPENAI_ORG_ID` / `HUGGINGFACE_TOKEN` / `STABILITY_API_KEY` / `REPLICATE_API_TOKEN`) | ad-hoc dev scripts, LLM-backed tooling, external benchmarks | Sourced via `secretspec run --profile development -- <script>` (dev profile has `default = "*replace-me*"` placeholders for the first three) OR `.envrc.local` + direnv in the dev shell. |
| **CI keys** (`GITHUB_TOKEN` / `GITHUB_RUNNER_PAT` / `NIX_PACKAGES_CACHE_TOKEN`) | CI pipeline runtime (GHA / act_runner / cachix runners) | Injected via `secretspec run --profile ci` in the runner's environment; never stored long-term. Phase 3 final home: `env://` (CI runtime only). |
| **Wallet PUBLIC addresses** (`ETHEREUM_WALLET_ADDRESS` / `MONERO_WALLET_ADDRESS`) | n/a — PUBLIC values | Mirror the private-key rows for `sops://` route parity only; no security boundary. Operators may publish these freely (e.g., for mining-pool payouts); they are not secrets. |

## Phase-by-phase rollout

### Phase 1 (today)

All services run with secrets supplied via `env://` + `dotenv://`.
`scripts/phase4-deploy-example.sh` covers the three dominant Patterns
(`inject`, `push-creds`, `push-k8s`). The unit wiring in each NixOS
host's `configuration.nix` is the missing piece — operators add
`LoadCredentialEncrypted=` per service as documented above.

### Phase 2 (when `secretspec-provider-sops` v0.1.0 tags)

Each NixOS host's `configuration.nix` overrides one service at a
time: replace the per-secret `env://FOO` resolution with
`sops://secrets.<cat>.yaml#foo` in `secretspec.toml` and add
`age_key = "vault://http://astral-key:8080/v1/secret/data/age_key?auth=approle"`
to the `credentials` block. Verify each migration with
`secretspec check -f /etc/secretspec.toml --profile default && --profile development && --profile production` (all must exit 0).

### Phase 3 (final provider routing)

Uncomment `[providers.onepassword]` + `[providers.astral_vault]` in
`secretspec.toml`'s `[providers]` block, then migrate per-secret
resolution chains per the matrix right-column:
- AI → onepassword://Homelab/AI
- Cloud → onepassword://Homelab/Cloud (DuckDNS → keyring:// per-host)
- Storage → onepassword://Homelab/Storage (wallet keys NEVER keyring)
- K8s + monitoring-with-OAuth + Vaultwarden-SMTP → vault:// via astral-key
- SelfHosting → mix (vaultwarden/mail → vault, media stack → onepassword, WiFi → keyring)

---

## Conventions

- **`SECRETSPEC_SKIP_CHECK=1`** — skip the `secretspec check` regression
  guard in `scripts/bootstrap-dev.sh` for unedited placeholder values.
- **Never store wallet keys in `keyring://`.** sops-provider-design.md ↗
  "Wallet key blocklist" is canonical. `ETHEREUM_WALLET_KEY`,
  `MONERO_WALLET_KEY`, `BACKUP_ENCRYPTION_KEY` are the three primaries.
- **`secretspec run -f` failure modes** — `value: null` (not
  error envelope) when a key is missing per protocol-v1 §5.1, so
  consumers see a clean "absent" signal; audit log target
  `secretspec_provider_sops::audit` carries the real cause for
  triage.
- **Phase 3 routing migrations** must be one secret at a time, with
  `secretspec check` reverted to exit 0 between each. Chain order
  in `providers = [...]` carries the rollback story: keep the
  Phase 2 entry last so reverting is a comment-out, not a parser
  rewrite.
