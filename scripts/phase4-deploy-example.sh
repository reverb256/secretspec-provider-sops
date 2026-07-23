#!/usr/bin/env bash
# phase4-deploy-example.sh — three Phase 4 deployment patterns that work
# **without** cachix/secretspec#65 (NixOS module) OR #41 (systemd-creds
# provider) upstream. Context: see CONTEXT.md → "NixOS Integration Status"
# → "What the community actually does"; astral-key's Vault KV v2 endpoint
# (astral-key-endpoint-spec.md) provides the `vault://` provider a backend
# to talk to for the homelab.
#
# These three patterns cover the dominant Phase 4 use cases from the issue
# #65 thread:
#
#   inject <cmd>            secretspec run -- <cmd>
#                           Service gets all declared secrets as uppercase
#                           env vars. No host-side install needed.
#
#   push-creds <host> <svc> <keys…>
#                           Piped-resolve each secret, ssh it to the target
#                           NixOS host, encrypt via systemd-creds at rest in
#                           /etc/credstore, advertise via NixOS service
#                           LoadCredentialEncrypted=.
#
#   push-k8s <ns> <name> <keys…>
#                           Pipe-resolved secrets into `kubectl create secret
#                           generic` so K8s services consume them.
#
# Required on the workstation:
#   ~/.local/bin/secretspec ≥ v0.16.0 (direct release from cachix/secretspec)
#   ssh, jq (for the k8s variant)
#
# Required on the target NixOS host (only for push-creds):
#   systemd ≥ 256 (so systemd-creds encrypt is present)
#
# Usage:
#   scripts/phase4-deploy-example.sh inject 'env | grep CLOUDFLARE_'
#   scripts/phase4-deploy-example.sh push-creds homelab-host cloudflared CLOUDFLARE_TUNNEL_TOKEN
#   scripts/phase4-deploy-example.sh push-k8s  prod k3s-cluster-tokens K3S_CLUSTER_TOKEN
#
# Exit codes: 0 success, non-zero on resolution / ssh / kubectl failure.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SECRETSPEC="${SECRETSPEC:-$HOME/.local/bin/secretspec}"

# Resolve one declared key. `secretspec get` respects -f/-p/-provider, so
# callers can override defaults via env vars if they normally use devenv.
resolve_secret() {
  local key="$1"
  "$SECRETSPEC" get "$key" -f "$REPO_ROOT/secretspec.toml" ${SECRETSPEC_PROFILE:+--profile "$SECRETSPEC_PROFILE"}
}

# --- Pattern 1: in-host injection -----------------------------------------
# The simplest path. Works on any host that has SecretSpec installed and a
# secretspec.toml somewhere SecretSpec can find. No NixOS module needed.
#
# The user's <cmd> is wrapped in `sh -c "..."` so shell metacharacters
# (`|`, `>`, `*`, env-var expansions, etc.) are interpreted normally;
# without the wrapper, secretspec's `--` argv passthrough would treat
# `|` as a literal arg (env would error on "no such file or directory").
deploy_inject_service() {
  local cmd="$1"
  echo "→ secretspec run -- sh -c \"${cmd}\""
  exec "$SECRETSPEC" run \
    -f "$REPO_ROOT/secretspec.toml" \
    ${SECRETSPEC_PROFILE:+--profile "$SECRETSPEC_PROFILE"} \
    -- sh -c "$cmd"
}

