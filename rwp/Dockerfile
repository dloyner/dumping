# syntax=docker/dockerfile:1.7
ARG BUN_VERSION=1.4.4
FROM oven/bun:${BUN_VERSION} AS bun

FROM rust:1.100-bookworm AS build
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN ln -s /usr/local/bin/bun /usr/local/bin/bunx \
    && cargo install wasm-pack --locked --version 0.15.1

WORKDIR /app
COPY . .
ARG INGRESS_URL=http://localhost:8001
RUN cd frontend && bun install --frozen-lockfile && bun run wasm \
    && INGRESS_URL="$INGRESS_URL" bun run build

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    set -eux; \
    cargo build --release -p ingress -p auth-axum-server; \
    cargo build --release -p to-do-axum-server --no-default-features --features auth-http; \
    mkdir /out; \
    for b in ingress to-do-axum-server auth-axum-server; do strip -o "/out/$b" "target/release/$b"; done

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /out/ /usr/local/bin
EXPOSE 8001 8080 8081
CMD ["ingress"]
