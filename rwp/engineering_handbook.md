# Engineering Handbook — Rust Web Programming: đọc hiểu project từ trong ra ngoài

> Study journal / tài liệu nghiên cứu cho project *Rust Web Programming* (theo sách của Maxwell Flitton).
> Snapshot: **2026-11-25** (đã cập nhật sau đợt review, xem Phụ lục D). Mở rộng `README.md`: README nói *làm gì và chạy thế nào*, tài liệu này nói *vì sao nó được thiết kế như vậy, từng file làm gì, dữ liệu đi đâu, và chỗ nào dễ vấp*.

**Trạng thái phát triển:** source hiện được đóng băng về feature. Chỉ sửa bug/error hoặc lỗ hổng được xác nhận. Các mục ghi là *potential feature*, *future work* hoặc *roadmap* là đề xuất, chưa được triển khai và không nên hiểu là một phần của contract hiện tại.

---

## Chương 0 — Cách dùng tài liệu này

### 0.1 Cấu trúc mỗi chương
Mỗi chương gồm: **mục tiêu** → **giải thích chi tiết** → **tóm tắt** → **lưu ý / caveats** → **câu hỏi tự kiểm tra**. Chương 16 gom toàn bộ caveats theo mức độ nghiêm trọng, Chương 17 là lộ trình học kèm bài tập.

### 0.2 Bản đồ chương

| # | Chương | Bạn sẽ hiểu |
|---|---|---|
| 1 | Bức tranh tổng thể | Project là gì, 3 hình thái chạy |
| 2 | Repo và Cargo workspace | Thư mục nào, crate nào, feature nào |
| 3 | Kiến trúc phân tầng | Pattern trait + descriptor, quy tắc phụ thuộc |
| 4 | `glue` | Lỗi, JWT, pool DB, logger dùng chung |
| 5 | Auth nanoservice | Đăng ký, đăng nhập, Argon2, session kernel |
| 6 | Session cache (Redis) | Module Redis viết bằng Rust, timeout, refresh |
| 7 | To-do nanoservice | Dữ liệu, cô lập theo user, API |
| 8 | Ingress | Ráp tất cả vào một process, rate limit, CSP |
| 9 | Frontend | Bun + esbuild + WASM + React |
| 10 | Luồng end-to-end | Sequence từng kịch bản |
| 11 | Dữ liệu và định danh | Mọi loại ID, bảng, key Redis |
| 12 | Cấu hình | Biến môi trường, ai đọc biến nào |
| 13 | Build, đóng gói, triển khai | Docker, compose, Nginx, OpenTofu, CI |
| 14 | Chiến lược test | Mỗi test bảo vệ điều gì |
| 15 | Mô hình bảo mật | Đã có gì, còn thiếu gì |
| 16 | Caveats tổng hợp | Danh sách bẫy |
| 17 | Lộ trình học | Thứ tự đọc code, bài tập |
| 18 | Glossary | Thuật ngữ |
| A–D | Phụ lục | Index file, cổng, lệnh, change log |

### 0.3 Quy ước
- Đường dẫn tính từ repo root, ví dụ `nanoservices/auth/kernel/src/lib.rs`.
- "Layout monolith" = một process `ingress`; "layout microservice" = `auth` + `to_do` tách riêng sau Nginx.
- `user_id` trong code có thể là **số** (`users.id`) hoặc **chuỗi** (`unique_id`) tùy tầng. Chương 11 phân biệt rõ.

---

## Chương 1 — Bức tranh tổng thể

### 1.1 Project là gì
Một ứng dụng **to-do nhiều người dùng** viết gần như hoàn toàn bằng Rust:

- Đăng ký và đăng nhập bằng email + mật khẩu (Argon2), nhận **JWT**.
- Mỗi request API mang JWT; server kiểm tra thêm **session trong Redis** (idle timeout, thu hồi khi logout).
- CRUD to-do **cô lập theo từng user** trong PostgreSQL.
- Frontend React (TypeScript) dùng một hàm nhỏ viết bằng Rust biên dịch ra **WebAssembly**.
- Có thể chạy như **monolith** (một binary) hoặc **microservice** (auth và to-do tách process).

Mục đích thực chất là *học kiến trúc*: cách chia một hệ thống Rust thành các "nanoservice" có thể gộp lại hoặc tách ra mà không viết lại logic.

### 1.2 Tech stack

| Lớp | Công nghệ | Ghi chú |
|---|---|---|
| Ngôn ngữ | Rust 1.100.0 (edition 2024, resolver 3) | ghim bằng `rust-toolchain.toml` |
| Web framework | axum 0.8, tokio, tower, tower-http | |
| DB | PostgreSQL 18 + sqlx 0.9 | migrations nhúng vào binary |
| Cache / session | Redis 7.4 + **module tự viết** (`redis-module`) | lệnh `session.*` |
| Băm mật khẩu | argon2 0.6 | |
| Token | jsonwebtoken 9 (HS256) | secret dùng chung |
| Rate limit | tower_governor 0.8 | chỉ trong `ingress` |
| Nhúng frontend | rust-embed 8 + mime_guess | |
| Frontend | React 18, TypeScript, axios, esbuild | build bằng **Bun** |
| WASM | wasm-bindgen + wasm-pack | crate `rust-interface` |
| Container | Docker multi-stage, distroless runtime, Compose, Nginx | |
| IaC | OpenTofu/Terraform (AWS) | máy build từ xa + stack production: ALB + ACM, EC2, RDS |
| CI | GitHub Actions | fmt, clippy, test, audit, build image, test hành vi Redis |

### 1.3 Bốn hình thái chạy

| Hình thái | Lệnh | Process |
|---|---|---|
| **Local dev** | `./scripts/init.sh` | Postgres + Redis (Docker) và `ingress` chạy trên host (release) |
| **Monolith (Docker)** | `compose.yaml` + `ingress/build-config.yml` | ingress, nginx (TLS tự ký), postgres, cache |
| **Microservice (Docker)** | `compose.yaml` + `auth` + `to_do` + `frontend` fragments | auth, to_do, frontend-nginx, postgres, cache |
| **Production (AWS)** | `deployment/run_deployment.sh` | ALB (HTTPS) → EC2 (nginx, ingress, cache) → RDS PostgreSQL |

Hai layout Docker dùng chung cổng 80 (nginx vs frontend) nên **không chạy cùng lúc**.

### 1.4 Sơ đồ hệ thống (layout monolith)

```text
 Browser (React + WASM)
    |  HTTP  Authorization: Basic (login) | Bearer <JWT> (mọi API khác)
    v
 [nginx]  (monolith Docker: TLS tự ký; production: nằm sau ALB; proxy_pass -> ingress)
    v
 +----------------------------- ingress (axum, :8001) -----------------------------+
 |  GET /, /bundle.js, /bundle.css ... (nhúng sẵn trong binary)                      |
 |  /api/v1/auth/*  /api/v1/users/*      /api/v1/{get,create,update,delete}          |
 |        |                                      |                                   |
 |  auth networking (axum handlers)       to-do networking (axum handlers)           |
 |        |                                      |   AuthUser extractor              |
 |   auth-core (logic)                           |-> GetUserSession (auth-kernel)    |
 |        |                                      |        |                          |
 |   auth-dal (SQL)                              |   cache-client ---- Redis         |
 |        |                                      |        |         `session.*`      |
 |        |                                      |        |   (khi REFRESH: đọc user |
 |        |                                      |        |    từ auth DB, login lại)|
 |        |                                 to-do-core (logic)                        |
 |        |                                 to-do-dal  (SQL)                          |
 +--------|--------------------------------------|-----------------------------------+
          v                                      v
    PostgreSQL `auth`                      PostgreSQL `to_do`
```

### 1.5 Vòng đời một request có đăng nhập (6 bước)
1. Browser gửi `GET /api/v1/get` kèm `Authorization: Bearer <JWT>`.
2. Extractor `HeaderToken` giải mã JWT (HS256, kiểm tra `exp`) và lấy `unique_id` cùng `session_id`.
3. Extractor `AuthUser` gọi `GetUserSession::get_user_session(unique_id, session_id)`.
4. Kernel gửi `session.update <unique_id> <session_id>` tới Redis. Kết quả: `OK` kèm số `perm_user_id`, `REFRESH`, hoặc lỗi 401 (`NOT_FOUND`, `TIMEOUT`, `STALE`).
5. Handler lấy `user_id` (chuyển thành chuỗi) và gọi `to-do-core`, rồi `to-do-dal` chạy SQL có điều kiện sở hữu.
6. Handler trả JSON (danh sách `pending` và `done`).

### 1.6 "Nanoservice" nghĩa là gì ở đây
Mỗi domain (auth, to_do) được chia thành các crate nhỏ theo tầng (`dal`, `core`, `networking`, `kernel`). Vì tầng `core` và `dal` không biết gì về HTTP, ta có thể:
- **Gộp** chúng vào một process (`ingress` gọi trực tiếp, in-process), hoặc
- **Tách** thành server riêng (`auth-axum-server`, `to-do-axum-server`) và nối với nhau bằng HTTP.

### Tóm tắt Chương 1
Một hệ thống to-do đa người dùng với kiến trúc phân tầng; JWT kết hợp session Redis; chạy được dưới dạng monolith lẫn microservice nhờ cùng một bộ crate.

### Lưu ý
- "Microservice" ở đây vẫn **dùng chung một Postgres server** (hai database `auth` và `to_do`) và **chung một Redis**.
- Cả hai layout đều dùng **chung `JWT_SECRET`** giữa các process (khóa đối xứng).

### Tự kiểm tra
1. Vì sao cần cả JWT lẫn session Redis? (đáp án ở Chương 6)
2. Hai layout Docker khác nhau ở những process nào?

---

## Chương 2 — Repo và Cargo workspace

### 2.1 Cây thư mục chú giải

```text
.
├── Cargo.toml                 # workspace: 12 member, dependency + lint + profile dùng chung
├── rust-toolchain.toml        # ghim Rust 1.100.0 + rustfmt, clippy, target wasm32
├── compose.yaml               # postgres + cache (Redis có module) — nền cho mọi layout
├── Dockerfile                 # một Dockerfile đa kiến trúc: image "compute-unit" (ingress + to-do + auth)
├── .env.example               # mẫu cấu hình (copy thành .env)
├── .github/workflows/ci.yml   # CI: fmt, clippy, test, frontend, redis-module (có test hành vi), audit, images
├── glue/                      # tiện ích dùng chung: lỗi, JWT, pool DB, logger, hằng số session
├── ingress/                   # process monolith: ráp frontend + auth + to-do
│   ├── src/main.rs            # logic ingress + 4 test functions
│   ├── build-config.yml       # compose fragment: ingress + nginx
│   ├── nginx.conf             # template nginx (chỉ khối server): TLS tự ký, proxy_pass -> ingress
│   └── scripts/run_server.sh  # bun build frontend rồi cargo run --release
├── nanoservices/
│   ├── auth/
│   │   ├── dal/               # SQL + schema + migrations (bảng users)
│   │   ├── core/              # logic: login, create user, get user
│   │   ├── kernel/            # session (Redis) + 2 backend tra user (core-postgres | http)
│   │   ├── networking/axum_server/  # route + extractor + main.rs standalone
│   │   └── build-config.yml   # compose fragment cho service `auth`
│   ├── to_do/
│   │   ├── dal/               # SQL + migrations + test Postgres
│   │   ├── core/              # logic CRUD + validate
│   │   ├── networking/axum_server/  # route + AuthUser + main.rs standalone + test API
│   │   ├── scripts/test_server_com.sh  # script E2E (xem lưu ý: đã cũ)
│   │   └── build-config.yml   # compose fragment cho service `to_do`
│   └── user-session-cache/
│       ├── cache-module/      # module Redis (cdylib) cung cấp session.login/update/logout
│       └── cache-client/      # client async gọi các lệnh đó
├── frontend/
│   ├── src/                   # React + TS (api/, components/, interfaces/)
│   ├── rust-interface/        # crate WASM: rust_generate_button_text
│   ├── public/                # index.html (commit) + bundle.js/css, .wasm (sinh ra, bị gitignore)
│   ├── tests/                 # bun test
│   ├── esbuild.js, package.json, tsconfig.json, serve.json
│   ├── nginx.conf             # template nginx cho layout microservice
│   ├── Dockerfile, build-config.yml
├── builds/                    # OpenTofu: máy build EC2 từ xa
│   ├── main.tf, .terraform.lock.hcl
│   ├── build_server.sh        # user_data khi EC2 boot (cài toolchain, Bun)
│   ├── publish_images.sh      # provision + build 2 image trên máy build + push Docker Hub
│   └── run_build.sh           # (cũ) build binary ingress rồi lấy về
├── deployment/                # OpenTofu + script: stack production AWS (ALB, EC2, RDS)
│   ├── main.tf, load_balancer.tf, security_groups.tf
│   ├── run_deployment.sh, server_build.sh
│   ├── compose.yaml, nginx.conf, init-databases.sql
│   └── .env.production.example
└── scripts/
    ├── init.sh, cleanup.sh, test.sh, basic_test.sh, test_redis_module.sh
    ├── create_user.sh, clean_db.sh, init-auth-db.sql, gen_dev_cert.sh
    └── deployment_cleanup.sh  # dọn local / máy build / production
```

### 2.2 Bảng crate trong workspace

| Package | Đường dẫn | Loại | Vai trò |
|---|---|---|---|
| `glue` | `glue/` | lib | Lỗi, JWT, pool DB, logger, hằng số session. Feature `axum`, `sqlx` |
| `auth-dal` | `nanoservices/auth/dal` | lib | Bảng `users`, SQL, Argon2, migrations |
| `auth-core` | `nanoservices/auth/core` | lib | Logic login, tạo user, lấy user |
| `auth-kernel` | `nanoservices/auth/kernel` | lib | Session Redis + tra user. Feature `core-postgres` hoặc `http` |
| `auth-axum-server` | `nanoservices/auth/networking/axum_server` | lib + bin | Route auth, extractor Basic, `main.rs` |
| `to-do-dal` | `nanoservices/to_do/dal` | lib | Bảng to-do, SQL. Feature `sqlx-postgres` |
| `to-do-core` | `nanoservices/to_do/core` | lib | Logic CRUD + validate |
| `to-do-axum-server` | `nanoservices/to_do/networking/axum_server` | lib + bin | Route to-do, `AuthUser`. Feature `auth-core-postgres` hoặc `auth-http` |
| `cache-client` | `nanoservices/user-session-cache/cache-client` | lib | Gọi `session.*` qua Redis |
| `cache-module` | `nanoservices/user-session-cache/cache-module` | **cdylib** | Module nạp vào Redis |
| `ingress` | `ingress/` | bin | Ráp mọi thứ, serve frontend |
| `rust-interface` | `frontend/rust-interface` | **cdylib** (wasm) | Hàm chạy trong trình duyệt |

### 2.3 Đồ thị phụ thuộc

```text
rust-interface (wasm, độc lập)

                      glue  <------------------------------------------+
                        ^                                              |
   auth-dal ------------+                                              |
      ^   ^                                                            |
      |   +-- auth-core                          cache-client ---------+
      |          ^                                    ^
      |          |                                    |
      +------ auth-kernel (core-postgres | http) -----+
                 ^        ^
                 |        +----------------------------+
   auth-axum-server                                    |
        ^                          to-do-dal <-- to-do-core
        |                               ^            ^
        |                               +-- to-do-axum-server (dùng auth-kernel)
        |                                            ^
        +-------------------+------------------------+
                            |
                         ingress  (dùng cả auth-axum-server lẫn to-do-axum-server)

cache-module (cdylib): không phụ thuộc crate nội bộ nào; chỉ redis-module + chrono
```