# --- Pattern 2: SSH-piped systemd-creds encrypt ---------------------------
# Canonical community workaround from issue #65 thread. Resolves each
# secret locally → stdin over ssh → `systemd-creds encrypt` writes to
# /etc/credstore on the host → NixOS service config maps via
# LoadCredentialEncrypted=. The NixOS module (cachix/secretspec#65) is
# *additive* — once it ships, the same pattern works with the module
# providing the LoadCredentialEncrypted= wiring declaratively.
deploy_systemd_creds_push() {
  local host="$1"; shift
  local service_name="$1"; shift
  local secret_keys=("$@")

  if (( ${#secret_keys[@]} == 0 )); then
    echo "✗ push-creds: need at least one secret key" >&2
    return 64
  fi

  echo "→ pushing ${#secret_keys[@]} secret(s) to ${host} for service ${service_name}"
  # shellcheck disable=SC2029  # ${service_name}-${key} are LOCAL shell vars
  # that we intentionally expand on this side before ssh sends the
  # command. The remote side only sees the rendered paths/names.
  for key in "${secret_keys[@]}"; do
    local value
    value="$(resolve_secret "$key")"
    if [[ -z "$value" ]]; then
      echo "✗ ${key} resolved to empty value (refusing push to avoid wiping a cred)" >&2
      return 65
    fi
    printf '%s' "$value" \
      | ssh "$host" "sudo mkdir -p /etc/credstore && \
                     sudo systemd-creds encrypt - \
                       /etc/credstore/${service_name}-${key}.cred \
                       --name ${service_name}-${key}"
    echo "  ✓ ${key} → /etc/credstore/${service_name}-${key}.cred"
  done
  echo
  cat <<EOF
On the target host, add to the NixOS service config:

  systemd.services.${service_name} = {
    serviceConfig.LoadCredentialEncrypted = [
$(for key in "${secret_keys[@]}"; do
    printf '      "%s:/etc/credstore/%s-%s.cred"\n' "$key" "$service_name" "$key"
  done)
    ];
  };

EOF
}

# --- Pattern 3: k8s secret push -------------------------------------------
# No SSH layer here — the deployment target IS the API host. Sealed in CI via
# GitHub Actions / GitLab CI / GitOps runner secrets. Resolved secrets are
# piped into `kubectl create secret generic` via --from-literal-file=-.
deploy_k8s_secret_push() {
  local namespace="$1"; shift
  local secret_name="$1"; shift
  local secret_keys=("$@")

  if (( ${#secret_keys[@]} == 0 )); then
    echo "✗ push-k8s: need at least one secret key" >&2
    return 64
  fi

  if ! command -v kubectl >/dev/null; then
    echo "✗ push-k8s: kubectl not on PATH (install or set KUBECONFIG)" >&2
    return 66
  fi

  echo "→ creating k8s secret ${namespace}/${secret_name}"
  # Materialize to literal args: kubectl create secret generic takes multiple
  # --from-literal=KEY=VALUE, each value resolved separately.
  local args=()
  for key in "${secret_keys[@]}"; do
    local value
    value="$(resolve_secret "$key")"
    if [[ -z "$value" ]]; then
      echo "✗ ${key} resolved to empty value (refusing push)" >&2
      return 65
    fi
    args+=("--from-literal=${key}=${value}")
  done
  kubectl create secret generic "$secret_name" \
    --namespace="$namespace" \
    "${args[@]}" \
    --dry-run=client -o yaml \
    | kubectl apply -f -
  echo "  ✓ ${secret_name} applied in ${namespace}"
}

# --- Dispatcher ------------------------------------------------------------
usage() {
  cat <<USAGE
Usage: $0 [pattern] [args...]

Patterns:
  inject <cmd>                              secretspec run -- <cmd>
  push-creds <host> <svc> <keys…>           SSH + systemd-creds + LoadCredentialEncrypted=
  push-k8s <ns> <name> <keys…>              piped to kubectl create secret generic

Examples:
  $0 inject 'env | grep CLOUDFLARE_'
  $0 push-creds homelab-host cloudflared CLOUDFLARE_TUNNEL_TOKEN CLOUDFLARE_API_KEY
  $0 push-k8s  prod k3s-cluster-tokens K3S_CLUSTER_TOKEN K3S_KUBELET_TOKEN

Env overrides:
  SECRETSPEC             path to the secretspec binary (default ~/.local/bin/secretspec)
  SECRETSPEC_PROFILE     profile to resolve against (default = user config)
USAGE
}

main() {
  if (( $# == 0 )); then usage; return 0; fi
  case "$1" in
    inject)        shift; deploy_inject_service "$@";;
    push-creds)    shift; deploy_systemd_creds_push "$@";;
    push-k8s)      shift; deploy_k8s_secret_push "$@";;
    -h|--help|help) usage;;
    *) echo "✗ unknown pattern: $1" >&2; usage >&2; return 1;;
  esac
}

main "$@"
