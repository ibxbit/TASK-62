# =============================================================================
# frontend/Dockerfile — Build the Yew/WASM SPA and serve it through nginx.
#
# Stage 1 (builder): full Rust toolchain + wasm-pack + trunk to compile the
#   WASM bundle.
# Stage 2 (runtime): nginx:alpine with the built dist/ + a config that does
#   SPA fallback for browser navigations and reverse-proxies all XHR requests
#   to the API container.
# =============================================================================

# ── Stage 1: Build ─────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        curl pkg-config libssl-dev ca-certificates build-essential \
 && rm -rf /var/lib/apt/lists/*

# wasm32 target + Trunk (build + bundle tool for WASM SPAs).
RUN rustup target add wasm32-unknown-unknown
RUN cargo install --locked trunk@0.20.3 \
 && curl -sSfL https://github.com/rustwasm/wasm-bindgen/releases/download/0.2.95/wasm-bindgen-0.2.95-x86_64-unknown-linux-musl.tar.gz \
        | tar -xz -C /usr/local/bin --strip-components=1 \
              wasm-bindgen-0.2.95-x86_64-unknown-linux-musl/wasm-bindgen

WORKDIR /app

# Cache the dependency graph by copying manifests first, building a stub.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
 && printf 'fn main(){}' > src/main.rs \
 && printf ''            > src/lib.rs  \
 && cargo fetch

# Real source.
COPY index.html ./
COPY src ./src
COPY tests ./tests

# Trunk produces dist/ with the hashed WASM + JS glue + index.html.
# Skip --release (and therefore wasm-opt) because the bundled wasm-opt is
# older than the WASM bindgen version produced by current Rust.  Debug
# bundle is still optimised enough for test/demo deployments and is the
# form the E2E Playwright suite exercises.
RUN trunk build

# ── Stage 2: Runtime ───────────────────────────────────────────────────────────
FROM nginx:1.27-alpine AS runtime

COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
