#!/bin/bash
# =============================================================================
# TransitOps database initialisation script
#
# Called by the PostgreSQL Docker image's entrypoint when the data volume is
# first created.  Subsequent container restarts skip this script entirely.
#
# Execution context
# -----------------
#   - Runs as the POSTGRES_USER (transitops_app, which is a PG superuser)
#   - POSTGRES_DB is already created and accessible
#   - The /opt/transitops/db volume contains schema.sql, migrations/, and seeds/
# =============================================================================

set -euo pipefail

DB_USER="${POSTGRES_USER:-transitops_app}"
DB_NAME="${POSTGRES_DB:-transitops}"
DB_DIR="/opt/transitops/db"

log() { echo "[init] $*"; }

log "=== TransitOps database initialisation started ==="

# ── Schema ─────────────────────────────────────────────────────────────────────
log "Applying schema.sql..."
psql -v ON_ERROR_STOP=1 \
     --username "$DB_USER" \
     --dbname   "$DB_NAME" \
     -f "$DB_DIR/schema.sql"

# ── Migrations (applied in numeric order) ─────────────────────────────────────
log "Applying migrations..."
if compgen -G "$DB_DIR/migrations/*.sql" > /dev/null 2>&1; then
    for f in $(ls -v "$DB_DIR/migrations/"*.sql 2>/dev/null | sort -V); do
        log "  migration: $(basename "$f")"
        psql -v ON_ERROR_STOP=1 \
             --username "$DB_USER" \
             --dbname   "$DB_NAME" \
             -f "$f"
    done
else
    log "  (no migration files found)"
fi

# ── Seeds (applied in numeric order; failures are tolerated) ──────────────────
log "Applying seeds..."
if compgen -G "$DB_DIR/seeds/*.sql" > /dev/null 2>&1; then
    for f in $(ls -v "$DB_DIR/seeds/"*.sql 2>/dev/null | sort -V); do
        log "  seed: $(basename "$f")"
        psql --username "$DB_USER" \
             --dbname   "$DB_NAME" \
             -f "$f" \
        || log "  WARNING: seed $(basename "$f") failed — skipping"
    done
else
    log "  (no seed files found)"
fi

log "=== TransitOps database initialisation complete ==="