Điểm đáng chú ý: **`auth-kernel` phụ thuộc `cache-client`; `to-do-axum-server` phụ thuộc `auth-kernel`**. Nghĩa là to-do không tự nói chuyện với Redis, mà đi qua kernel của auth.

### 2.4 Feature flags và sự loại trừ lẫn nhau

| Crate | Feature | Ý nghĩa |
|---|---|---|
| `glue` | `axum` | `IntoResponse` cho lỗi, `FromRequestParts` cho `HeaderToken` |
| `glue` | `sqlx` | module `db` (pool, `db_error`) |
| `to-do-dal` | `sqlx-postgres` | bật impl Postgres, `migrations`, `FromRow` |
| `auth-kernel` | `core-postgres` | tra user bằng `auth-core` + Postgres (in-process) |
| `auth-kernel` | `http` | tra user bằng HTTP tới `AUTH_API_URL/api/v1/users/me` |
| `to-do-axum-server` | `auth-core-postgres` (default) | mở thêm pool tới auth DB |
| `to-do-axum-server` | `auth-http` | không mở auth DB, gọi auth qua HTTP |

Hai backend của kernel được code theo kiểu **loại trừ**: `#[cfg(all(feature = "http", not(feature = "core-postgres")))]`. Vì vậy `RedisSessionDescriptor` có **hai hình dạng**:

- `core-postgres`: `RedisSessionDescriptor<T> { store: T }` (có handle DB).
- `http` (không có core-postgres): `RedisSessionDescriptor` là struct rỗng.

Ma trận build đang dùng:

| Mục tiêu | Lệnh | Backend |
|---|---|---|
| ingress | `cargo build --release -p ingress` | core-postgres |
| to-do standalone (Docker microservice) | `cargo build --release -p to-do-axum-server --features=auth-http --no-default-features` | http |
| auth standalone | `cargo build --release -p auth-axum-server` | core-postgres |
| CI kiểm tra riêng | `cargo check -p auth-kernel --no-default-features --features http` | http |

### 2.5 Cấu hình workspace
- `edition = "2024"`, `resolver = "3"`.
- Lint: `unsafe_op_in_unsafe_fn = deny`, `clippy::correctness = deny`; mỗi crate bật bằng `[lints] workspace = true`.
- Release profile: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1` (build chậm hơn, binary nhanh và nhỏ hơn).
- `rust-toolchain.toml` ghim `1.100.0`, `profile = minimal`, kèm `rustfmt`, `clippy`, target `wasm32-unknown-unknown`. Dockerfile dùng `rust:1.100-bookworm` và CI dùng `RUST_VERSION: 1.100.0` cho khớp.

### Tóm tắt Chương 2
12 crate, chia theo domain và theo tầng. Hai cặp feature quyết định backend tra user: **in-process Postgres** (monolith) hoặc **HTTP** (microservice).

### Lưu ý
- **Feature unification:** Cargo hợp nhất feature trong **một lần build**. Build `ingress` và `to-do-axum-server --features auth-http` **trong cùng một lệnh** sẽ kéo cả hai feature vào `auth-kernel`, làm vỡ giả định loại trừ. Dockerfile build từng lệnh riêng nên an toàn.
- `cache-module` có `test = false`, nên `cargo test --workspace` **không chạy test nào** cho nó (chỉ biên dịch).
- Cây thư mục bạn có chứa `aws_access_key.txt`. File này bị gitignore, nhưng không nên tồn tại trong working tree (Chương 15).

### Tự kiểm tra
1. Vì sao `RedisSessionDescriptor` có hai định nghĩa khác nhau?
2. Lệnh nào build to-do ở chế độ HTTP, và vì sao dùng `--no-default-features`?

---

## Chương 3 — Kiến trúc phân tầng

### 3.1 Các tầng và trách nhiệm

| Tầng | Crate | Biết về | Không được biết về |
|---|---|---|---|
| **networking** | `*-axum-server` | HTTP, header, JSON, status code | SQL |
| **core** | `auth-core`, `to-do-core` | quy tắc nghiệp vụ, validate | HTTP, SQL cụ thể |
| **kernel** | `auth-kernel` | session, cách tra user | HTTP routing |
| **dal** | `*-dal` | SQL, schema, migrations | HTTP, nghiệp vụ |
| **glue** | `glue` | lỗi, JWT, pool | mọi thứ nghiệp vụ |

Chiều gọi: `networking → core → dal`, và `networking → kernel → (cache-client | core | HTTP)`. Chiều ngược lại không bao giờ xảy ra.

### 3.2 Pattern "trait giao dịch + descriptor"
Ví dụ với to-do (`nanoservices/to_do/dal/src/to_do_items/`):

```rust
// transactions/create.rs
pub trait SaveOne {
    fn save_one(&self, item: NewToDoItem, user_id: &str)
        -> impl Future<Output = Result<ToDoItem, NanoServiceError>> + Send;
}

// descriptors.rs
pub struct SqlxPostgresDescriptor { pub pool: sqlx::PgPool }

// transactions/create.rs
impl SaveOne for SqlxPostgresDescriptor { /* INSERT ... RETURNING */ }
```

- Mỗi **thao tác** (`SaveOne`, `GetAll`, `GetById`, `UpdateOne`, `DeleteOne`) là một trait nhỏ.
- **Descriptor** là "chỗ lưu trữ cụ thể" (Postgres). Core chỉ nhận `T: SaveOne`, không biết Postgres.
- Trait dùng `fn ... -> impl Future<...> + Send` (async trong trait của Rust hiện đại), không cần `async_trait` và không tốn `Box<dyn Future>`.
- Vì là generics, trình biên dịch **monomorphize** (static dispatch): không có chi phí gọi động.

Lợi ích thực tế đã thấy trong repo:
- `to-do-core` test bằng struct `Store` giả chỉ vài dòng (không cần DB).
- `api_integration.rs` thay Postgres bằng `MemoryStore` (HashMap) và thay Redis bằng `MemoryUserSessions`, rồi chạy **toàn bộ router axum** trong bộ nhớ.

### 3.3 `AppState<T, X>`
Cả hai server dùng cùng một khuôn:

```rust
pub struct AppState<T, X> { pub store: T, pub user_session: X }
```

- `T` = kho dữ liệu (trait bound theo từng route).
- `X` = nhà cung cấp session (`GetUserSession`, `LoginUserSession`, `LogoutUserSession`).
- Route handler khai báo đúng bound nó cần, ví dụ `get_all<T: GetAll, X: GetUserSession>`.

### 3.4 Pattern extractor
Axum cho phép viết "tham số handler" tự trích dữ liệu từ request:

| Extractor | File | Làm gì |
|---|---|---|
| `HeaderToken` | `glue/src/token.rs` | đọc `Authorization: Bearer`, giải mã JWT |
| `Credentials` | `auth/.../extract_auth.rs` | đọc `Authorization: Basic`, giải base64 |
| `AuthUser(String)` | `to_do/.../auth_user.rs` | `HeaderToken` rồi tra session ra `user_id` |

Handler chỉ cần khai báo `AuthUser(owner_key): AuthUser`; nếu extractor lỗi, request bị chặn trước khi chạm vào logic.

### 3.5 Mô hình lỗi xuyên tầng
Mọi tầng trả `Result<_, NanoServiceError>` (`message` + `status` enum). Ở rìa HTTP, `NanoServiceError: IntoResponse` đổi status enum thành mã HTTP (Chương 4). Vì vậy `core` và `dal` mang "ý định lỗi" (NotFound, Conflict, ...) mà không phụ thuộc axum.

### Tóm tắt Chương 3
Tách *cái gì* (trait) khỏi *ở đâu* (descriptor), cộng với state generic và extractor, giúp cùng một logic chạy được trên Postgres thật, bộ nhớ, in-process hoặc qua HTTP.

### Lưu ý
- Giá phải trả: chữ ký generic dài, thông báo lỗi biên dịch khó đọc.
- Kernel dùng `cfg` để đổi hình dạng struct; cực kỳ linh hoạt nhưng làm code khó điều hướng (nhiều khối trùng nhau cho hai feature).

### Tự kiểm tra
1. Nếu muốn thêm backend MySQL cho to-do, bạn phải thêm gì và *không* phải sửa gì?
2. Vì sao `AuthUser` implement `FromRequestParts<AppState<T, X>>` thay vì `FromRequestParts<S>` chung chung?

---

## Chương 4 — `glue`: nền móng dùng chung

**Mục tiêu:** hiểu 5 file trong `glue/src/` vì mọi crate khác đều dựa vào chúng.

### 4.1 `errors.rs`
- `NanoServiceErrorStatus`: `NotFound(404)`, `Forbidden(403)`, `Unknown(500)`, `BadRequest(400)`, `Conflict(409)`, `Unauthorized(401)`.
- `NanoServiceError { message, status }` (dùng `thiserror`).
- Trait `ResultExt` cho mọi `Result<T, E: Display>`:
  - `.eject(status)`: đổi lỗi bất kỳ thành `NanoServiceError`.
  - `.eject_ctx(status, ctx)`: như trên, thêm tiền tố ngữ cảnh `"ctx: lỗi gốc"`.
- Với feature `axum`: `impl IntoResponse for NanoServiceError`.
  - Body luôn là **JSON string** (ví dụ `"invalid title"`), không phải object.
  - Với `Unknown`, message gốc chỉ được log; client nhận `"internal server error"`. Đây là chủ đích để không lộ chi tiết nội bộ.

### 4.2 `token.rs` (JWT)
- Claims: `{ unique_id: String, session_id: String, exp: u64 }`. Thuật toán
  **HS256**. TTL **24 giờ**. `session_id` là UUID v4 riêng cho mỗi lần login; nó
  ràng buộc JWT với session Redis hiện tại.
- Secret lấy từ env `JWT_SECRET`, cache trong `OnceLock` (đọc một lần cho cả process).
- `validate_secret()`: lỗi nếu secret **ngắn hơn 32 ký tự** hoặc bắt đầu bằng `"replace with"` (chính là giá trị mẫu trong `.env.example`). Được gọi lúc khởi động `ingress`, `auth-axum-server` và `to-do-axum-server` standalone (bản standalone của to-do từng bỏ sót, đã sửa trong đợt review).
- `encode()` / `decode()`: `decode` lỗi → `Unauthorized`.
- Với feature `axum`: `HeaderToken: FromRequestParts` đọc header `Authorization`, yêu cầu scheme `Bearer` (không phân biệt hoa thường), rồi `decode`. Mọi lỗi đều là 401.

### 4.3 `db.rs` (feature `sqlx`)
- `create_pool(url, max)`: `PgPoolOptions` với `acquire_timeout = 5s`, **`connect_lazy`** (không kết nối ngay).
- `pool_from_env(prefix)`: đọc `{PREFIX}_DB_URL` (bắt buộc) và `{PREFIX}_MAX_CONNECTIONS` (mặc định 5). Dùng `"TO_DO"` và `"AUTH"`.
- `db_error(sqlx::Error)`: vi phạm unique → `Conflict("already exist")` (409); lỗi khác → log và trả `Unknown("database error")`.

### 4.4 `log/`
- `init_logger()`: `tracing_subscriber` với mức tối đa **INFO**, đặt làm global subscriber (gọi hai lần sẽ panic).
- `log/mod.rs` re-export `TraceLayer`, `DefaultMakeSpan`, ... của `tower-http` để các server dùng chung cấu hình trace.
- Các hàm `log_info/warn/...` là `async` dù không `await` gì (tiện ích mỏng).

### 4.5 `session.rs`
Một hằng duy nhất: `SESSION_TIMEOUT_MINS: usize = 20`. Trước đây con số 20 được lặp ở hai chỗ (`login.rs` của auth-networking và `get.rs` của auth-kernel); giờ cả hai import từ đây. Lưu ý: ngưỡng refresh `REFRESH_THRESHOLD = 20` trong `cache-module` vẫn nằm riêng vì crate đó chạy bên trong Redis và cố ý không phụ thuộc `glue` (Chương 6).

### Tóm tắt Chương 4
`glue` cho cả hệ thống bốn thứ: một kiểu lỗi, một cách phát/kiểm JWT, một cách tạo pool, một hằng số timeout session.

### Lưu ý
- `connect_lazy` nghĩa là `create_pool` thành công kể cả khi DB không tồn tại. Nhờ vậy test của `ingress` dựng được cả app mà không cần DB. Trong chạy thật, `run_migrations` ngay sau đó mới là lần kết nối đầu tiên nên lỗi DB lộ ra lúc khởi động.
- JWT mặc định của `jsonwebtoken` cho phép **leeway 60 giây** khi kiểm `exp`.
- Test của `glue` đặt secret ngắn (`"glue-token-tests"`) vì không gọi `validate_secret`.

### Tự kiểm tra
Vì sao lỗi `Unknown` bị che message ở HTTP nhưng lỗi `BadRequest` thì không?

---

## Chương 5 — Auth nanoservice

### 5.1 `auth-dal`

**Migrations** (`nanoservices/auth/dal/migrations/`):

| File | Nội dung |
|---|---|
| `20261113092138_init.sql` | `users(id SERIAL PK, email UNIQUE, password, unique_id UNIQUE)` |
| `20261121105757_make_email_unique.sql` | chuẩn hóa email về `lower(btrim(...))`, tạo `UNIQUE INDEX users_email_lower_uq ON users(lower(email))` |

**Schema** (`users/schema.rs`):

| Kiểu | Mục đích |
|---|---|
| `NewUser { email, password_hash, unique_id }` | dữ liệu để INSERT. `NewUser::new` sinh `unique_id = UUID v4` và băm mật khẩu bằng **Argon2** (mặc định của crate, muối ngẫu nhiên) |
| `User { id: i32, email, password_hash, unique_id }` | dòng đọc từ DB. Cột `password` map vào `password_hash`; `Debug` che hash thành `[redacted]` |
| `TrimmedUser { id, email, unique_id }` | phiên bản "an toàn" không có hash (trả ra API `/users/me`) |

Hai hàm xác minh mật khẩu:
- `verify_password`: so sánh thật.
- `verify_dummy_password`: băm/so với một hash giả (tạo một lần bằng `OnceLock`) để **tốn thời gian CPU tương đương** khi email không tồn tại. Chống đoán email qua chênh lệch thời gian phản hồi (timing attack).

**Transactions:** `SaveOne` (INSERT ... RETURNING), `GetByEmail`, `GetByUniqueId` (đều `fetch_optional`, không thấy → `NotFound`). `AuthStore = SaveOne + GetByEmail + GetByUniqueId` là trait "gói" dùng làm bound cho router.

**`migrations.rs`:** `sqlx::migrate!("./migrations")` **nhúng migration vào binary lúc biên dịch**, rồi `.run(pool)` khi khởi động.

### 5.2 `auth-core` (logic thuần)

| Hàm | Quy tắc |
|---|---|
| `login(store, email, password)` | email `trim` + lowercase; từ chối nếu dài hơn 254 hoặc không có `@` (400). Không tìm thấy user → chạy `verify_dummy_password` rồi trả 401 `"invalid email or password"`. Sai mật khẩu → cùng 401. Đúng → sinh `session_id` UUID v4, phát JWT `HeaderToken{unique_id, session_id}` và trả cả session ID cho networking để ghi Redis |
| `users::create(store, CreateUser)` | cùng quy tắc email; mật khẩu **8–128 ký tự** (đếm theo ký tự Unicode). Băm Argon2 trong `spawn_blocking` rồi lưu |
| `users::get_by_unique_id` | lấy `User` rồi chuyển sang `TrimmedUser` |

Cả băm và xác minh Argon2 chạy trong `tokio::task::spawn_blocking` để **không chặn executor async** (Argon2 cố ý chậm và tốn RAM).

### 5.3 `auth-kernel` (tầng điều phối session)

Đây là tầng khó đọc nhất vì nhiều `cfg`. Hãy nhớ mô hình:

```text
get_user_session(unique_id, session_id)
   └─ cache_client::update(addr, unique_id, session_id)  ──► Redis: session.update
        ├─ Ok(user_id)  ──► trả UserSession{user_id}
        ├─ STALE        ──► 401 (JWT không còn là phiên active)
        └─ Refresh      ──► tra user (core-postgres: DB trực tiếp | http: GET /users/me)
                            └─ cache_client::login(addr, unique_id, session_id, 20 phút, user.id)
                               giữ nguyên session_id, reset counter, trả UserSession{user_id}
