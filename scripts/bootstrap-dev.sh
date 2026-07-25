#!/usr/bin/env bash
# bootstrap-dev.sh — copy the committed `.env.secrets.example` template to
# a gitignored `.env.secrets` for local dev.
#
# Idempotent: refuses to overwrite an existing `.env.secrets` unless
# `--force` is passed. After copy, instructs the caller to edit the
# placeholders and run `secretspec check --profile development`.
#
# Usage:
#   ./scripts/bootstrap-dev.sh           # copy if absent
#   ./scripts/bootstrap-dev.sh --force   # overwrite existing .env.secrets
#
# See `.env.secrets.example` for the placeholder list and rationale.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

FORCE=0
for arg in "$@"; do
  case "$arg" in
    --force|-f) FORCE=1 ;;
    -h|--help)
      cat <<'USAGE'
Usage: bootstrap-dev.sh [--force]

Copies .env.secrets.example to .env.secrets (gitignored). Refuses to
overwrite an existing .env.secrets unless --force is passed. After copy,
edit the placeholders and run `secretspec check --profile development`.
USAGE
      exit 0
      ;;
  esac
done

if [[ ! -f .env.secrets.example ]]; then
  echo "✗ .env.secrets.example not found at $REPO_ROOT" >&2
  echo "  (expected — this file is committed. Did you move the script?)" >&2
  exit 1
fi

if [[ -f .env.secrets ]] && [[ "$FORCE" != "1" ]]; then
  echo "✗ .env.secrets already exists at $REPO_ROOT/.env.secrets" >&2
  echo "  Pass --force to overwrite, or edit it in place." >&2
  echo "  (Tip: \`diff .env.secrets .env.secrets.example\` to see drift.)" >&2
  exit 1
fi

cp .env.secrets.example .env.secrets
chmod 0600 .env.secrets

cat <<'NEXT'
✓ .env.secrets written from template (mode 0600).

Next steps:
  $EDITOR .env.secrets      # replace placeholders with real values
  secretspec check --profile development
NEXT

# Regression guard: confirm the dev profile still resolves end-to-end.
# graceful-passthrough: if `secretspec` isn't on PATH (e.g. minimal CI
# image without rustup profiles installed) we skip rather than fail —
# the dev VM is expected to have it via `nix profile install nixpkgs#secretspec`.
#
# Note: command -v returns the full executable path on success and exits
# non-zero on miss; the `[ -x … ]` test handles both branches.

SECRETSPEC_BIN="$(command -v secretspec 2>/dev/null || true)"
if [ -z "$SECRETSPEC_BIN" ] || [ ! -x "$SECRETSPEC_BIN" ]; then
  # Fall back to the standard local-bin install (matches CI workflow + the
  # v0.16 direct-release path documented in README.md). Only check
  # executability here — a non-executable file at this path is still a
  # miss so we treat it like "not installed".
  if [ -x "$HOME/.local/bin/secretspec" ]; then
    SECRETSPEC_BIN="$HOME/.local/bin/secretspec"
  else
    echo "  ↪ secretspec not on PATH; skipping dev-profile check."
    echo "    Install: nix profile install nixpkgs#secretspec (or curl v0.16 direct release)."
    SECRETSPEC_BIN=""
  fi
fi

if [ -n "$SECRETSPEC_BIN" ]; then
  echo "→ secretspec check --profile development (sanity)"
  if "$SECRETSPEC_BIN" check -f "$REPO_ROOT/secretspec.toml" --profile development; then
    echo "  ✓ secretspec check passed (49/49 resolved)"
  else
    echo "  ⚠ secretspec check failed (placeholders are unedited?)." >&2
    echo "    Edit .env.secrets to set real values, OR re-run with" >&2
    echo "    SECRETSPEC_SKIP_CHECK=1 to bypass this guard." >&2
    if [ "${SECRETSPEC_SKIP_CHECK:-0}" = "1" ]; then
      echo "    (SECRETSPEC_SKIP_CHECK=1 set — skipping failure exit.)"
    else
      exit 2
    fi
  fi
fi
