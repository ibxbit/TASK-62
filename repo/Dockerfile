# =============================================================================
# TransitOps Backend — Multi-stage Dockerfile
#
# Stage 1 (builder): Uses postgres:16-bookworm as base so that a temporary
#   PostgreSQL instance can be started during `cargo build`.  This satisfies
#   sqlx's compile-time query verification without requiring a pre-generated
#   `.sqlx/` offline cache.
#
# Stage 2 (runtime): Minimal debian:bookworm-slim image with only the compiled
#   binary and database migration files.
# =============================================================================

# ── Stage 1: Build ─────────────────────────────────────────────────────────────
FROM postgres:16-bookworm AS builder

USER root
ARG DEBIAN_FRONTEND=noninteractive

# Install Rust build prerequisites
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        build-essential \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (stable, minimal profile — no docs/clippy needed in CI)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH="/root/.cargo/bin:$PATH"

WORKDIR /app

# ── Dependency cache layer ─────────────────────────────────────────────────────
# Copy only manifests first so Docker can cache the dependency fetch layer.
# The empty stub sources let `cargo fetch` resolve the full dependency graph.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && printf 'fn main(){}' > src/main.rs \
    && printf ''             > src/lib.rs  \
    && cargo fetch

# ── Full source ────────────────────────────────────────────────────────────────
COPY src ./src
COPY db  ./db

# ── Build-time environment ─────────────────────────────────────────────────────
# ENCRYPTION_KEY is a 64-hex-character (32-byte) AES-256 dev key.
# DATABASE_URL points to the temporary postgres we start below.
ENV DB_USER=transitops_app \
    DB_PASS=build_devpass \
    DB_NAME=transitops \
    ENCRYPTION_KEY=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
    SERVER_HOST=0.0.0.0 \
    SERVER_PORT=8080

# ── Start temporary PostgreSQL, run schema/migrations, compile ─────────────────
RUN set -eux; \
    \
    # Initialise postgres data directory with trust auth (no passwords needed
    # for this ephemeral build-time instance)
    mkdir -p /tmp/pgdata; \
    chown postgres:postgres /tmp/pgdata; \
    su postgres -c "initdb -D /tmp/pgdata \
        --auth-local=trust \
        --auth-host=trust \
        --username=postgres"; \
    echo "listen_addresses = 'localhost'" >> /tmp/pgdata/postgresql.conf; \
    \
    # Start postgres and wait until it accepts connections
    su postgres -c "pg_ctl start -D /tmp/pgdata -l /tmp/pg.log -w"; \
    \
    # Create application role (superuser so it can CREATE EXTENSION) and database
    psql -h localhost -U postgres -c \
        "CREATE ROLE $DB_USER WITH SUPERUSER LOGIN PASSWORD '$DB_PASS';"; \
    psql -h localhost -U postgres -c \
        "CREATE DATABASE $DB_NAME OWNER $DB_USER;"; \
    \
    # Apply schema (extensions, tables, indexes, constraints)
    psql -h localhost -U postgres -d "$DB_NAME" -f /app/db/schema.sql; \
    \
    # Apply migrations in numeric order
    for f in $(ls -v /app/db/migrations/*.sql 2>/dev/null); do \
        echo "-- migration: $f"; \
        psql -h localhost -U postgres -d "$DB_NAME" -f "$f"; \
    done; \
    \
    # Apply seeds (tolerate failures — some seeds have optional FK dependencies)
    for f in $(ls -v /app/db/seeds/*.sql 2>/dev/null); do \
        echo "-- seed: $f"; \
        psql -h localhost -U postgres -d "$DB_NAME" -f "$f" || true; \
    done; \
    \
    # Compile the application binary with sqlx pointing at our live database
    DATABASE_URL="postgres://$DB_USER:$DB_PASS@localhost:5432/$DB_NAME" \
    cargo build --release --bin transitops-backend; \
    \
    # Shut down the build-time postgres (data is discarded)
    su postgres -c "pg_ctl stop -D /tmp/pgdata -m fast"

# ── Stage 2: Runtime ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary
COPY --from=builder /app/target/release/transitops-backend ./transitops-backend
# Copy database files so health-check/migration tooling can access them if needed
COPY --from=builder /app/db ./db

EXPOSE 8080

ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

ENTRYPOINT ["./transitops-backend"]