```

- `lib.rs::cache_api_url()` đọc `CACHE_API_URL` một lần (`OnceLock`). `ingress` và `auth-axum-server` gọi hàm này lúc khởi động để **fail sớm nếu thiếu biến** (nhưng chưa kiểm tra Redis có sống không).
- `user_session/transactions/get.rs`: trait `GetUserSession`, nhận cả `unique_id` và `session_id`; có hai impl (core-postgres và http). Dùng `SESSION_TIMEOUT_MINS` (20 phút) từ `glue::session`.
- `login.rs`: trait `LoginUserSession`, truyền `session_id` vào `cache_client::login`.
- `logout.rs`: trait `LogoutUserSession`, truyền cả `unique_id` và `session_id`; `"STALE"` là 401, còn `"OK"` và `"NOT_FOUND"` được chấp nhận để logout idempotent.
- `api/users/get.rs`: hai cách tra user:
  - `get_user_by_unique_id_core(store, id)`: gọi `auth-core` + Postgres.
  - `get_user_by_unique_id_http(id, session_id)`: tự **ký JWT nội bộ** chứa cùng `unique_id` và `session_id`, gọi `GET {AUTH_API_URL}/api/v1/users/me` với `Bearer`, parse `TrimmedUser`.

Chi tiết quan trọng của backend HTTP: to-do server **tự ký JWT** (vì có `JWT_SECRET`) để gọi auth. Hai service vì thế phải chia sẻ cùng một secret. Endpoint `/users/me` cũng kiểm tra session Redis, không chỉ kiểm chữ ký JWT.

**Caveat của đường REFRESH ở microservice mode:** to-do đã gọi `session.update` và nhận `REFRESH`, sau đó gọi auth `GET /users/me`. Handler `/users/me` lại chạy `GetUserSession`, nên nó thực hiện thêm một `session.update`; vì counter của lần REFRESH đầu vẫn ở ngưỡng, auth service cũng có thể đi qua nhánh REFRESH, tra user và `session.login`. Khi HTTP call trả về, to-do kernel lại gọi `session.login` lần nữa. Kết quả không phải vòng lặp vô hạn (auth service dùng backend `core-postgres`), nhưng refresh qua HTTP có thêm một vòng Redis và lookup user so với monolith. Đây là một điểm tối ưu/thiết kế cần cân nhắc trước khi mở rộng tải.

### 5.4 `auth-axum-server` (networking)

| Method | Path | Extractor | Handler | Thành công |
|---|---|---|---|---|
| POST | `/api/v1/auth/login` | `Credentials` (Basic) | login | `200` + JSON string là JWT. JWT có `unique_id` + `session_id`; đồng thời ghi session Redis |
| POST | `/api/v1/auth/logout` | `HeaderToken` (Bearer) | logout | `204` nếu session hiện tại được xóa hoặc không còn; `STALE` → `401` |
| POST | `/api/v1/users/create` | JSON `{email,password}` | create | `201` (không body) |
| GET | `/api/v1/users/me` | `HeaderToken` | get_me | `200` + `TrimmedUser`, sau khi kiểm tra Redis session |

- `Credentials`: parse `Authorization: Basic base64(email:password)`; tách tại dấu `:` **đầu tiên**, nên mật khẩu được phép chứa `:`, còn email thì không.
- Router được tách thành các hàm `auth_router`, `create_router`, `get_router` để **`ingress` có thể gắn rate limit riêng cho từng nhóm** (Chương 8).
- `main.rs` standalone: `dotenv` → logger → `validate_secret` → `cache_api_url` → pool AUTH → **migrate** → bind `AUTH_WEB_HOST:AUTH_WEB_PORT` (mặc định `127.0.0.1:8081`) → serve với trace layer. **Không có rate limit, không có CSP, không có CORS.**

### Tóm tắt Chương 5
`dal` lưu và băm, `core` quyết định, `kernel` nối với Redis và chọn cách tra user, `networking` lo HTTP. Sau login, "danh tính" của user trong hệ thống là `unique_id` (UUID) nằm trong JWT.

### Lưu ý
- Đăng ký trùng email trả **409** (từ `db_error`), nên API cho phép dò email đã tồn tại (bị rate limit nếu đi qua ingress).
- Migration thứ hai của auth sẽ **làm hỏng khởi động** nếu DB cũ đã có hai email chỉ khác nhau về hoa thường **[suy luận từ SQL]**.
- Standalone `auth-axum-server` mở `users/create` không giới hạn tốc độ.

### Tự kiểm tra
1. Vì sao login trả cùng một thông báo cho "email không tồn tại" và "sai mật khẩu"?
2. `verify_dummy_password` giải quyết rủi ro gì, và nó tốn gì?

---

## Chương 6 — Session cache (Redis module)

### 6.1 Vì sao cần session ngoài JWT
JWT xác minh chữ ký và `exp`, nhưng bản thân JWT **không biết session nào đang active**. Hệ thống ràng buộc mỗi token với một `session_id` và lưu ID hiện tại trong Redis:
1. **Thu hồi**: logout chỉ xóa key nếu JWT mang đúng `session_id` đang active; token cũ không thể xóa session mới.
2. **Một phiên active trên mỗi user**: login lần hai tạo `session_id` mới, làm token từ lần login trước trở thành `STALE`.
3. **Idle timeout** 20 phút.
4. **Ánh xạ** `unique_id → users.id` nhanh, không cần truy vấn DB mỗi request.
5. **Kiểm lại user định kỳ** qua cơ chế `REFRESH`.

### 6.2 `cache-module`: module Redis viết bằng Rust
Biên dịch thành `libcache_module.so` (cdylib), nạp bằng `redis-server --loadmodule ./libcache_module.so`. Đăng ký ba lệnh (`redis_module!` trong `lib.rs`, tên module `user_sessions`). Cả ba khai báo **không có key** (first/last/step = 0): tên key thật (`user_session_<unique_id>`) do module tự ghép từ tham số, nên không thể khai báo như một tham số key của Redis.

| Lệnh | Cờ | Tham số | Hành vi | Trả về |
|---|---|---|---|---|
| `session.login` | `write fast deny-oom` | `unique_id session_id timeout_mins perm_user_id` | ghi `last_interacted = now`, `session_id`, `timeout_mins`, `counter = 0`, `perm_user_id`; đặt TTL | `OK` |
| `session.update` | `write fast deny-oom` | `unique_id session_id` | so khớp session ID rồi cập nhật activity/counter | số `perm_user_id`, `REFRESH`, `TIMEOUT`, `NOT_FOUND`, hoặc `STALE` |
| `session.logout` | `write fast` | `unique_id session_id` | chỉ xóa key nếu session ID khớp | `OK`, `NOT_FOUND`, hoặc `STALE` |

`session.logout` **không** có `deny-oom`: nó chỉ xóa dữ liệu, nên phải chạy được cả khi Redis đầy bộ nhớ.

Dữ liệu: một **hash** tại key `user_session_<unique_id>` với các field `session_id`, `last_interacted` (chuỗi `YYYY-MM-DD HH:MM:SS` UTC), `timeout_mins`, `counter`, `perm_user_id`.

**TTL của Redis.** Sau mỗi `session.login` và mỗi `session.update` hợp lệ (kể cả nhánh `REFRESH`), hàm `touch_expiry` đặt TTL = `timeout_mins * 60` giây. Với timeout 20 phút, TTL là đúng 20 phút. Redis có thể tự xóa key trước khi request tiếp theo tới; khi đó client nhận `NOT_FOUND` thay vì `TIMEOUT`, nhưng cả hai đều là 401.

**Thuật toán `session.update`:**

```text
key không tồn tại ........................ NOT_FOUND
session_id không khớp ................... STALE (không sửa/xóa phiên hiện tại)
elapsed_secs = now - last_interacted
elapsed_secs > timeout_mins * 60 ....... xóa key  -> TIMEOUT
counter += 1
counter >= 20 ........................... last_interacted = now, gia hạn TTL -> REFRESH
ngược lại ................................ last_interacted = now, gia hạn TTL -> perm_user_id
```

Ý nghĩa: timeout là **trượt** (mỗi request hợp lệ làm mới đồng hồ). Request hợp lệ thứ 20 kích hoạt `REFRESH`; kernel tra lại user từ nguồn thật rồi gọi `session.login` với **cùng `session_id`**, đặt counter về 0. Login/logout vẫn gọi `replicate_verbatim()` để lệnh ghi được truyền tới replica/AOF nếu bật chúng (hiện stack chạy một Redis đơn, không replica). `session.update` chỉ thay counter/activity sau khi session ID khớp.

### 6.3 `cache-client`
- Một `ConnectionManager` toàn cục trong `tokio::sync::OnceCell`, tạo ở lần gọi đầu và clone cho mỗi lệnh (tự reconnect).
- `update` ánh xạ kết quả Redis thành: `TIMEOUT`/`NOT_FOUND` → lỗi **401**; `REFRESH` → `UserSessionStatus::Refresh`; số → `UserSessionStatus::Ok(i32)`.
- Redis báo lỗi → `Unknown` (500).

### 6.4 Dòng thời gian ví dụ
```text
t=0     login               -> key tạo, session_id=s1, counter=0, TTL = 20 phút
t=1m    request #1          -> OK(user_id), counter=1, TTL được gia hạn
t=...   request #19         -> OK, counter=19
        request #20         -> REFRESH: kernel đọc user từ DB, gọi session.login
                               với cùng session_id=s1 -> counter=0
login lần 2                  -> session_id=s2 ghi đè s1
JWT có s1                   -> STALE -> 401; không thể logout session s2
JWT có s2                   -> hoạt động bình thường
sau 20 phút idle            -> Redis xóa key theo TTL; request kế tiếp -> NOT_FOUND -> 401
```

### 6.5 Đóng gói và kiểm thử
`cache-module/Dockerfile`: stage build `rust:1.100-bookworm` (+ `libclang-dev` vì crate `redis-module` cần bindgen) rồi stage chạy `redis:7.4-bookworm` (tag theo nhánh minor để nhận bản vá bảo mật khi build lại), copy `.so`. Trong `compose.yaml`, service `cache` **ghi đè `command`** để thêm `--requirepass ${REDIS_PASSWORD}`.

CI có job `redis-module`: build image, nạp module rồi chạy `scripts/test_redis_module.sh`, kiểm tra `NOT_FOUND`, login, update, login lần hai làm token cũ thành `STALE`, token cũ không logout được phiên mới, refresh ở request thứ 20, logout và việc key có TTL. Test hiện xác nhận TTL được gắn (`PTTL > 0`), **không chờ key hết hạn thật**. Nhánh `TIMEOUT` chưa được test; muốn test nhanh và ổn định nên tách phép tính thời gian/timeout ra phần logic thuần.

### Tóm tắt Chương 6
JWT cho biết "bạn là ai", Redis session cho biết "phiên này còn hiệu lực không và ứng với `users.id` nào". Mỗi request có đúng một lệnh Redis.

### Lưu ý
1. Timeout được kiểm tra theo giây và TTL đặt đúng `timeout_mins * 60`; timestamp trong hash vẫn chỉ có độ chính xác một giây. **Implementation hiện tại so sánh `elapsed_secs > timeout_secs` (strictly greater), không phải `>=`.** Redis thường tự xóa key khi hết TTL, vì vậy request sau thời gian idle thường thấy `NOT_FOUND`; nhánh `TIMEOUT` là lớp bảo vệ khi key còn tồn tại và chưa được test riêng.
2. **[Đã sửa]** Key từng không có TTL nên user đăng nhập rồi không quay lại để lại key vĩnh viễn. Giờ có TTL trượt.
3. **[Đã sửa]** Khai báo key sai vị trí (`1,1,1`) và cờ `deny-oom` trên `logout`.
4. **Một session active cho mỗi user:** key theo `unique_id`; login mới ghi `session_id` mới nên JWT cũ bị `STALE`. Đây không phải session đa thiết bị.
5. Logout bằng JWT stale không xóa session mới; chỉ đúng `session_id` hiện tại mới có thể xóa key.
6. `ConnectionManager` là **toàn cục theo process**: địa chỉ Redis của lần gọi đầu thắng, các `address` về sau bị bỏ qua.
7. Cache chạy không volume và không persistence: restart container Redis = mất mọi session (user phải đăng nhập lại).
8. `clean_db.sh` chỉ chạy với DB localhost, xóa các key `user_session_*` trước khi truncate và **không reset sequence** (`CONTINUE IDENTITY`). Tuy nhiên, thông báo xác nhận trong script vẫn ghi “Sequences will restart” — đây là text sai lệch so với SQL thực thi. Restore/reset DB thủ công vẫn phải giữ đồng bộ identity giữa auth DB, to-do DB và Redis.
9. `last_interacted` do **từng node Redis** tự tính bằng đồng hồ của nó; nếu sau này bật replica, lệnh được replay trên replica với đồng hồ của replica **[suy luận]**.

### Tự kiểm tra
1. Điều gì xảy ra ở request hợp lệ thứ 20, và vì sao kernel phải tra lại user?
2. Vì sao `update` vẫn gọi `update_last_interacted` khi trả `REFRESH`?
3. Vì sao TTL của Redis được gia hạn sau mỗi request hợp lệ, và điều gì xảy ra khi key tự hết hạn trước request tiếp theo?

---

## Chương 7 — To-do nanoservice

### 7.1 `to-do-dal`

**Lịch sử migration** (đọc theo thứ tự để thấy hệ thống tiến hóa):

| File | Thay đổi |
|---|---|
| `20261111144523_initial-setup.sql` | `to_do_items(id SERIAL, title VARCHAR(255) UNIQUE NOT NULL, status VARCHAR(7))` |
| `20261118002629_adding_user_connection.sql` | bảng `user_connections(user_id INTEGER, to_do_id INTEGER, PK(user_id,to_do_id))` |
| `20261118010000_user_connections_use_auth_unique_id.sql` | `user_id` đổi sang `TEXT`; thêm FK `to_do_id → to_do_items(id) ON DELETE CASCADE` |
| `20261121104146_make_title_not_global_unique.sql` | bỏ ràng buộc UNIQUE của `title`; thêm `CHECK (status IN ('PENDING','DONE'))`; thêm index trên `user_connections(to_do_id)` |
| `20261125074738_owner_on_items.sql` | **thêm `user_id` và `created_at` thẳng vào `to_do_items`**, chuyển dữ liệu từ `user_connections` sang, xóa các item mồ côi, đặt `user_id NOT NULL`, tạo index `(user_id, id)`, rồi `DROP TABLE user_connections` |

Lý do migration cuối: mỗi item chỉ thuộc **một** user, nên bảng nối nhiều-nhiều chỉ thêm join và thêm cách để sai. Mô hình dữ liệu hiện tại:

```text
to_do_items
-----------
id          SERIAL PK
title       VARCHAR(255)
status      VARCHAR(7) CHECK (status IN ('PENDING','DONE'))
user_id     TEXT NOT NULL          -- users.id (số) dạng chuỗi; không có FK vì auth nằm ở DB khác
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
INDEX to_do_items_user_id_idx (user_id, id)
```

**Ba kiểu mục `ToDoItem`:** `ToDoItem` (đọc ra), `NewToDoItem {title,status}` (tạo), `UpdateToDoStatus {id,status}` (đổi trạng thái). `AllTodoItems { pending, done }` gom nhóm theo trạng thái. Các câu SELECT chỉ lấy `id, title, status` nên cột mới không ảnh hưởng JSON trả ra.

**Câu lệnh SQL quan trọng:**
- **Tạo:** một câu `INSERT INTO to_do_items (title, status, user_id) ... RETURNING id, title, status`. Một câu lệnh đã atomic nên **không còn cần transaction**.
- **Đọc:** `WHERE user_id = $1 ORDER BY id LIMIT 1000` (giới hạn cứng, chưa có phân trang); `get_by_id` thêm `AND id = $2`.
- **Sửa/Xóa:** `UPDATE/DELETE ... WHERE id = $1 AND user_id = $2 RETURNING ...`. Kiểm tra **quyền sở hữu nằm ngay trong câu lệnh**; không khớp → `fetch_optional` ra `None` → `404`.

`TaskStatus` (`enums.rs`): `Done`/`Pending`; serde `rename_all = "UPPERCASE"` (JSON dùng `"DONE"`/`"PENDING"`); `FromStr` thì **không phân biệt hoa thường**; `Display` luôn in dạng chuẩn.

### 7.2 `to-do-core`
Chỉ làm hai việc: validate và ủy quyền. Duy nhất `create` có luật: `title.trim()` không rỗng và **≤ 255 ký tự** (đếm ký tự), sai → 400 `"invalid title"`. `get_all` bọc kết quả bằng `AllTodoItems::from_vec`.

### 7.3 `to-do-axum-server`

| Method | Path | Handler | Mã thành công | Body trả về |
|---|---|---|---|---|
| GET | `/api/v1/get` | `get_all` | 200 | `{pending:[...], done:[...]}` |
| GET | `/api/v1/get/{id}` | `get_by_id` | 200 | một `ToDoItem` |
| POST | `/api/v1/create` | `create` | **201** | **toàn bộ danh sách** (không chỉ item mới) |
| PATCH | `/api/v1/update` | `update` | 200 | toàn bộ danh sách |
| DELETE | `/api/v1/delete/{id}` | `delete_by_id` | 200 | toàn bộ danh sách |

Quy ước "mutation trả về danh sách mới" giúp frontend chỉ cần `setData(response.data)` sau mỗi thao tác, không phải gọi lại GET.

**`AuthUser` extractor:** `HeaderToken` → `GetUserSession` → `session.user_id` (số `i32`) → **`.to_string()`** → đây chính là `owner_key` được ghi vào `to_do_items.user_id`.

**Ba hàm dựng router** (`basic_actions/mod.rs`), cùng bọc một `router(store, user_session)` chung:
- `basic_actions_router_with_session(store, session)`: dùng cho test, inject session giả.
- `basic_actions_router_core_postgres(store, auth_store)`: ingress và standalone mặc định.
- `basic_actions_router_http(store)`: chế độ microservice.

`main.rs` standalone chọn nhánh bằng `#[cfg(feature)]`: nhánh `auth-core-postgres` **mở thêm pool `AUTH`** (khóa DB auth), nhánh `auth-http` thì không.

