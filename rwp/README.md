# Rust Web Programming

A multi-user to-do web application written in Rust, following Maxwell Flitton's
_Rust Web Programming_ (3rd Edition). It is deliberately built as a set of
"nanoservices" (auth, to-do, session cache) that can run **together in one
process** (monolith) or **as separate processes** (microservices).

- **Backend:** Axum, SQLx (PostgreSQL), a custom Redis module for sessions
- **Frontend:** React 18, bundled with esbuild and Bun, with a small Rust → WebAssembly helper
- **Deployment:** Docker Compose locally; AWS (ALB + ACM, EC2, RDS) in production

This file explains **what the project is and how to run and test it**. For a
code-level explanation of the architecture, design choices and limitations, see
[`engineering_handbook.md`](engineering_handbook.md). For building images and
operating the deployments (Docker, Docker Hub, AWS, HTTPS, Nginx), see
[`devops_handbook.md`](devops_handbook.md).

## Contents

1. [Architecture](#architecture)
2. [Repository layout](#repository-layout)
3. [How it works](#how-it-works)
4. [Run locally](#run-locally)
5. [Configuration reference](#configuration-reference)
6. [Test](#test)
7. [Deployment layouts at a glance](#deployment-layouts-at-a-glance)
8. [Known limitations](#known-limitations)

## Architecture

```text
 Browser
   | HTTP(S): login and to-do API requests (JWT in `Authorization: Bearer`)
   v
+-------------------------- ingress process ----------------------------+
|  Frontend assets       /api/v1/auth/*          /api/v1/{get,create,   |
|  (embedded files)       |                      update,delete}         |
|                         v                           v                 |
|                 Auth Axum router              To-do Axum router       |
+-------------------------|---------------------------|-----------------+
                          |                           |
                  auth-core layer             GetUserSession
                          |                    (auth-kernel)
                  auth-dal layer                      |
                          |                    cache-client
                          v                           |
                  Auth PostgreSQL DB          Redis `session.*` commands
                                                      |
                                             Redis user-session module
                                                      |
                              on refresh: auth-kernel reloads the user
                              and logs the refreshed session into Redis
                                                      |
                                               to-do-core layer
                                                      |
                                               to-do-dal layer
                                                      v
                                               To-do PostgreSQL DB
```

Ingress currently assembles the frontend, auth router, and to-do router into one
Axum process. The routers call their core and data-access layers in-process. In
the default `auth-core-postgres` build, the standalone to-do server opens the auth
database pool to resolve Redis sessions to numeric user IDs; in the
`auth-http` build, it resolves users through the auth HTTP service instead.
`to-do-axum-server` and `auth-axum-server` also contain standalone server entry
points (used by the microservice layout).

In production, traffic reaches ingress through an AWS Application Load Balancer
(TLS) and an Nginx container; see the
[deployment guide](devops_handbook.md#4-aws-production-deployment).

## Repository layout

| Path | What it is |
|---|---|
| `frontend/` | React UI (esbuild + Bun). `rust-interface/` is the WebAssembly crate that produces task action labels. |
| `ingress/` | The application entry point (port 8001). Embeds frontend assets, serves SPA routes, combines the auth and to-do routers, applies rate limiting, CSP and compression. |
| `nanoservices/auth/` | `kernel` (session orchestration), `core` (login/registration logic), `dal` (users table, migrations), `networking/axum_server` (HTTP layer + standalone binary). |
| `nanoservices/to_do/` | `core` (to-do logic), `dal` (items table, migrations), `networking/axum_server` (HTTP layer + standalone binary). |
| `nanoservices/user-session-cache/` | `cache-client` (Rust client) and `cache-module` (a Redis module implementing `session.login`, `session.update`, `session.logout`). |
| `glue/` | Shared JWT, error, database-pool, logging and session-constant utilities. |
| `compose.yaml` | Local PostgreSQL and Redis. |
| `Dockerfile` | One multi-architecture image (`compute-unit`) containing `ingress`, `auth-axum-server`, `to-do-axum-server`. |
| `builds/` | OpenTofu + scripts for the remote build server that builds and publishes images. |
| `deployment/` | OpenTofu + scripts + Compose file for the AWS production stack. |
| `scripts/` | Local helpers: `init.sh`, `cleanup.sh`, `clean_db.sh`, `create_user.sh`, `test.sh`, `basic_test.sh`, `test_redis_module.sh`, `gen_dev_cert.sh`, `deployment_cleanup.sh`. |

### Components and layers

- **Networking / Axum** (`nanoservices/*/networking/axum_server/`): extracts
  request data and JWTs, maps routes to handlers, formats HTTP responses.
- **Kernel / core** (`auth/kernel`, `auth/core`, `to_do/core`): coordinates
  authentication and session lookup, then applies service operations without
  handling HTTP routing or SQL details.
- **DAL** (`nanoservices/*/dal/`): owns database schemas, migrations and
  PostgreSQL queries. Auth and to-do data live in **separate databases**.
- **User-session cache**: `cache-client` sends session commands; `cache-module`
  implements them inside Redis. A to-do request resolves the JWT's unique ID to
  a cached numeric user ID.
- **Glue**: shared utilities used by every layer.

Layers talk through traits (for example `GetAll`, `SaveOne`, `GetUserSession`),
so the core and networking code is tested with in-memory fakes and the real
PostgreSQL implementation is swapped in only at the edge.

## How it works

### Request path

1. The browser sends a JWT with its API request to ingress. The token contains
   the user's `unique_id` and a per-login `session_id`.
2. The Axum handler verifies the JWT and asks `GetUserSession` to validate the
   same `session_id` against the Redis session for that `unique_id`.
3. `cache-client` calls the Redis module. On refresh, the auth kernel looks up
   the user through the configured auth backend and refreshes the cache entry
   without changing the active `session_id`.
4. The handler passes the user ID to `to-do-core`; the core calls `to-do-dal`,
   which reads or changes only that user's rows in PostgreSQL.

Auth can resolve users through the core PostgreSQL backend or the HTTP backend,
selected by the `auth-core-postgres` or `auth-http` feature.

### Authentication and sessions

- **Registration** (`POST /api/v1/users/create`): the password is hashed with
  Argon2 (on a blocking thread) and the email is normalized to lower case.
- **Login** (`POST /api/v1/auth/login`): Basic auth in, a JWT (HS256, 24 h) out.
  The JWT contains `unique_id` and a random `session_id` generated for that
  login. Unknown emails still burn an Argon2 verification so timing does not
  reveal which emails exist. Login writes the matching session to Redis.
- **Session** (Redis): one active session per `unique_id`. A second login
  replaces the stored `session_id`, so JWTs from earlier logins are rejected
  even if their signatures and expiry are still valid. A session expires after
  **20 minutes of inactivity** (`glue::session::SESSION_TIMEOUT_MINS`); Redis
  also deletes idle keys using the same 20-minute TTL. Every 20 authenticated
  requests the session asks for a **refresh**, which reloads the user from the
  database while retaining the same `session_id` (so deleted users stop working).
- **Logout** (`POST /api/v1/auth/logout`): deletes the Redis session only when
  the JWT's `session_id` matches the active session. A stale token cannot log
  out a newer session. The `/api/v1/users/me` endpoint also requires an active
  Redis session.
- Redis has no persistence: **restarting Redis logs everybody out**.

**Token compatibility:** the JWT claim set now includes `session_id`. Tokens
issued by the earlier format do not contain that claim and must be replaced by
logging in again after deploying the new version.

### Data model

- Auth DB: `users(id, email, password, unique_id)`; emails are unique
  case-insensitively.
- To-do DB: `to_do_items(id, title, status, user_id, created_at)`. `user_id`
  holds the numeric auth user ID as text. Every query filters by `user_id`, so a
  user can never read, update or delete another user's items. Two users may use
  the same title.
- Migrations run automatically on startup (ingress runs both databases'
  migrations).

### Rate limits

Ingress rate-limits auth routes **per client IP**. Login and logout share a token
bucket with a burst of five requests and one token added every 12 seconds.
Registration has a separate bucket with a burst of three and one token added per
hour. Rejected requests return `429` and rate-limit headers. Note that
`per_second(12)` means one token every 12 seconds, not 12 requests per second.

Which IP counts as "the client" depends on `TRUST_PROXY`:

- `TRUST_PROXY=0` (default, local native run): the TCP peer address is used.
- `TRUST_PROXY=1`: the `X-Real-IP` header is used. **Only enable this when ingress
  cannot be reached except through a proxy that overwrites that header** (the
  production Nginx does). Otherwise a client could fake its IP. If it is on but
  the proxy does not set the header, all clients share the proxy's bucket.

Production Nginx adds a second, independent limit (30 requests per minute per
IP, burst 10) on `/api/v1/auth/*` and `/api/v1/users/*`.

### Frontend and security headers

The browser app stores the JWT in `localStorage` and sends it as a Bearer token.
Ingress sets a strict Content-Security-Policy (`default-src 'self'`, WebAssembly
allowed, no framing) and a 30-second request timeout, and compresses responses.
CORS is enabled only in debug builds, for the dev server on `localhost:3000`.
In production the frontend is served from the same origin as the API.

## Run locally

### Prerequisites

- Docker with the Docker Compose plugin
- Rust toolchain and Cargo (`rust-toolchain.toml` pins the version)
- [Bun](https://bun.sh/) for building and serving the frontend
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) and the `wasm32-unknown-unknown` Rust target
- `xh` for the user-creation helper, `jq` for the smoke test
- `psql` (PostgreSQL client) for `scripts/clean_db.sh`

The image build uses Bun (no Node.js or npm needed) and `wasm-pack` inside its
build stage; the remote build server runs Docker to execute that image build.

### Configuration

Copy `.env.example` to `.env` once and keep local and Docker settings there
(`scripts/init.sh` does this for you). Generate each secret with:

```sh
openssl rand -hex 32
```

Use **hex**, not base64: the passwords are embedded in database and Redis URLs,
and base64 contains `/`, `+` and `=`, which break them. Set `POSTGRES_PASSWORD`,
`REDIS_PASSWORD` and `JWT_SECRET`, and keep the passwords inside the URLs in
sync with their password variables. The app refuses to start when `JWT_SECRET`
is shorter than 32 characters or still starts with `replace with`.

See the [configuration reference](#configuration-reference) for every variable.

### Start the app

1. From the repository root, initialize configuration and start the services:

   ```sh
   ./scripts/init.sh
   ```

   The script copies `.env.example` to `.env` if needed, starts PostgreSQL and
   Redis through Docker Compose (the Compose file also creates the `auth`
   database), builds the frontend, and builds and starts ingress in release
   mode on port `8001`. Ingress runs the auth and to-do migrations on startup.
   PostgreSQL and Redis are bound to localhost only.

2. In another terminal, register a user:

   ```sh
   ./scripts/create_user.sh user@example.com
   ```

   The helper prompts for the password without echoing it (8–128 characters),
   then posts to `http://127.0.0.1:8001/api/v1/users/create`. To target another
   ingress, set `INGRESS_URL` in the root `.env` before running the helper. The
   script sources `.env` after process startup, so a one-off prefix such as
   `INGRESS_URL=https://example.com ./scripts/create_user.sh ...` is overwritten
   when `.env` already contains `INGRESS_URL` (a current helper-script caveat).

3. Open <http://127.0.0.1:8001> and sign in. To-do items are scoped to the
   signed-in user, and logging out clears the session.

4. Stop the local stack when finished:

   ```sh
   ./scripts/cleanup.sh
   ```

   This stops the containers and preserves their database data.

To delete application rows from both PostgreSQL databases while keeping table
schemas and SQLx migration history, run:

```sh
./scripts/clean_db.sh
```

The script is intentionally limited to PostgreSQL URLs on `localhost` or
`127.0.0.1`. It requires the local Redis `cache` service to be running, removes
only keys in the `user_session_*` namespace, then truncates application tables
in both databases while preserving schemas and SQLx migration history.
The SQL uses `CONTINUE IDENTITY`, so sequences are **not** reset; this avoids
reusing old `users.id` values. The script currently prints the stale message
“Sequences will restart” before confirmation, which does not match the SQL it
executes. The script asks you to type `CLEAN to_do auth` before changing data;
use `--yes`
only for non-interactive use after verifying the targets. Do not use this helper
against production or a remote database.

### Frontend dev server (optional)

For frontend work with hot rebuilds, run ingress as above, then in `frontend/`:

```sh
bun install
bun run wasm
bun run build
bun run serve        # serves public/ on http://localhost:3000
```

When the page origin is `localhost:3000`, the app sends API calls to
`INGRESS_URL` (default `http://localhost:8001`), and ingress (debug builds only)
allows that origin through CORS (`FRONTEND_DEV_ORIGIN`).

## Configuration reference

All settings come from environment variables (loaded from `.env` by `dotenvy`
for host processes). Values without `DOCKER` in their name are for processes on
your machine; `*_DOCKER_*` values use Compose service names (`postgres`, `auth`,
`to_do`, `cache`) and are read by the Compose fragments.

| Variable | Used by | Meaning |
|---|---|---|
| `JWT_SECRET` | all servers | HMAC key for JWTs. At least 32 characters. |
| `POSTGRES_PASSWORD` | Compose, URLs | Password of the local PostgreSQL user `username`. |
| `REDIS_PASSWORD` | Compose, URLs, `clean_db.sh` | Redis `requirepass`; `clean_db.sh` uses it to remove only `user_session_*` keys. |
| `TO_DO_DB_URL`, `AUTH_DB_URL` | ingress, servers, `clean_db.sh` | PostgreSQL URLs. |
| `TO_DO_MAX_CONNECTIONS`, `AUTH_MAX_CONNECTIONS` | servers | Pool sizes (default 5). |
| `CACHE_API_URL` | servers | Redis URL, `redis://:PASSWORD@host:6379`. |
| `AUTH_API_URL` | to-do server (`auth-http` mode) | Base URL of the auth server. |
| `INGRESS_WEB_HOST` / `INGRESS_WEB_PORT` | ingress | Listen address (default `0.0.0.0:8001`). |
| `TO_DO_WEB_HOST` / `TO_DO_WEB_PORT` | to-do server | Listen address (default `127.0.0.1:8080`). |
| `AUTH_WEB_HOST` / `AUTH_WEB_PORT` | auth server | Listen address (default `127.0.0.1:8081`). |
| `TRUST_PROXY` | ingress | `1` = read the client IP from `X-Real-IP` (see [Rate limits](#rate-limits)). |
| `INGRESS_URL` | frontend build, `create_user.sh` | Where the dev frontend and helper scripts reach ingress. |
| `FRONTEND_DEV_ORIGIN` | ingress (debug) | Origin allowed by CORS (default `http://localhost:3000`). |
| `COMPUTE_PLATFORM` | Compose | Platform of the `compute-unit` image (`linux/amd64` or `linux/arm64`). |
| `TODO_DOCKER_DB_URL`, `AUTH_DOCKER_DB_URL`, `CACHE_DOCKER_URL`, `TODO_DOCKER_API_URL`, `AUTH_DOCKER_API_URL`, `INGRESS_DOCKER_API_URL` | Compose fragments | In-network endpoints. |

Production uses a separate file, `deployment/.env.production`; see the
[deployment guide](devops_handbook.md#45-configure-deploymentenvproduction).

## Test

Start PostgreSQL first, then run everything:

```sh
docker compose up -d postgres
./scripts/test.sh
```

`test.sh` loads the root `.env` and sets `DATABASE_URL` from `TO_DO_DB_URL` when
`DATABASE_URL` is not already set. SQLx needs this variable for the two
`#[sqlx::test]` PostgreSQL integration tests in `nanoservices/to_do/dal/tests/postgres.rs`.

This runs:

- Rust unit and integration tests for the whole workspace, including a
  PostgreSQL-backed check that two users can use the same title and that one
  user cannot read, update or delete another user's items, plus the Basic-auth
  parser, the in-memory to-do lifecycle and the ingress routing and rate-limit
  tests;
- frontend URL/utility tests, TypeScript type checking and the frontend build.

When running the PostgreSQL integration test directly instead of using the
script, load `.env` and export the URL yourself:

```sh
set -a
source .env
set +a
export DATABASE_URL="${DATABASE_URL:-$TO_DO_DB_URL}"
cargo test -p to-do-dal --features sqlx-postgres --test postgres -- --nocapture
```

`DATABASE_URL must be set: EnvVar(NotPresent)` means this SQLx test process did
not receive that environment variable; it is not a failure of either ownership
query by itself.

### Redis module behaviour

CI builds the Redis image and runs a behaviour test covering login, update,
refresh on the 20th request, replacement of an old session by a new login,
stale-token rejection, logout, and the presence of a Redis TTL. The script checks
that a TTL is configured; it does not wait for the key to physically expire or
exercise the `TIMEOUT` branch. To run it yourself:

```sh
docker build -f nanoservices/user-session-cache/cache-module/Dockerfile -t cache-module-ci .
docker run -d --name cache-ci cache-module-ci \
  redis-server --loadmodule ./libcache_module.so --requirepass ci-password
./scripts/test_redis_module.sh
docker rm -f cache-ci
```

### API smoke test

To include the PostgreSQL-backed API smoke test, start the local stack and sign
in to get a JWT (needs `xh` and `jq`):

```sh
export TOKEN="$(xh --ignore-stdin -a user@example.com:astrongpassword POST :8001/api/v1/auth/login | jq -r .)"
RUN_POSTGRES_INTEGRATION=1 ./scripts/test.sh
```

The smoke test starts the standalone to-do server on `TO_DO_WEB_PORT` (8080 by
default). Its API uses `title`, numeric item IDs, and `/get/{id}` and
`/delete/{id}` routes.

### Continuous integration

`.github/workflows/ci.yml` runs formatting, Clippy (warnings are errors), the
Rust and frontend tests, the Redis module behaviour test, dependency audits
(`cargo audit`, `bun audit`) and a Docker image build plus Compose file
validation.

## Deployment layouts at a glance

| Layout | What runs | Typical use | Guide |
|---|---|---|---|
| Native | ingress on the host; PostgreSQL and Redis in Compose | Development | [Run locally](#run-locally) |
| Monolith (Compose) | `ingress` + Nginx with a self-signed certificate | Trying the production shape on one machine | [§3.1](devops_handbook.md#31-monolith-ingress--nginx-with-https) |
| Microservices (Compose) | `auth`, `to_do` and a frontend Nginx as separate processes | Exploring the nanoservice split | [§3.2](devops_handbook.md#32-microservices) |
| AWS production | ALB (HTTPS) → EC2 (Nginx, ingress, Redis) → RDS PostgreSQL | Real deployment | [§4](devops_handbook.md#4-aws-production-deployment) |

## Known limitations

- Production runs on a **single** EC2 instance and a single-AZ database. Redis
  sessions live on that instance, so running several app instances would require
  a shared Redis.
- No email verification, password reset or account deletion.
- The JWT is kept in `localStorage`. The strict CSP reduces XSS risk, but an
  HttpOnly cookie would be safer.
- Rate limits are per IP only; there is no per-account lockout.
- Only one active session is supported per user; logging in elsewhere
  invalidates the previous login. The Redis session store is not persistent,
  so restarting Redis invalidates all sessions.
- The to-do database stores the numeric auth `users.id` as text and cannot
  enforce a foreign key across the separate auth database. `clean_db.sh`
  protects the local cleanup path by clearing matching Redis sessions and not
  resetting sequences, but manual database resets/restores must preserve the
  same identity relationship.
- `GET /api/v1/get` has a hard limit of 1,000 items and does not yet provide
  pagination.
- `/healthz` is a liveness check only; it does not prove PostgreSQL and Redis
  are ready.
