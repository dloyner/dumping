# Leptos Todo — full-stack Rust

A small CRUD application built with:
- **Frontend:** Leptos 0.8 (CSR/WASM), Trunk
- **Backend:** Axum 0.8, Tokio
- **Database:** PostgreSQL, SQLx 0.8
- **Communication:** JSON REST API

This intentionally uses CSR + REST rather than SSR so the frontend and backend boundaries are easy to understand. The UI and API are both written in Rust.

## Architecture

```text
Browser
  |
  | HTTP :8080
  v
Leptos UI (WASM) ---- JSON REST ----> Axum API :3000
                                          |
                                          | SQLx
                                          v
                                     PostgreSQL :5432
```

## Prerequisites

- Rust stable + `wasm32-unknown-unknown`
- `trunk` (`cargo install trunk`)
- Docker / Docker Compose

Install the WASM target and Trunk:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Run

1. Start PostgreSQL:

```bash
docker compose up -d db
```

2. Start the backend (from the repository root):

```bash
cp .env.example .env
set -a && source .env && set +a
cargo run -p backend
```

The API listens on `http://127.0.0.1:3000`. SQLx migrations run automatically at startup.

3. In another terminal, start the Leptos frontend:

```bash
cd frontend
trunk serve --address 127.0.0.1 --port 8080
```

Open <http://127.0.0.1:8080>.

## API

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/api/todos` | List todos |
| POST | `/api/todos` | Create todo (`{"title":"..."}`) |
| PATCH | `/api/todos/:id` | Toggle completion |
| DELETE | `/api/todos/:id` | Delete todo |

## Project layout

```text
.
├── backend/
│   ├── migrations/
│   └── src/
│       └── main.rs
├── frontend/
│   ├── index.html
│   ├── Trunk.toml
│   ├── style.css
│   └── src/
│       └── main.rs
├── compose.yaml
└── Cargo.toml
```

## Next steps

A sensible learning sequence:
1. Add input validation and structured API errors.
2. Add integration tests using a disposable PostgreSQL database.
3. Add authentication and per-user ownership.
4. Replace REST calls with Leptos server functions and add SSR using `cargo-leptos` + `leptos_axum`.
5. Add deployment configuration and CI.

This is a learning starter, not a production-ready app: it has no authentication, rate limiting, CSRF strategy, or production CORS policy.