### Tóm tắt Chương 7
To-do không có khái niệm "user" riêng: nó chỉ nhận một chuỗi `owner_key` từ tầng trên, lưu vào cột `user_id` của mỗi item và điều kiện hóa mọi câu SQL theo cột đó.

### Lưu ý
- **`owner_key` là `users.id` (số, dạng chuỗi), không phải `unique_id`** dù tên migration `..._use_auth_unique_id` nói ngược lại. Hệ quả: nếu DB auth bị reset/khôi phục mà DB to-do không, id có thể được tái sử dụng và user mới thừa hưởng to-do của user cũ.
- **[Đã sửa]** `user_connections` từng là bảng nhiều-nhiều trên giấy nhưng code chỉ dùng quan hệ 1 user–N item; giờ là cột `user_id`.
- `TaskStatus` khi deserialize JSON **phân biệt hoa thường**: `{"status":"done"}` bị axum từ chối với 422 (lỗi do `Json` extractor, không đi qua `NanoServiceError`).
- Chưa có phân trang; `GET /get` trả tối đa 1000 item.
- Thao tác trên item của người khác trả **404 (không phải 403)**: không lộ sự tồn tại của item.
- Có test trên **PostgreSQL thật** chứng minh user A không đọc/sửa/xóa được item của user B (trước đây chỉ test bằng store giả trong bộ nhớ).

### Tự kiểm tra
1. Vì sao câu `UPDATE` đặt điều kiện `user_id` trong `WHERE` thay vì đọc ra rồi mới sửa?
2. Vì sao sau migration mới, tạo item không còn cần transaction?

---

## Chương 8 — Ingress: ráp tất cả vào một process

**Mục tiêu:** hiểu `ingress/src/main.rs`, nơi duy nhất biết *kiểu cụ thể* (Postgres, Redis) và gắn mọi mảnh lại với nhau.

### 8.1 Vai trò: "composition root"
Các crate `core`/`dal`/`networking` chỉ biết trait. `ingress` là chỗ chọn implementation thật:

- store to-do = `to_do_dal::...::SqlxPostgresDescriptor` (Postgres DB `to_do`);
- store auth = `auth_dal::...::SqlxPostgresDescriptor` (Postgres DB `auth`);
- session = `RedisSessionDescriptor<AuthPostgresDescriptor>` (backend `core-postgres`, nên refresh đọc thẳng DB auth, không qua HTTP).

Vì vậy một request từ browser đi **hoàn toàn trong process**: không có hop mạng nội bộ nào giữa "auth" và "to-do" (chỉ có hop tới Postgres và Redis).

### 8.2 Thứ tự khởi động (`main`)
```text
dotenvy::dotenv()            đọc .env (nếu có)
init_logger()                tracing, mức INFO
HeaderToken::validate_secret()   JWT_SECRET >= 32 ký tự, không phải giá trị mẫu
auth_kernel::cache_api_url()     có CACHE_API_URL (chưa kiểm Redis sống)
pool_from_env("TO_DO") -> run_todo_migrations     lần kết nối DB đầu tiên
pool_from_env("AUTH")  -> run_auth_migrations
bind INGRESS_WEB_HOST:INGRESS_WEB_PORT   (mặc định 0.0.0.0:8001)
axum::serve(app.into_make_service_with_connect_info::<SocketAddr>())
      .with_graceful_shutdown(Ctrl-C hoặc SIGTERM)
```
Cấu hình sai (thiếu secret, DB không tới được) làm process **thoát ngay lúc khởi động** thay vì chạy dở. `into_make_service_with_connect_info` là bắt buộc để handler/extractor biết IP của kết nối TCP (rate limit cần nó).

### 8.3 Bảng route

| Đường | Xử lý | Ghi chú |
|---|---|---|
| `GET /` | `index` | trả `INDEX_HTML` (`include_str!` lúc biên dịch) |
| `GET /healthz` | closure → `200` | không cần xác thực; ALB và `run_deployment.sh` dùng |
| `GET /api/v1/get`, `/get/{id}`, `POST /create`, `PATCH /update`, `DELETE /delete/{id}` | router to-do | cần `Bearer` (Chương 7) |
| `POST /api/v1/auth/login`, `/auth/logout` | `auth_router` | **chung một bucket** rate limit |
| `POST /api/v1/users/create` | `create_router` | bucket riêng |
| `GET /api/v1/users/me` | `get_router` | không rate limit |
| mọi đường khác | `catch_all` | xem 8.4 |

### 8.4 Phục vụ frontend từ trong binary
- `#[derive(RustEmbed)] #[folder = "../frontend/public"] #[exclude = "*.map"]`: toàn bộ thư mục `public` (bundle, wasm, css) được **nhúng vào binary lúc biên dịch**. Ở build release không cần file nào trên đĩa. (Ở build debug, rust-embed mặc định đọc từ đĩa lúc chạy **[kiến thức chung về crate]**.)
- Hệ quả quan trọng: **phải build frontend trước khi build ingress** (`ingress/scripts/run_server.sh` và `Dockerfile` đều làm đúng thứ tự). Quên bước này thì binary nhúng frontend cũ hoặc thiếu.
- `catch_all(uri, headers)`:
  1. đường bắt đầu `api/` → 404 (API không tồn tại không được rơi vào SPA);
  2. file có trong embed → trả file, kèm `Content-Type` đoán theo đuôi (`mime_guess`), `Cache-Control: no-cache` và `ETag` (băm sha256 của nội dung). Trình duyệt gửi lại `If-None-Match` và nhận **304** nếu chưa đổi, nên không tải lại bundle mỗi lần;
  3. đường có dấu `.` mà không tìm thấy (ví dụ `/missing.js`) → 404;
  4. còn lại (ví dụ `/tasks/42`) → trả `index.html` (**SPA fallback**, để reload trang ở route phía client vẫn chạy).

### 8.5 Rate limit
Dùng `tower_governor` (thuật toán token bucket) qua hàm `rate_limited(router, per_second, burst_size)`:

| Nhóm | `per_second` | `burst_size` | Nghĩa thực tế |
|---|---|---|---|
| `auth_router` (login + logout) | 12 | 5 | burst 5 request, sau đó **1 request mỗi 12 giây** |
| `create_router` (đăng ký) | 3600 | 3 | burst 3, sau đó **1 request mỗi giờ** |

- `per_second(n)` là *chu kỳ nạp một token* (n giây/token), không phải "n request mỗi giây". Đây là chỗ rất dễ đọc nhầm.
- Mỗi lần gọi `rate_limited` tạo **một limiter riêng**, nên login và logout dùng chung bucket (cùng một router), đăng ký có bucket khác. Logout tốn token của login.
- Khi vượt hạn mức: `429` kèm `retry-after` và các header `x-ratelimit-*` (`use_headers()`).
- Mỗi limiter có một task nền gọi `retain_recent()` mỗi 60 giây để dọn bộ nhớ của các IP đã cũ.

**Khóa theo "IP khách": `ClientIp` + `TRUST_PROXY`.** Extractor tùy biến quyết định IP nào đại diện cho client:

- `TRUST_PROXY` không đặt hoặc khác `1`: dùng **địa chỉ TCP của peer** (`ConnectInfo<SocketAddr>`).
- `TRUST_PROXY=1`: ưu tiên header `X-Real-IP`; thiếu hoặc không parse được thì quay về peer.

Vì sao cần? Sau Nginx/ALB, peer luôn là địa chỉ proxy, nên **mọi người dùng chung một bucket**: 5 lần login cho cả thế giới rồi mỗi 12 giây một lần, và chỉ 3 lượt đăng ký mỗi giờ. `X-Real-IP` do Nginx production *ghi đè* bằng IP thật (Chương 13), nên chỉ bật `TRUST_PROXY=1` ở nơi ingress **không thể bị truy cập trực tiếp** từ ngoài; nếu không, client tự đặt `X-Real-IP` để né giới hạn. Cố ý **không** dùng `X-Forwarded-For` (phần bên trái do client kiểm soát).

### 8.6 Chuỗi middleware
Trong axum, `.layer(...)` thêm sau nằm **ngoài** lớp thêm trước. Thứ tự trong code (trong → ngoài):

```text
request ──► [CORS: chỉ build debug] ► [CSP header] ► [Compression gzip/br] ► [Timeout 30s -> 408] ► [Trace] ► route
```

- **Timeout 30 giây** trả `408`.
- **Nén** (gzip, brotli) cho text/js/wasm/json. Nginx production không nén lần nữa để tránh nén hai lần.
- **CSP** (chỉ đặt nếu handler chưa đặt): `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`. Ý nghĩa: chỉ chạy script cùng origin, cho phép biên dịch WebAssembly (`wasm-unsafe-eval` là điều kiện để module WASM chạy), cấm nhúng iframe, cấm plugin.
- **CORS** chỉ tồn tại khi build debug (`cfg(debug_assertions)`), cho origin `FRONTEND_DEV_ORIGIN` (mặc định `http://localhost:3000`). Build release **không có CORS** vì frontend cùng origin.

### 8.7 Test của ingress (4 test functions)
Dựng cả `app()` với pool `connect_lazy` (không cần DB thật) và chèn `ConnectInfo` giả:
1. `serves_the_frontend_shell_and_assets`: `/healthz`, `/`, route SPA `/tasks/42`, `/bundle.css` (đúng MIME), file thiếu → 404.
2. `routes_api_requests_and_returns_404_for_unknown_api_paths` (chỉ debug): API không token → 401; đường API lạ → 404; preflight CORS từ `localhost:3000` được chấp nhận.
3. `logout_requires_a_valid_token_header`: thiếu/sai token → 401.
4. `auth_routes_are_rate_limited_by_peer_ip`: 5 lần đầu `401` (không có credential), lần thứ 6 `429` kèm `retry-after`.

### Tóm tắt Chương 8
`ingress` là chỗ ráp: chọn implementation thật, nhúng frontend, gắn rate limit, CSP, nén, timeout; và là nơi duy nhất biết "IP khách" là gì.

### Lưu ý
- **Phải build frontend trước ingress**, nếu không binary mang frontend cũ.
- `TRUST_PROXY=1` **chỉ an toàn** khi proxy ghi đè `X-Real-IP` và ingress không mở ra ngoài. Sai chỗ này vừa hỏng rate limit vừa mở đường né giới hạn.
- Login và logout chung bucket; user logout nhiều lần có thể làm mình bị `429` ở lần login kế.
- Test rate limit dùng IP peer giả (`127.0.0.1`), chưa có test cho nhánh `TRUST_PROXY=1`.
- `/healthz` chỉ chứng minh process sống, **không** kiểm tra Postgres/Redis.

### Tự kiểm tra
1. Vì sao `/api/v1/xyz` trả 404 còn `/tasks/42` trả `index.html`?
2. Nếu quên `into_make_service_with_connect_info`, điều gì hỏng?
3. Bật `TRUST_PROXY=1` mà ingress mở thẳng ra Internet thì kẻ tấn công làm được gì?

---

## Chương 9 — Frontend: Bun + esbuild + WASM + React

**Mục tiêu:** hiểu cách một SPA TypeScript, một hàm Rust-WASM và API Rust nói chuyện với nhau.

### 9.1 Chuỗi build
```text
frontend/rust-interface (Rust)  --wasm-pack build --target web-->  pkg/rust_interface.js + _bg.wasm
                                 cp _bg.wasm  ->  frontend/public/rust_interface_bg.wasm
frontend/src/index.tsx (TS/React) --esbuild-->  frontend/public/bundle.js (+ bundle.css)
                 mọi thứ trong public/  --rust-embed (lúc biên dịch ingress)-->  binary ingress
```
Các script (`package.json`): `bun run wasm` (bước 1), `bun run build` (esbuild), `bun run typecheck` (`tsc --noEmit`), `bun test`, `bun run serve` (dev server tĩnh cổng 3000), `wasm-trace` (xuất `.wat` để soi).

