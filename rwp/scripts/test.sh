#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi
export DATABASE_URL="${DATABASE_URL:-${TO_DO_DB_URL:-postgres://username:password@127.0.0.1:5432/to_do}}"

cargo test --workspace

cd "$ROOT/frontend"
bun test
bun run typecheck
bun run build

if [[ "${RUN_POSTGRES_INTEGRATION:-0}" == "1" ]]; then
  "$ROOT/scripts/basic_test.sh"
fi
