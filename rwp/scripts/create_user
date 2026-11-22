#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <email>" >&2
  exit 2
fi
EMAIL="$1"
read -r -s -p "Password: " PASSWORD
printf '\n'

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
if [[ -f "$ROOT/.env" ]]; then
  set -a
  # .env is maintained as shell-compatible KEY=value configuration.
  # shellcheck disable=SC1091
  source "$ROOT/.env"
  set +a
fi

if ! command -v xh >/dev/null 2>&1; then
  echo "create_user requires xh to send the JSON request" >&2
  exit 1
fi

BASE_URL="${INGRESS_URL:-http://127.0.0.1:${INGRESS_WEB_PORT:-8001}}"
BASE_URL="${BASE_URL%/}"
if RESPONSE="$(xh --ignore-stdin --pretty=none --print=hb \
  POST "$BASE_URL/api/v1/users/create" email="$EMAIL" password="$PASSWORD")"; then
  STATUS="$(awk 'NR == 1 { print $2 }' <<<"$RESPONSE")"
else
  STATUS="$(awk 'NR == 1 { print $2 }' <<<"$RESPONSE")"
  echo "User creation request failed${STATUS:+ (HTTP $STATUS)}:" >&2
  printf '%s\n' "$RESPONSE" >&2
  exit 1
fi

if [[ "$STATUS" != "201" ]]; then
  echo "User creation expected HTTP 201, got ${STATUS:-no status}:" >&2
  printf '%s\n' "$RESPONSE" >&2
  exit 1
fi

unset PASSWORD
echo "Created user $EMAIL"