`esbuild.js`: entry `src/index.tsx`, `bundle`, định dạng **ESM**, `minify`, và hai `define` biên dịch:
- `process.env.NODE_ENV = "production"`;
- `__INGRESS_URL__` = `INGRESS_URL` (mặc định `http://localhost:8001`), chỉ dùng khi trang chạy ở `localhost:3000`.

`index.html` rất mỏng: `<div id="root">`, link `/bundle.css`, `<script type="module" src="/bundle.js">`.

### 9.2 WASM: `rust-interface`
Một hàm duy nhất:
```rust
#[wasm_bindgen]
pub fn rust_generate_button_text(status: &str) -> String   // "PENDING" -> "done", "DONE" -> "delete", khác -> "an error has occurred"
```
So sánh không phân biệt hoa thường. `index.tsx` import `init` và hàm này từ `rust-interface/pkg/rust_interface.js`; `init()` tải `rust_interface_bg.wasm` (file bạn copy vào `public/`). Trước khi `init()` xong, ứng dụng hiển thị "Loading..." vì các nút cần hàm này. Chạy được là nhờ CSP có `wasm-unsafe-eval`.

Đây chủ yếu là **bài tập tích hợp Rust↔browser**: logic quá nhỏ để cần WASM, nhưng nó chứng minh toàn bộ chuỗi công cụ.

### 9.3 `Url`: biết API ở đâu
`Url.getBaseUrl()`: nếu `window.location.origin` chứa `localhost:3000` thì dùng `__INGRESS_URL__`; ngược lại dùng **chính origin** của trang. Vì thế bản build production không cần cấu hình URL: ingress (hay Nginx/ALB) đã phục vụ cả frontend lẫn API cùng origin. `Url` còn dựng `create`, `getAll`, `update`, `login`, `logout`, `deleteUrl(id)`, `getByIdUrl(id)`.

### 9.4 Lớp gọi API (`api/utils.ts`)
- `ApiResult<X> = {status, data} | {status, error, data?}`: thành công hay thất bại là một **union** có thể phân biệt bằng `"error" in response`.
- `handleRequest` dùng axios với `validateStatus: () => true` (không ném lỗi theo mã HTTP) và so với **mã mong đợi** của từng lời gọi (`201` cho create, `204` cho logout...). Khác mã mong đợi → trả `{status, error: "expected status X got Y", data}`. Lỗi mạng → `{status: 500, error}`.
- Mỗi request tự thêm `Authorization: Bearer <token>` lấy từ `localStorage["token"]`.
- `login.ts` khác: tự dựng `Authorization: Basic base64(email:password)` (mã hóa UTF-8 trước khi `btoa`) và trả chuỗi JWT.

### 9.5 `App` (index.tsx): máy trạng thái
| State | Ý nghĩa |
|---|---|
| `loggedin` | khởi tạo từ việc có `token` trong `localStorage` |
| `data` | `{pending, done}` lấy từ API |
| `wasmReady` | `init()` WASM xong chưa |
| `error` / `notice` | lỗi toàn trang / thông báo thao tác |

Hai `useEffect`: (1) khởi tạo WASM một lần; (2) khi `wasmReady && loggedin` mới gọi `getAll()`. Cờ `cancelled` ngăn cập nhật state sau khi component đã unmount.

Quy ước quan trọng: **mọi mutation trả về cả danh sách mới** (Chương 7), nên `reRenderItems(response)` chỉ cần `setData(response.data)`. Nếu `status === 401` ở bất kỳ lời gọi nào → `handleUnauthorized()` xóa token và quay về màn login. Đây là nơi hết hạn session (Chương 6) hiện ra phía người dùng.

Component:
- `LoginForm`: gọi `login`, lưu token, báo lỗi (có thông báo riêng cho `429`).
- `CreateItemForm`: khóa nút khi đang gửi (`busy`), `trim` tiêu đề, chỉ xóa ô nhập khi thành công.
- `ToDoItem`: nút đổi hành động theo trạng thái: `PENDING` → gọi `update` sang `DONE`; `DONE` → gọi `delete`. Nhãn nút do WASM sinh.

### 9.6 Kiểu dữ liệu và test
`interfaces/toDoItems.ts`: `enum TaskStatus {PENDING, DONE}`, `NewToDoItem`, `ToDoItem`, `ToDoItems {pending, done}` khớp với JSON server.

`bun test` có 7 test trong hai file: `api-url.test.js` (3 test: URL của create/list/update, id nguyên trong path, dùng origin hiện tại ngoài dev server) và `api-utils.test.js` (4 test: gửi token + header + body, trả status/payload khi lệch mã, đổi exception mạng thành kết quả lỗi, patch gửi payload). Chưa có test cho component React.

### Tóm tắt Chương 9
Một SPA nhỏ, build bằng Bun + esbuild, nhúng vào binary Rust; một hàm WASM làm cầu nối chuỗi công cụ; mọi giao tiếp API đi qua một lớp mỏng trả `ApiResult` và xử lý 401 ở một chỗ.

### Lưu ý
- **JWT nằm trong `localStorage`**: JS cùng origin đọc được. CSP chặt giảm rủi ro XSS nhưng không loại bỏ. Cookie `HttpOnly` an toàn hơn (nhưng kéo theo bài toán CSRF).
- **Lỗi UX:** khi session đã hết hạn, bấm Logout nhận `401`; code hiện `alert("Logout failed…")` và **không xóa token**. Người dùng vẫn thấy giao diện đã đăng nhập cho tới khi một request khác trả 401 (hoặc reload).
- `__INGRESS_URL__` được "đóng đinh" lúc build; build image Docker nhận nó qua build-arg `INGRESS_URL`.
- `esbuild ^0.24`, `package name es_react`, `index.html` title mặc định là dấu vết của template ban đầu (chỉ thẩm mỹ).
- `bun run wasm` cần `wasm-pack` và target `wasm32-unknown-unknown`; thiếu thì build frontend lỗi trước cả khi tới esbuild.

### Tự kiểm tra
1. Vì sao `validateStatus: () => true` hữu ích cho thiết kế `ApiResult`?
2. Điều gì xảy ra nếu bạn build ingress trước khi chạy `bun run wasm`?
3. Vì sao bản production không cần biết URL của API?

---

## Chương 10 — Luồng end-to-end

**Mục tiêu:** theo dấu từng kịch bản qua *toàn bộ* các tầng đã học. Ký hiệu: `B` browser, `N` nginx/ALB, `I` ingress, `K` kernel, `R` Redis, `P` Postgres.

### 10.1 Đăng ký
```text
`scripts/create_user.sh` (xh) / client:  POST /api/v1/users/create {email, password}
 I: rate limit (bucket đăng ký: burst 3, 1/giờ theo IP khách)
 I: create_router -> auth-core users::create
      email: trim + lowercase, <=254, có '@' ; mật khẩu 8–128 ký tự   (sai -> 400)
      Argon2 băm trong spawn_blocking (muối ngẫu nhiên)
      auth-dal SaveOne: INSERT users(email, password, unique_id=UUIDv4) RETURNING
 P: trùng email (unique index trên lower(email)) -> db_error -> 409
 <- 201, không body
```

### 10.2 Đăng nhập
```text
B: POST /api/v1/auth/login   Authorization: Basic base64(email:password)
 I: rate limit (bucket login/logout)
 I: Credentials extractor: tách tại dấu ':' đầu tiên
 I: auth-core login: chuẩn hóa email -> GetByEmail
      không có user -> verify_dummy_password (tốn CPU như thật) -> 401 "invalid email or password"
      có user       -> verify_password (spawn_blocking); sai -> cùng 401
      đúng          -> sinh session_id UUID v4; JWT HS256 { unique_id, session_id, exp=+24h }
 I: LoginUserSession -> R: session.login unique_id session_id 20 users.id
      (counter=0, TTL 20 phút; login mới thay session_id cũ)
 <- 200  "<jwt>"   (JSON string)
B: localStorage["token"] = jwt ; setLoggedin(true) -> getAll()
```

### 10.3 Một request có đăng nhập (tạo item)
```text
B: POST /api/v1/create  Bearer <jwt>  {"title":"viết tài liệu","status":"PENDING"}
 I: HeaderToken: giải mã JWT (HS256, exp, session_id)            sai -> 401
 I: AuthUser -> GetUserSession -> K -> R: session.update <unique_id> <session_id>
      NOT_FOUND / TIMEOUT / STALE -> 401  Ok(id) -> tiếp       REFRESH -> xem 10.4
 I: to-do-core create: title.trim() 1..255 ký tự                  sai -> 400 "invalid title"
 I: to-do-dal SaveOne: INSERT to_do_items(title,status,user_id=id.to_string()) RETURNING
 I: to-do-core get_all: SELECT ... WHERE user_id=$1 ORDER BY id LIMIT 1000
 <- 201 {"pending":[...],"done":[...]}
```

### 10.4 Request thứ 20: `REFRESH`
```text
R: session.update -> counter=20 >= 20 -> "REFRESH"
K: tra user:  core-postgres -> auth-core get_by_unique_id (DB)
                http -> GET AUTH_API_URL/api/v1/users/me (cũng kiểm tra session Redis)
      user đã bị xóa -> request bị từ chối; session không được refresh thành công
K: R: session.login unique_id session_id 20 users.id (giữ session_id, counter=0)
 I: tiếp tục xử lý request bình thường
```
Mục đích: phát hiện user bị xóa/đổi sau tối đa 20 request hợp lệ mà không phải truy vấn DB mỗi request.

### 10.5 Đổi trạng thái và xóa
`PATCH /api/v1/update {id, status:"DONE"}` → `UPDATE ... WHERE id=$1 AND user_id=$2 RETURNING`; `DELETE /api/v1/delete/{id}` → `DELETE ... WHERE id=$1 AND user_id=$2 RETURNING`. Không khớp (id không tồn tại *hoặc* của người khác) → cùng một kết quả: **404**.

### 10.6 Logout
```text
B: POST /api/v1/auth/logout  Bearer <jwt>
 I: HeaderToken -> LogoutUserSession -> R: session.logout <unique_id> <session_id>
      khớp phiên hiện tại -> xóa key -> OK; key không có -> NOT_FOUND; session khác -> STALE
 <- 204 khi OK/NOT_FOUND; STALE -> 401 (không xóa session login mới hơn)
B: xóa token khỏi localStorage
sau đó: dùng lại JWT -> NOT_FOUND -> 401 (JWT còn hạn nhưng session không còn)
```

### 10.7 Hết hạn session
Sau **20 phút không có request hợp lệ**, Redis hết TTL và xóa key. Request kế tiếp thường gặp `NOT_FOUND` (hoặc `TIMEOUT` nếu key vẫn còn) → 401 → `handleUnauthorized()` → màn login. Đăng nhập lại tạo `session_id` mới.

### 10.8 Bị rate limit
Vượt bucket → `429` + `retry-after`. `LoginForm` hiện "Too many attempts. Please wait a minute…". Phía production, Nginx còn giới hạn riêng (30 request/phút/IP, burst 10) trên `/api/v1/auth/*` và `/api/v1/users/*`.

### 10.9 Layout microservice
Browser → Nginx frontend: tĩnh cho `/`, `/api/v1/auth/*` và `/api/v1/users/*` → `auth:8081`, còn lại dưới `/api/v1/` → `to_do:8080`. Server to-do build với `auth-http`: ở `REFRESH` nó **tự ký JWT nội bộ** chứa `unique_id` và `session_id` hiện tại rồi gọi `GET AUTH_API_URL/api/v1/users/me`. Endpoint `/users/me` xác thực cả JWT lẫn session Redis. Đường đi khác, nhưng logic lookup vẫn dùng chung kernel/core/DAL.

### Tóm tắt Chương 10
Mọi kịch bản đều là tổ hợp của vài bước: *giải mã JWT → hỏi Redis → (có thể refresh) → logic nghiệp vụ → SQL có điều kiện sở hữu*.

### Tự kiểm tra
1. Vẽ lại luồng khi user A gọi `DELETE /delete/5` trong khi item 5 thuộc user B. Mỗi tầng trả gì?
2. Giữa hai lần `REFRESH`, nếu một user bị xóa khỏi DB auth thì tối đa bao nhiêu request nữa họ còn dùng được?

---

## Chương 11 — Dữ liệu và định danh

**Mục tiêu:** phân biệt mọi loại "ID" trong hệ thống để không bao giờ nhầm `users.id` với `unique_id` nữa.

### 11.1 Bảng các định danh

| Tên | Kiểu | Sinh ở đâu | Nằm ở đâu | Dùng để |
|---|---|---|---|---|
| `users.id` | `i32` (`SERIAL`) | Postgres lúc INSERT | auth DB; `perm_user_id` trong Redis; `to_do_items.user_id` (dạng chuỗi) | khóa thật của user; **chủ sở hữu item** |
| `users.unique_id` | UUID v4 (chuỗi) | `NewUser::new` | auth DB; claim `unique_id` trong JWT; phần tên key Redis | định danh **công khai/ổn định** của user |
| `session_id` | UUID v4 (chuỗi) | mỗi lần login | claim `session_id` trong JWT; field Redis `session_id` | định danh **một lần đăng nhập**; login mới thu hồi JWT cũ |
| JWT | chuỗi HS256 | login | `localStorage["token"]` phía browser | chứng minh user `unique_id` đang dùng phiên `session_id` này (24 giờ) |
| Redis key | `user_session_<unique_id>` | `session.login` | Redis | hash chứa phiên active hiện tại |
| `perm_user_id` | số | `session.login` (tham số) | field trong hash Redis | ánh xạ nhanh `unique_id → users.id` |
| `to_do_items.id` | `i32` (`SERIAL`) | Postgres | to-do DB; URL `/get/{id}`, `/delete/{id}` | khóa của item |
| `to_do_items.user_id` | `TEXT` | `AuthUser` (`users.id.to_string()`) | to-do DB, có index | chủ sở hữu item |

### 11.2 Hai database, một "khóa ngoại tưởng tượng"

```text
AUTH DB (auth)                          TO-DO DB (to_do)
users                                   to_do_items
-----                                   -----------
id          SERIAL PK  ───────────────► user_id  TEXT   (chỉ là giá trị, KHÔNG có FK)
email       UNIQUE (lower(email))       id, title, status, created_at
password    (Argon2)
unique_id   UUID UNIQUE
```

Vì hai DB tách rời, Postgres **không** thể bảo đảm `to_do_items.user_id` trỏ tới user có thật. Tính toàn vẹn dựa vào: (1) `user_id` chỉ đến từ session hợp lệ; (2) không có chức năng xóa user. Nếu sau này thêm "xóa tài khoản", bạn phải tự xóa item của user ở DB to-do.

### 11.3 Cái gì lưu ở đâu

| Dữ liệu | Nơi lưu | Vòng đời |
|---|---|---|
| Tài khoản, mật khẩu băm | Postgres `auth` | bền vững |
| Item to-do | Postgres `to_do` | bền vững |
| Session đăng nhập | Redis (hash + TTL) | tối đa 20 phút sau lần dùng cuối; mất khi Redis restart |
| JWT | `localStorage` của browser | tới 24 giờ hoặc tới khi logout/401 |
| Giới hạn tốc độ | bộ nhớ của process ingress | mất khi ingress restart |

### 11.4 Quy ước giá trị
- `status` trong DB: `'PENDING'` hoặc `'DONE'` (ràng buộc `CHECK`); trong JSON cũng viết hoa.
- Email luôn được lưu dạng đã chuẩn hóa (`trim` + chữ thường), và có unique index trên `lower(email)`.
- Thời gian trong Redis: chuỗi `YYYY-MM-DD HH:MM:SS` UTC.

