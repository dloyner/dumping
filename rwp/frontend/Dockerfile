# frontend/Dockerfile
ARG BUN_VERSION=1
FROM oven/bun:${BUN_VERSION} AS bun

FROM rust:1.100-bookworm AS build

COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN ln -s /usr/local/bin/bun /usr/local/bin/bunx
RUN cargo install wasm-pack

WORKDIR /app
COPY . .
WORKDIR /app/frontend
ARG INGRESS_URL=http://localhost:8001
RUN bun install --frozen-lockfile
RUN bun run wasm
RUN INGRESS_URL="$INGRESS_URL" bun run build

FROM nginx:alpine
COPY --from=build /app/frontend/public /usr/share/nginx/html
COPY frontend/nginx.conf /etc/nginx/templates/default.conf.template

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
