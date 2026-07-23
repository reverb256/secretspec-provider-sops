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