### Tóm tắt Chương 11
`unique_id` là danh tính đi trong token và tên key Redis; `users.id` là khóa dữ liệu thật và là chủ sở hữu item. Redis giữ cầu nối giữa hai thứ.

### Lưu ý
- `clean_db.sh` chỉ cho phép URL PostgreSQL localhost, xóa `user_session_*` trong Redis trước khi truncate cả hai DB và dùng `CONTINUE IDENTITY` để không tái sử dụng `users.id`. Nếu reset/restore DB thủ công hoặc chỉ khôi phục một database, vẫn có thể làm lệch owner mapping giữa auth DB, to-do DB và Redis.
- `to_do_items.id` là số tuần tự, dễ đoán; an toàn nhờ điều kiện `user_id` trong SQL, không nhờ việc id khó đoán.

### Tự kiểm tra
1. Vì sao không thể đặt `FOREIGN KEY` từ `to_do_items.user_id` tới `users.id`?
2. Nếu đổi `owner_key` sang `unique_id`, phải sửa những chỗ nào và dữ liệu cũ xử lý ra sao?

---

## Chương 12 — Cấu hình

**Mục tiêu:** biết biến môi trường nào do ai đọc, đọc lúc nào, và vì sao cấu hình sai dễ gây lỗi khó hiểu.

### 12.1 Bốn cách nạp cấu hình
| Cơ chế | Ai dùng | Đặc điểm |
|---|---|---|
| `dotenvy::dotenv()` | các binary Rust | đọc `.env` ở thư mục chạy; biến đã có trong môi trường được ưu tiên; hỗ trợ `${VAR}` |
| `source .env` trong bash | `init.sh`, `test.sh`, `clean_db.sh`, `run_server.sh` | chạy như shell, tuân quy tắc quote của bash |
| `docker compose --env-file .env` | các Compose fragment | cú pháp của Compose |
| parser riêng (không `source`) | `deployment/run_deployment.sh` đọc `.env.production` | chỉ chấp nhận dòng `KEY=value` và danh sách khóa cho phép; không thực thi |

Ba bộ parser đầu có quy tắc quote/escape **khác nhau**, nên giữ giá trị đơn giản. Mọi secret nên sinh bằng `openssl rand -hex …` (chỉ chứa `0-9a-f`), vì mật khẩu được nhúng vào URL; ký tự `/ + =` của base64 làm hỏng URL.

### 12.2 Bảng biến

| Biến | Ai đọc | Ghi chú |
|---|---|---|
| `JWT_SECRET` | `glue::token` (mọi server); to-do ở chế độ `auth-http` dùng để tự ký JWT | cache trong `OnceLock`; ≥ 32 ký tự; mọi process phải **cùng giá trị** |
| `TO_DO_DB_URL`, `TO_DO_MAX_CONNECTIONS` | `pool_from_env("TO_DO")` | URL bắt buộc; pool mặc định 5 |
| `AUTH_DB_URL`, `AUTH_MAX_CONNECTIONS` | `pool_from_env("AUTH")` | ingress, auth server, và to-do ở chế độ `auth-core-postgres` |
| `CACHE_API_URL` | `auth_kernel::cache_api_url()` | `redis://:PASSWORD@host:6379` |
| `AUTH_API_URL` | kernel backend `http` | base URL của auth server |
| `INGRESS_WEB_HOST/PORT` | ingress | mặc định `0.0.0.0:8001` |
| `TO_DO_WEB_HOST/PORT` | to-do server | mặc định `127.0.0.1:8080` |
| `AUTH_WEB_HOST/PORT` | auth server | mặc định `127.0.0.1:8081` |
| `TRUST_PROXY` | ingress | `1` = lấy IP khách từ `X-Real-IP` (Chương 8) |
| `FRONTEND_DEV_ORIGIN` | ingress (chỉ debug) | origin cho CORS |
| `INGRESS_URL` | `esbuild.js` (`__INGRESS_URL__`), `create_user.sh` | nơi dev frontend/helper gọi API |
| `POSTGRES_PASSWORD`, `REDIS_PASSWORD` | `compose.yaml`; nhúng vào các URL | phải khớp với mật khẩu trong URL |
| `TODO_DOCKER_DB_URL`, `AUTH_DOCKER_DB_URL`, `CACHE_DOCKER_URL`, `TODO_DOCKER_API_URL`, `AUTH_DOCKER_API_URL`, `INGRESS_DOCKER_API_URL` | các `build-config.yml`, Nginx template | dùng tên service Compose |
| `COMPUTE_PLATFORM` | Compose | `linux/amd64` hoặc `linux/arm64` |
| `DATABASE_URL` | `sqlx::test` | `test.sh` lấy từ `TO_DO_DB_URL` |
| `REDIS_PASSWORD` | local `clean_db.sh` | cần để xóa các key `user_session_*` trước khi reset DB; Redis `cache` phải đang chạy |
| `RUN_POSTGRES_INTEGRATION`, `TOKEN`, `TO_DO_WEB_PORT` | `basic_test.sh` | smoke test API |
| `.env.production`: `DOCKERHUB_USER`, `IMAGE_TAG`, `DOCKERHUB_TOKEN`, `JWT_SECRET`, `REDIS_PASSWORD`, `DB_USERNAME`, `DB_PASSWORD`, `APP_DB_PASSWORD`, `DOMAIN_NAME` | `run_deployment.sh` | quy tắc chi tiết ở `devops_handbook.md` §4.5 |

### 12.3 Hai điểm cần nhớ
1. **Đọc một lần.** `JWT_SECRET` và `CACHE_API_URL` được cache trong `OnceLock`; đổi giá trị giữa chừng không có tác dụng, phải restart process.
2. **Hai bộ giá trị tách bạch:** biến *không* có `DOCKER` dành cho process chạy trên máy bạn (`127.0.0.1`), biến `*_DOCKER_*` dành cho container (tên service như `postgres`, `cache`).

### Tóm tắt Chương 12
Một file `.env` cho dev, một `.env.production` cho AWS; Rust đọc qua `dotenvy`, script đọc qua `source` hoặc parser riêng, Compose qua `--env-file`.

### Lưu ý
- Quên `TRUST_PROXY=1` ở production → mọi người dùng chung một bucket rate limit; bật nhầm ở nơi ingress lộ trực tiếp → mở đường né rate limit.
- Mật khẩu trong URL và biến mật khẩu phải **đồng bộ**; Compose không tự suy ra.
- `JWT_SECRET` đổi → mọi token đã cấp vô hiệu → mọi người đăng nhập lại.

### Tự kiểm tra
1. Vì sao `.env.production` không được `source` trong script?
2. Hai process cần cùng biến nào để chạy được layout microservice?

---

## Chương 13 — Build, đóng gói, triển khai

**Mục tiêu:** hiểu *vì sao* chuỗi build/deploy được tổ chức như vậy. Hướng dẫn thao tác từng bước nằm ở `devops_handbook.md`; chương này giải thích mô hình.

### 13.1 Image `compute-unit` (root `Dockerfile`)
Một Dockerfile **đa kiến trúc** (chọn bằng `--platform`), ba stage:
1. `oven/bun` chỉ để lấy binary `bun`;
2. `rust:1.100-bookworm`: cài `wasm-pack` (ghim phiên bản), build frontend (`bun install --frozen-lockfile`, `bun run wasm`, `bun run build`), rồi build ba binary (`ingress`, `auth-axum-server`, `to-do-axum-server` với `--no-default-features --features auth-http`) và `strip`. Có **BuildKit cache mount** cho registry Cargo và thư mục `target`, nên build lại nhanh;
3. `gcr.io/distroless/cc-debian12:nonroot`: chỉ chứa glibc, libgcc, chứng chỉ CA, libssl; **không shell, không package manager, chạy user không phải root**. Mặc định chạy `ingress`. (Họ image `cc` đã kèm sẵn các thư viện runtime cần thiết **[kiến thức chung về distroless]**; nếu build báo thiếu OpenSSL thì cần thêm `libssl-dev` ở stage build.)

Vì sao distroless: bề mặt tấn công nhỏ, và không cần tự copy từng thư viện (image `cc` đã có sẵn). Cái giá là khó debug trực tiếp trong container (không có `sh`); dùng `docker compose logs`.

`cache-module/Dockerfile` build module Redis và đặt vào image `redis:7.4`.

### 13.2 Compose: ba tầng cấu hình
- `compose.yaml`: **nền** (Postgres 18 và Redis có module), dùng cho mọi layout. Postgres có volume, script tạo DB `auth` lần đầu, cổng chỉ bind `127.0.0.1`.
- `*/build-config.yml`: các **fragment** ghép thêm service (`ingress` + `nginx`; hoặc `auth`; `to_do`; `frontend`).
- `deployment/compose.yaml`: bản **production** (không có Postgres vì dùng RDS), image kéo từ Docker Hub, `TRUST_PROXY=1`, `cache` có healthcheck và ingress chỉ khởi động khi cache khỏe.

Hai layout local không chung được vì cùng chiếm cổng 80.

### 13.3 Ba cấu hình Nginx
| File | Nạp như thế nào | Vai trò |
|---|---|---|
| `deployment/nginx.conf` | **nguyên** `/etc/nginx/nginx.conf` (có `events {}` và `http {}`) | reverse proxy sau ALB: lấy IP thật, giới hạn auth, header bảo mật |
| `ingress/nginx.conf` | **template** → `conf.d/default.conf` (chỉ khối `server {}`) | TLS tự ký cho layout monolith local |
| `frontend/nginx.conf` | **template** → `conf.d/default.conf` | phục vụ tĩnh và định tuyến API cho layout microservice |

Template nằm trong `/etc/nginx/templates/`, image tự thay biến môi trường rồi ghi vào `conf.d/`, vốn đã nằm *trong* `http {}`. Đặt `events {}`/`http {}` vào template làm Nginx báo `"events" directive is not allowed here`.

Nginx production làm bốn việc: (1) `real_ip` tin ALB để lấy IP khách; (2) ghi đè `X-Real-IP`/`X-Forwarded-For` bằng giá trị sạch, nên `TRUST_PROXY=1` an toàn; (3) `limit_req` 30 request/phút trên auth/users; (4) HSTS và vài header an toàn. Nó resolve `ingress` lúc request (biến trong `proxy_pass` + `resolver` của Docker) nên container ingress đổi IP không cần restart Nginx.

### 13.4 Hai stack OpenTofu
**`builds/`**: một EC2 `m7i-flex.large` tạm thời để build image. `publish_images.sh` tạo máy (nếu chưa có), gửi source (bỏ `.env`, khóa `.pem`, state), build hai image bằng BuildKit, đẩy lên Docker Hub (`<user>/compute-unit:<tag>` và `redis-cache`). Máy **vẫn chạy và tính tiền** sau khi build, để giữ cache cho lần sau.

**`deployment/`**: stack production: ALB (80 → 301 sang 443; 443 dùng cert ACM), target group (health check `/healthz`), EC2 `t3.small` có Elastic IP, RDS PostgreSQL private (TLS bắt buộc), security group chặt (EC2 chỉ nhận cổng 80 từ ALB, DB chỉ nhận 5432 từ EC2, SSH chỉ từ IP của bạn). `run_deployment.sh` điều phối: kiểm tra `.env.production` → `tofu apply` → viết `.env` trên server → tạo role `todo_app` và DB `auth` bằng `init-databases.sql` → `docker compose pull && up -d` → chờ `/healthz` qua ALB.

Ứng dụng dùng role `todo_app` (chủ của hai database) thay vì tài khoản master của RDS; master chỉ chạy `init-databases.sql`. Deployment tạo runtime `.env` chỉ chứa thông tin app cần; master credentials và `APP_DB_PASSWORD` được chuyển qua `.env.bootstrap` tạm thời, rồi xóa sau bước bootstrap.

### 13.5 Dọn dẹp và CI
`scripts/deployment_cleanup.sh`: mặc định chỉ dừng container local; `--volumes`, `--images`, `--tofu` (hủy máy build), `--prod` (hủy production, **không** nằm trong `--all`), `--dry-run`; xác nhận bằng cách gõ `CLEANUP`.

CI (`.github/workflows/ci.yml`):
- `checks`: Postgres 18 làm service, `cargo fmt --check`, `clippy -D warnings`, kiểm `auth-kernel` ở chế độ `http`, `cargo test --workspace`, rồi `bun test` và `typecheck`;
- `redis-module`: build image, nạp module, chạy `test_redis_module.sh`;
- `audit`: `cargo audit` và `bun audit`;
- `images`: `docker build` image `compute-unit` và `docker compose ... config -q` cho cả hai layout.

### Tóm tắt Chương 13
Một image đa kiến trúc phục vụ cả ba chế độ; Compose ghép fragment; hạ tầng AWS tách làm "máy build tạm" và "production"; mọi thứ thao tác qua script có kiểm tra đầu vào.

### Lưu ý
- Public repo Docker Hub = binary và frontend công khai (secret không nằm trong image).
- Máy build không tự tắt; nhớ `--tofu`.
- `deployment/backend.tf` dùng local backend mặc định (`terraform.tfstate`). Chỉ chuyển sang S3 khi đã tạo bucket, thay block backend và chạy `tofu init -migrate-state`.
- Script upload đúng `deployment/compose.yaml` lên EC2; không còn dùng tên `docker-compose.yml`.
- Runtime `.env` trên EC2 không chứa master DB password; `.env.bootstrap` chỉ tồn tại trong giai đoạn khởi tạo database và bị xóa sau đó.
- Sửa `server_build.sh` (user data) **không** thay EC2; muốn áp dụng phải `tofu apply -replace=aws_instance.production`.
- Publish image mới **trước** khi áp thay đổi hạ tầng cần chúng (ví dụ bật `rds.force_ssl`).
- `set_real_ip_from` trong `deployment/nginx.conf` phải khớp CIDR của VPC; sai thì IP khách bị đánh đồng với ALB.

### Tự kiểm tra
1. Vì sao image runtime không cần shell, và bạn debug bằng gì?
2. Điều gì xảy ra nếu bạn mount `deployment/nginx.conf` như một template?
3. Vì sao ALB chỉ health-check `/healthz` mà không gọi API thật?

---

## Chương 14 — Chiến lược test

**Mục tiêu:** biết mỗi test bảo vệ điều gì, và quan trọng hơn, **chỗ nào chưa có test**.

### 14.1 Bản đồ test

| Vị trí | Số test | Kiểu | Bảo vệ điều gì |
|---|---|---|---|
| `glue/src/token.rs` | 2 | unit | JWT round-trip giữ nguyên `unique_id` và `session_id`; token bị sửa → `Unauthorized` |
| `glue/src/errors.rs` | 2 | unit | ánh xạ status → mã HTTP; `eject`/`eject_ctx` giữ ngữ cảnh và status |
| `auth/networking/.../extract_auth.rs` | 2 | unit | parse `Basic` hợp lệ; thiếu/sai định dạng bị từ chối |
| `to_do/core/.../basic_actions/*.rs` | 5 | unit với `Store` giả | core chuyển đúng tham số và chủ sở hữu xuống store, giữ nguyên lỗi của store |
| `to_do/dal/src/.../enums.rs`, `schema.rs` | 3 | unit | `TaskStatus` (parse không phân biệt hoa thường, từ chối giá trị lạ), `AllTodoItems` gom nhóm đúng |
| `to_do/dal/tests/postgres.rs` | 2 | `sqlx::test` trên **Postgres thật** | hai user dùng chung title; **cô lập giữa các user** (đọc/sửa/xóa item người khác đều thất bại) |
| `to_do/networking/.../tests/api_integration.rs` | 1 (dài) | tích hợp, store và session **trong bộ nhớ** | toàn vòng đời qua router axum: create → get → update → delete, 401 thiếu token, 404 item lạ/của người khác, cô lập user |
| `ingress/src/main.rs` | 4 | tích hợp router | frontend + SPA + healthz, 404 API, CORS preflight (debug), logout cần token, rate limit 429 |
| `frontend/rust-interface/src/lib.rs` | 2 | unit | nhãn nút WASM |
| `frontend/tests/*.test.js` | 7 | `bun test` | dựng URL API, helper gọi API |
| CI `scripts/test_redis_module.sh` | 1 kịch bản | hành vi trên Redis thật | login/update, session cũ `STALE`, refresh lần 20, logout và TTL được gắn |
| `scripts/basic_test.sh` | smoke | cần `TOKEN` + Postgres | CRUD qua server to-do standalone |
| `nanoservices/to_do/scripts/test_server_com.sh` | E2E | cũ | xem Lưu ý |

