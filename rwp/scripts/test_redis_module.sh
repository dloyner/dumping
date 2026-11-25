#!/usr/bin/env bash

set -euo pipefail

R="docker exec ${CONTAINER:-cache-ci} redis-cli -a ${REDIS_PW:-ci-password} --no-auth-warning"
eq() { [[ "$1" == "$2" ]] || {
  echo "FAIL: got '$1', want '$2'" >&2
  exit 1
}; }

eq "$($R session.update nobody)" NOT_FOUND
eq "$($R session.login u1 20 42)" OK
eq "$($R session.update u1)" 42
for _ in $(seq 1 19); do $R session.update u1 >/dev/null; done
eq "$($R session.update u1)" REFRESH
$R session.login u1 20 42 >/dev/null
eq "$($R session.update u1)" 42
eq "$($R session.logout u1)" OK
eq "$($R session.update u1)" NOT_FOUND
eq "$($R session.logout u1)" NOT_FOUND
$R session.login u2 1 7 >/dev/null
ttl="$($R pttl user_session_u2)"
((ttl > 0)) || {
  echo "FAIL: session has no TTL" >&2
  exit 1
}
echo "redis module OK"