### 14.2 Hai mức "giả"
- **Store giả** (struct vài dòng): kiểm tra *core* không phụ thuộc DB.
- **`MemoryStore` + `MemoryUserSessions`** (`api_integration.rs`): chạy **cả router axum** (extractor, status code, JSON) mà không cần Postgres/Redis. Đây là lý do pattern trait + descriptor (Chương 3) đáng giá.

Còn lại phải tin vào test trên hạ tầng thật: `postgres.rs` cho SQL và `test_redis_module.sh` cho hành vi Redis. Redis behavior test hiện cover session replacement (`session_id` cũ bị `STALE`), refresh ở request 20, logout và việc TTL được gắn; nó chưa chờ key hết hạn thực tế.

### 14.3 Chỗ chưa có test (xác nhận bằng cách đếm trong source)
- **`auth-core`** (login, tạo user, giới hạn độ dài), **`auth-dal`** (SQL, Argon2), **`auth-kernel`** (tích hợp `REFRESH` → tra user → refresh cùng session ID), **`cache-client`**: chưa có test trực tiếp đầy đủ.
- **Nhánh `TIMEOUT`** và hết hạn TTL thực tế của module Redis (cần clock injection hoặc test có thời gian chờ ngắn).
- Luật **`invalid title`** của `to-do-core` (rỗng hoặc > 255 ký tự).
- **`TRUST_PROXY=1`** của ingress.
- **Component React** và luồng UI.
- **Layout microservice** (chưa có test end-to-end đi qua `auth-http` và `/users/me` với Redis session).
- Toàn bộ luồng HTTP đăng ký → login → API → logout và compatibility của JWT sau deploy.

### 14.4 Chạy test
```sh
docker compose up -d postgres
./scripts/test.sh                       # cargo test --workspace + bun test + typecheck + build frontend
RUN_POSTGRES_INTEGRATION=1 TOKEN=... ./scripts/test.sh   # thêm smoke test
```
`sqlx::test` tạo **database tạm riêng cho từng test** và chạy migration, nên cần `DATABASE_URL` trỏ tới Postgres cho phép `CREATEDB` (user `username` của compose đủ quyền).

### Tóm tắt Chương 14
Phần *logic* được test mạnh nhờ kiến trúc; phần *rìa hạ tầng* (Redis timeout, auth, microservice) còn mỏng.

### Lưu ý
- `cargo test --workspace` không chạy gì cho `cache-module` (`test = false`); Redis behavior test là script shell riêng.
- `test_redis_module.sh` kiểm tra `PTTL > 0`, không chứng minh key tự biến mất đúng lúc; chưa test nhánh `TIMEOUT`.
- `test_server_com.sh` là script E2E cũ (nhắc lại ở Chương 16); đừng coi nó là bằng chứng độ phủ.
- Test CORS chỉ có ý nghĩa ở build debug; `cargo test --release` bỏ qua nó (đã đặt `cfg(debug_assertions)`).

### Tự kiểm tra
1. Test nào chứng minh user A không xóa được item của user B, và nó chạy trên DB thật hay trong bộ nhớ?
2. Bạn sẽ viết test nào đầu tiên để bảo vệ logic `REFRESH`?

---

## Chương 15 — Mô hình bảo mật

**Mục tiêu:** nhìn hệ thống như người tấn công: đã chặn gì, còn hở gì.

### 15.1 Bảng đối chiếu

| Mối đe dọa | Đã có | Còn thiếu / ghi chú |
|---|---|---|
| Lộ mật khẩu từ DB | Argon2 (muối ngẫu nhiên, tham số mặc định của crate); `Debug` che hash | không có chính sách đổi mật khẩu |
| Dò email qua thời gian phản hồi | `verify_dummy_password` khi email không tồn tại; cùng một thông báo 401 | — |
| Dò email qua đăng ký | bị rate limit | đăng ký trùng trả `409`, nên vẫn lộ email đã có (đánh đổi của đăng ký mở) |
| Brute-force đăng nhập | token bucket theo IP khách (ingress) + `limit_req` ở Nginx | **không khóa theo tài khoản**; botnet nhiều IP vẫn thử được |
| Giả mạo token | HS256 + `exp` bắt buộc; secret ≥ 32 ký tự, từ chối giá trị mẫu | khóa đối xứng: mọi service giữ secret đều ký được token của bất kỳ ai |
| Token cũ sau login mới | `session_id` trong JWT phải khớp session Redis hiện tại; login mới làm JWT cũ `STALE` | chỉ một session active cho mỗi user; chưa hỗ trợ nhiều thiết bị |
| Token bị đánh cắp/dùng sau logout | session Redis: logout xóa key đúng `session_id`, idle timeout 20 phút, refresh định kỳ; `/users/me` cũng kiểm tra Redis | token trong `localStorage` bị XSS lấy được trước khi thu hồi (xem dưới) |
| SQL injection | mọi câu SQL dùng tham số (`bind`) | — |
| Đọc/sửa dữ liệu người khác | điều kiện `user_id` ngay trong SQL; 404 thay vì 403; có test trên DB thật | `user_id` là số tuần tự; an toàn nhờ SQL, không nhờ che giấu |
| XSS | CSP chặt (`script-src 'self'`, không inline script, `frame-ancestors 'none'`), React escape mặc định | JWT ở `localStorage` nên XSS = mất phiên |
| CSRF | xác thực bằng header `Authorization`, không dùng cookie | nếu chuyển sang cookie phải thêm bảo vệ CSRF |
| Nghe lén đường truyền | production: HTTPS (ACM) tới ALB, HSTS; DB qua TLS (`sslmode=require` + `rds.force_ssl`) | ALB → Nginx → ingress → Redis chạy HTTP/giao thức thường trong mạng riêng |
| Né rate limit bằng header giả | `X-Real-IP` do Nginx ghi đè; EC2 chỉ nhận cổng 80 từ ALB | chỉ an toàn nếu `TRUST_PROXY=1` đúng chỗ (Chương 8) |
| Lộ chi tiết nội bộ | lỗi `Unknown` chỉ trả `"internal server error"`, chi tiết vào log | — |
| Lộ secret qua repo/image | `.env*` bị gitignore và dockerignore; tar gửi lên máy build loại `.env`, `*.pem`, state | `aws_access_key.txt` tồn tại trong working tree (đã bị ignore, nên xóa) |
| Quyền DB quá rộng | app dùng role `todo_app` chỉ sở hữu hai database, không dùng master RDS | role tạo bằng `init-databases.sql`, nên thử trên RDS thật trước khi tin |
| Hạ tầng | DB private, SG tối thiểu, IMDSv2 bắt buộc, ổ đĩa mã hóa, SSH chỉ từ IP bạn | SSH vẫn mở cổng 22 (có thể thay bằng SSM); state local nếu chưa bật backend S3 |
| Chuỗi cung ứng | ghim version (Rust, Bun, wasm-pack, `--frozen-lockfile`), `cargo audit`/`bun audit` trong CI, image distroless chạy non-root | tag Redis theo minor (`7.4`) để nhận vá, đổi lại kém tái lập |
| Tải/DoS | timeout 30 giây, giới hạn body mặc định của axum, rate limit | không giới hạn số luồng Argon2 đồng thời |

### 15.2 Ba ranh giới tin cậy
1. **Internet → ALB (HTTPS):** nơi duy nhất nhận lưu lượng công khai.
2. **ALB → EC2 (SG):** chỉ ALB được nói chuyện với Nginx; vì vậy header do Nginx đặt là đáng tin.
3. **Ứng dụng → DB/Redis:** chỉ nội bộ; DB bắt buộc TLS, Redis có mật khẩu nhưng không có TLS.

### 15.3 JWT + session: vì sao cả hai
JWT cho phép *xác minh danh tính không cần tra DB*, nhưng không thu hồi được. Session Redis bù chỗ đó: thu hồi tức thì (logout), idle timeout, kiểm lại user định kỳ. Cái giá: mỗi request tốn một lệnh Redis, và Redis trở thành thành phần **bắt buộc**.

### Tóm tắt Chương 15
Nền tảng chắc: băm mật khẩu tốt, chống timing, SQL có điều kiện sở hữu, CSP chặt, HTTPS và hạ tầng tối thiểu. Còn hở chính: JWT ở `localStorage`, không khóa theo tài khoản, khóa đối xứng dùng chung, thiếu email verification/đặt lại mật khẩu.

### Lưu ý
- Layout microservice **không có** rate limit của ingress, không CSP, không timeout; chỉ nên dùng để học.
- Không có tính năng xóa tài khoản hoặc đổi mật khẩu; thêm chúng sẽ chạm vào session (phải xóa key Redis) và dữ liệu to-do (khác DB).

### Tự kiểm tra
1. Kẻ tấn công chiếm được `JWT_SECRET` làm được gì, và `session.update` có ngăn được không?
2. Vì sao dùng header `Authorization` giúp tránh CSRF, nhưng lại đẩy rủi ro sang XSS?

---

## Chương 16 — Caveats tổng hợp

Gom từ mọi chương, theo mức độ. Mục **[Đã xử lý]** là những vấn đề đã sửa trong đợt review (Phụ lục D) nhưng đáng nhớ vì chúng cho thấy *kiểu lỗi* hay gặp.

### 16.1 Cao (nên xử lý trước khi chạy thật)
1. **`TRUST_PROXY` đặt sai.** Tắt sau proxy → cả hệ thống chung một bucket rate limit (5 login burst cho tất cả, 3 lượt đăng ký/giờ); bật khi ingress lộ trực tiếp → né rate limit bằng `X-Real-IP` giả. (Ch. 8, 12)
2. **JWT trong `localStorage`.** XSS = mất phiên; CSP chỉ giảm nhẹ. (Ch. 9, 15)
3. **Khóa đối xứng dùng chung.** Bất kỳ service nào giữ `JWT_SECRET` đều giả danh được mọi user (cả chế độ `auth-http`). (Ch. 5, 15)
4. **`user_id` không có FK** (hai DB). Reset/khôi phục không đồng bộ giữa auth DB và to-do DB vẫn có thể làm user mới thừa hưởng item của user cũ. `clean_db.sh` bảo vệ luồng cleanup local thông thường, nhưng không thể bảo vệ thao tác restore/manual truncate ngoài script. (Ch. 7, 11)

### 16.2 Trung bình
5. **Redis không persistence:** restart = mọi người bị logout. (Ch. 6)
6. **Một session active mỗi user:** đăng nhập thiết bị thứ hai làm JWT ở thiết bị đầu thành `STALE`; chưa hỗ trợ nhiều thiết bị đồng thời. (Ch. 6)
7. **Login và logout chung bucket** rate limit. (Ch. 8)
8. **Không khóa theo tài khoản**, không phân biệt IP bị chia sẻ (NAT). (Ch. 15)
9. **Production là một EC2 + RDS một AZ.** Không chịu lỗi một máy; Redis session nằm trên máy đó nên không scale ngang được. (devops_handbook.md §7)
10. **Máy build không tự tắt** và tính tiền; ALB, EC2, RDS đều tính theo giờ. (Ch. 13)
11. **Layout microservice thiếu bảo vệ** của ingress. (Ch. 15)
12. **`TaskStatus` trong JSON phân biệt hoa thường** → 422 do `Json` extractor, không qua `NanoServiceError`. (Ch. 7)
13. **Nhánh `TIMEOUT`, TTL expiry thật, `auth-core`, `auth-kernel`, `TRUST_PROXY=1`, luật `invalid title`, component React** chưa có test đầy đủ. (Ch. 14)

### 16.3 Thấp / nên biết
14. `/healthz` chỉ chứng minh process sống, không kiểm Postgres/Redis. (Ch. 8)
15. Chưa có phân trang; `GET /get` trả tối đa 1000 item. (Ch. 7)
16. Migration thứ hai của auth làm hỏng khởi động nếu DB cũ đã có hai email chỉ khác hoa thường **[suy luận từ SQL]**. (Ch. 5)
17. Trong microservice mode, HTTP refresh thông qua `/users/me` xác thực session thêm một lần; có thể tạo thêm `session.update`, `session.login` và một lần đọc user so với monolith. (Ch. 5)
18. Điều kiện code của nhánh `TIMEOUT` là `elapsed_secs > timeout_secs`, trong khi Redis TTL được đặt cùng khoảng thời gian; nhánh này chưa có test trực tiếp. (Ch. 6)
17. `ConnectionManager` Redis toàn cục: địa chỉ của lần gọi đầu thắng. (Ch. 6)
18. `nanoservices/to_do/scripts/test_server_com.sh` là script E2E cũ; API đã đổi (`title`, id số, `/get/{id}`), đừng dùng làm chuẩn. (Ch. 14)
19. `aws_access_key.txt` nếu còn trong working tree cần xóa; dùng `aws configure` hoặc SSO. (Ch. 15)
20. Tên package `es_react`, title `My React App`, `esbuild ^0.24`: dấu vết template, chỉ thẩm mỹ. (Ch. 9)
21. Hai backend của `auth-kernel` bằng `cfg(feature)` làm code trùng ở nhiều file; kiến trúc sạch hơn là inject một trait lúc runtime. (Ch. 2, 3)
22. Tag Redis `7.4` (không ghim patch) đổi lấy việc nhận bản vá; build hai thời điểm có thể khác nhau.

### 16.4 [Đã xử lý / patch trong đợt review]
- Production chỉ HTTP → ALB + ACM, HTTPS, HSTS, DB qua TLS.
- Rate limit gộp vào IP của proxy → `ClientIp` + `TRUST_PROXY` + Nginx `real_ip`.
- `.env.example` dùng base64 cho mật khẩu nhúng URL → `openssl rand -hex 32`.
- To-do standalone không gọi `validate_secret` → đã gọi.
- `ports:` publish 8001/8080/8081 ra mọi interface → bind `127.0.0.1`.
- Redis: key không TTL, `logout` có `deny-oom`, key position sai, thiếu `replicate_verbatim`.
- Hằng timeout 20 phút lặp hai chỗ → `glue::session::SESSION_TIMEOUT_MINS`.
- Bảng nối `user_connections` thừa → cột `user_id` trên `to_do_items`; thêm test cô lập trên DB thật.
- `ingress/nginx.conf` dùng `events{}`/`http{}` trong template; cú pháp `http2` cũ.
- Dockerfile không có layer/cache, chạy root → BuildKit cache, distroless non-root; sửa BuildKit syntax directive.
- `CompressionLayer` import phải có ở release vì middleware được dùng ở mọi profile.
- Deployment: dùng đúng `compose.yaml`; local OpenTofu backend mặc định thay cho S3 bucket placeholder.
- Dev certificate `.crt`/`.key` được ignore trong Git và Docker context.
- `clean_db.sh`: chỉ chấp nhận DB localhost, xóa `user_session_*` trước khi truncate và không reset sequence.
- JWT: thêm `session_id`; Redis `login/update/logout` so khớp session ID; login mới thu hồi JWT cũ; `/users/me` kiểm tra Redis.
- Timeout/refresh: TTL 20 phút và refresh ở request hợp lệ thứ 20.
- Deployment: tách runtime `.env` khỏi `.env.bootstrap`; master DB credentials không lưu trong runtime env của EC2.
- Frontend logout xử lý 401 bằng cách xóa token và trở về login; sửa `width=device-width`.
- CI có audit, build image, test Redis behavior và kiểm tra Compose.

---

## Chương 17 — Lộ trình học

### 17.1 Thứ tự đọc code (từ đơn giản tới phức tạp)
1. `glue/src/errors.rs`, `token.rs`, `db.rs`, `session.rs`: nền móng nhỏ nhất.
2. `nanoservices/to_do/dal/` (migrations → `schema.rs` → `transactions/*` → `tests/postgres.rs`): SQL và pattern trait + descriptor.
3. `nanoservices/to_do/core/`: logic mỏng, test với `Store` giả.
4. `nanoservices/to_do/networking/axum_server/`: extractor `AuthUser`, router, và `tests/api_integration.rs` (đọc test này trước: nó chạy cả vòng đời).
5. `nanoservices/auth/dal` → `core` → `networking`: đăng ký/đăng nhập, Argon2, dummy verify.
6. `nanoservices/user-session-cache/cache-module` rồi `cache-client` rồi `auth/kernel`: phần khó nhất.
7. `ingress/src/main.rs`: ráp mọi thứ, rate limit, CSP.
8. `frontend/src/index.tsx`, `api/`, `rust-interface/`.
9. `Dockerfile`, `compose.yaml`, `deployment/`: đóng gói và hạ tầng.

### 17.2 Lộ trình 5 buổi

| Buổi | Việc | Kết quả kỳ vọng |
|---|---|---|
| 1 | Chạy `./scripts/init.sh`, tạo user, đăng nhập, thao tác UI; dùng `curl`/`xh` gọi từng API; xem Redis bằng `redis-cli` (`HGETALL user_session_<unique_id>`, `PTTL ...`) | Thấy đúng dữ liệu của Chương 11 |
| 2 | Theo dấu request "tạo item" qua từng crate (Ch. 10.3); đọc log INFO của `TraceLayer` hoặc dùng debugger | Vẽ được sơ đồ không nhìn tài liệu |
| 3 | Đọc module Redis; chạy `test_redis_module.sh`; thử tay chuỗi lệnh `session.*` | Hiểu `REFRESH`, TTL, counter |
| 4 | Chạy layout monolith Docker (TLS tự ký) rồi microservice; so sánh hành vi (rate limit, CSP) | Giải thích được khác biệt (Ch. 15) |
| 5 | Đọc `devops_handbook.md`; chạy `deployment_cleanup.sh --dry-run`; nếu có AWS, triển khai thử rồi hủy | Hiểu đường đi ALB → Nginx → ingress → RDS |

### 17.3 Bài tập (tăng dần độ khó)
1. **Dễ:** thêm `GET /api/v1/count` trả số item pending/done. Bạn phải chạm những tầng nào?
2. **Dễ:** đổi `SESSION_TIMEOUT_MINS` thành 5, build lại; quan sát nhánh `TIMEOUT` bằng tay.
3. **Vừa:** thêm endpoint đổi tiêu đề (`PATCH /api/v1/title`) kèm test core, test API và test DAL cô lập user.
4. **Vừa:** viết test cho `TRUST_PROXY=1` trong `ingress` (cẩn thận biến môi trường toàn cục giữa các test).
5. **Vừa:** viết test cho `REFRESH` ở `auth-kernel` dùng một `cache` giả (cần tách `cache_client` sau một trait).
6. **Khó:** tách logic thời gian của `cache-module` thành crate thuần để unit test `TIMEOUT`.
7. **Khó:** thêm khóa tạm thời theo tài khoản sau N lần đăng nhập sai (lưu ở đâu? Redis hay Postgres?).
8. **Khó:** chuyển JWT sang cookie `HttpOnly; SameSite=Strict` và bổ sung bảo vệ CSRF.
9. **Khó:** thay hai nhánh `cfg(feature)` của kernel bằng một trait `UserLookup` inject lúc runtime.
10. **Dài:** thêm phân trang cho `GET /get` (cursor theo `id`) và cập nhật frontend.

### Tự kiểm tra tổng hợp
1. Mô tả đường đi và dữ liệu của một request `POST /create` từ browser tới Postgres, nêu tên từng crate.
2. Đặt `TRUST_PROXY` ra sao ở local native, monolith Docker và production, và vì sao?
3. Liệt kê ba nơi cùng phải đổi nếu đổi thời gian timeout session.

---

## Chương 18 — Glossary

| Thuật ngữ | Nghĩa trong project này |
|---|---|
| **Nanoservice** | Một domain (auth/to-do) chia thành crate nhỏ theo tầng, có thể gộp in-process hoặc tách process |
| **Descriptor** | Struct đại diện cho một nơi lưu trữ cụ thể (Postgres, bộ nhớ) và implement các trait giao dịch |
| **Transaction trait** | Trait nhỏ cho một thao tác dữ liệu (`SaveOne`, `GetAll`, ...); *không* liên quan transaction SQL |
| **Kernel** | Tầng điều phối session/tra user của auth |
| **DAL** | Data access layer: SQL, schema, migration |
| **Extractor** | Kiểu tham số của handler axum tự trích dữ liệu từ request |
| **Ingress** | Process monolith ráp frontend + auth + to-do; điểm vào cổng 8001 |
| **`unique_id`** | UUID của user; đi trong JWT và tên key Redis |
| **`session_id`** | UUID v4 cho một lần login; phải khớp field trong Redis để JWT được chấp nhận |
| **`perm_user_id`** | `users.id` lưu trong session Redis |
| **`owner_key`** | Chuỗi chủ sở hữu item (`users.id` dạng chuỗi) truyền xuống to-do |
| **Session (Redis)** | Hash `user_session_<unique_id>` có `session_id`, timeout trượt 20 phút, counter và TTL |
| **`STALE`** | Redis báo JWT có `session_id` khác session active; client nhận 401 |
| **`REFRESH`** | Kết quả `session.update` khi counter đạt 20, buộc kernel tra lại user nhưng giữ nguyên session ID |
| **Token bucket** | Thuật toán rate limit: xô chứa token, nạp đều, mỗi request tiêu một token |
| **`TRUST_PROXY`** | Cờ cho ingress tin `X-Real-IP` làm IP khách |
| **`real_ip`** | Module Nginx đặt `$remote_addr` theo `X-Forwarded-For` từ proxy tin cậy |
| **ALB** | AWS Application Load Balancer, kết thúc TLS, forward về EC2 |
| **ACM** | AWS Certificate Manager, cấp và tự gia hạn chứng chỉ TLS |
| **RDS** | AWS Relational Database Service (PostgreSQL được quản lý) |
| **Distroless** | Image runtime tối giản không shell, không package manager |
| **WASM** | WebAssembly; ở đây là hàm Rust chạy trong browser |
| **CSP** | Content-Security-Policy: header giới hạn nguồn script/style/kết nối |
| **HSTS** | Header buộc browser dùng HTTPS cho domain |
| **Monomorphization** | Trình biên dịch sinh bản mã riêng cho từng kiểu cụ thể của generic |
| **Feature unification** | Cargo hợp nhất mọi feature được yêu cầu trong một lần build |
| **OpenTofu** | Nhánh mã nguồn mở tương thích Terraform |

---

## Phụ lục A — Index file quan trọng

| File | Vai trò |
|---|---|
| `Cargo.toml`, `rust-toolchain.toml` | Workspace, lint, profile; ghim Rust 1.100.0 |
| `glue/src/{errors,token,db,session}.rs`, `log/` | Lỗi, JWT, pool DB, hằng số session, logger |
| `nanoservices/auth/dal/migrations/*.sql` | Bảng `users`, email duy nhất không phân biệt hoa thường |
| `nanoservices/auth/core/src/api/**` | Login, tạo user, lấy user |
| `nanoservices/auth/kernel/src/**` | `GetUserSession`, backend tra user |
| `nanoservices/auth/networking/axum_server/src/**` | Route auth, `Credentials`, `main.rs` standalone |
| `nanoservices/to_do/dal/migrations/*.sql` | Lịch sử schema to-do (5 migration) |
| `nanoservices/to_do/dal/src/to_do_items/**` | SQL, schema, `TaskStatus` |
| `nanoservices/to_do/core/src/api/basic_actions/*.rs` | Logic CRUD |
| `nanoservices/to_do/networking/axum_server/src/**` | Route, `AuthUser`, routers |
| `nanoservices/to_do/networking/axum_server/tests/api_integration.rs` | Test vòng đời qua router, store giả |
| `nanoservices/user-session-cache/cache-module/src/**` | Module Redis: `login`, `update`, `logout`, `touch_expiry` |
| `nanoservices/user-session-cache/cache-client/src/lib.rs` | Client async cho module |
| `ingress/src/main.rs` | Ráp process, rate limit, CSP, embed frontend, 4 test functions |
| `frontend/src/index.tsx`, `src/api/*`, `src/components/*` | UI và lớp gọi API |
| `frontend/rust-interface/src/lib.rs` | Hàm WASM |
| `Dockerfile` | Image `compute-unit` đa kiến trúc |
| `compose.yaml`, `*/build-config.yml` | Postgres + Redis và các fragment layout |
| `ingress/nginx.conf`, `frontend/nginx.conf`, `deployment/nginx.conf` | Ba cấu hình Nginx |
| `builds/*` | Máy build tạm, `publish_images.sh` |
| `deployment/*` | Stack production AWS |
| `scripts/*` | `init`, `cleanup`, `clean_db`, `create_user.sh`, `test`, `basic_test`, `test_redis_module`, `gen_dev_cert`, `deployment_cleanup` |
| `.github/workflows/ci.yml` | CI |
| `README.md`, `engineering_handbook.md`, `devops_handbook.md` | Chạy project, học kiến trúc và triển khai/vận hành |

## Phụ lục B — Cổng và địa chỉ

| Cổng | Dịch vụ | Phạm vi |
|---|---|---|
| 8001 | ingress | host (local native) hoặc `127.0.0.1` (Docker); trong container ở production |
| 8080 | to-do standalone | `127.0.0.1` |
| 8081 | auth standalone | `127.0.0.1` |
| 5432 | PostgreSQL | `127.0.0.1` (local); private (RDS) |
| 6379 | Redis | `127.0.0.1` (local); không publish ở production |
| 80 / 443 | Nginx (monolith local, TLS tự ký) | host |
| 80 | Nginx frontend (microservice) | host |
| 3000 | dev server tĩnh của frontend (`bun run serve`) | host |
| 80 / 443 | ALB | Internet (production) |
| 80 | Nginx trên EC2 | chỉ từ ALB |
| 22 | SSH tới EC2 | chỉ từ IP của bạn |

## Phụ lục C — Lệnh hay dùng

```sh
# Local native
./scripts/init.sh                      # .env + Postgres/Redis + build + chạy ingress
./scripts/create_user.sh you@example.com
./scripts/cleanup.sh                   # dừng container (giữ dữ liệu)
./scripts/clean_db.sh                  # DB localhost + xóa user_session_*; không reset sequence

# Test
docker compose up -d postgres && ./scripts/test.sh
./scripts/test_redis_module.sh         # cần container cache-ci đang chạy

# Redis
docker exec -it to-do-redis redis-cli -a "$REDIS_PASSWORD"
#   HGETALL user_session_<unique_id> ; PTTL user_session_<unique_id> ; FLUSHALL

# Docker
docker build --platform linux/amd64 -t compute-unit .
docker compose --env-file .env -f compose.yaml -f ingress/build-config.yml up --build

# Production (xem devops_handbook.md)
(cd builds && DOCKERHUB_USER=... ./publish_images.sh)
./deployment/run_deployment.sh
DOMAIN_NAME=... ./scripts/deployment_cleanup.sh --prod --dry-run
```

## Phụ lục D — Change log

### 2026-11-23 → 2026-11-25
**Bảo mật và vận hành**
- Production: ALB + ACM (HTTPS, redirect 80→443), Elastic IP, security group chặt, RDS TLS bắt buộc, role DB riêng `todo_app`.
- Ingress: `ClientIp` + `TRUST_PROXY`, nén gzip/brotli, ETag cho asset.
- Nginx production: `real_ip`, ghi đè `X-Real-IP`, `limit_req`, HSTS; sửa `ingress/nginx.conf` (chỉ khối `server`); `frontend/nginx.conf` có `limit_req`.
- Compose: các cổng chỉ bind `127.0.0.1`; `TRUST_PROXY=1` ở nơi có proxy.
- `to-do-axum-server` standalone gọi `validate_secret()`.
- `sqlx` bật `tls-rustls`; `.env.example` dùng `openssl rand -hex 32`.

**Code**
- `glue::session::SESSION_TIMEOUT_MINS` thay hằng lặp.
- Redis module: `PEXPIRE` sau login/update; `logout` bỏ `deny-oom`; key position `0,0,0`; `replicate_verbatim` ở login/logout.
- To-do: migration `20261125074738_owner_on_items.sql` (cột `user_id`, `created_at`, bỏ `user_connections`), query một câu, `LIMIT 1000`; test cô lập user trên Postgres thật.
- Test route/API có assertion preflight CORS bên trong (không phải test function riêng); assertion này chỉ chạy ở build debug.
- `index.html`: `lang`, `viewport`, title.

**Build và CI**
- Một `Dockerfile` đa kiến trúc (BuildKit cache, distroless non-root); bỏ `Dockerfile.x86/.aarch`.
- `rust-toolchain.toml` ghim `1.100.0`; wasm-pack/Bun ghim version.
- CI: test hành vi Redis, `cargo audit` + `bun audit`, build image, kiểm Compose.
- `scripts/gen_dev_cert.sh` thay cert dev bị commit.

**Tài liệu**
- Tách `README.md` (tổng quan), `engineering_handbook.md` (kiến trúc/kỹ thuật) và `devops_handbook.md` (triển khai/vận hành).
- Hoàn thiện `engineering_handbook.md` (tài liệu này): thêm Chương 8–18, Phụ lục A–D; cập nhật Chương 1–7.

### 2026-11-25 — đồng bộ sau review
- JWT thêm `session_id` riêng mỗi lần login; Redis kiểm tra session ID trên `update` và `logout`; `/users/me` cũng kiểm tra session.
- Redis TTL chuyển về đúng 20 phút; `REFRESH` tại request hợp lệ thứ 20; script behavior test kiểm tra session cũ bị `STALE`.
- `clean_db.sh` chỉ cho phép DB localhost, xóa key `user_session_*` và không reset sequence.
- Deployment dùng local OpenTofu state mặc định, upload `compose.yaml`, và tách `.env.bootstrap` tạm khỏi runtime `.env`.
- Cập nhật README và hai handbook theo các thay đổi trên.

### Chưa làm (cố ý để lại)
JWT sang cookie HttpOnly; SSM thay SSH; khóa theo tài khoản; trait `UserLookup`; test nhánh `TIMEOUT` và TTL expiry thật; phân trang đầy đủ; HA cho Redis/EC2/RDS.
