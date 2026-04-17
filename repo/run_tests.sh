#!/usr/bin/env bash
# =============================================================================
# run_tests.sh — Containerised TransitOps test runner
#
# All tests run inside Docker so neither developer machines nor CI runners
# need a host Python, Rust, wasm-pack, browser, or Node toolchain.  Anything
# that's installable is installed once into a Docker layer and reused.
#
# Usage
# -----
#   ./run_tests.sh                  Same as `all`
#   ./run_tests.sh unit             Pure-Python unit tests (no API needed)
#   ./run_tests.sh api              Python API integration tests
#                                   (spins up disposable db-test + api-test)
#   ./run_tests.sh integration      Rust integration tests under tests/
#   ./run_tests.sh frontend         Yew/wasm-pack component tests (headless)
#   ./run_tests.sh e2e              Playwright browser tests against the
#                                   real frontend + real API + real DB
#   ./run_tests.sh all              unit + api + integration + frontend + e2e
#   ./run_tests.sh -h | --help      Show this message
#
# Environment variables (host mode)
# ---------------------------------
#   COMPOSE_BIN     `docker compose` (default) or `docker-compose`
#   KEEP_STACK=1    Don't `compose down` after the run (faster iteration)
#
# Environment variables (in-container mode, set by docker-compose)
# ----------------------------------------------------------------
#   IN_CONTAINER=1  Skip the docker-compose orchestration loop and execute
#                   the requested category directly.  Set automatically by
#                   the test-runner image's ENV.
#   API_URL         Base URL for the API (default http://api-test:8081)
#   DATABASE_URL    PostgreSQL DSN
#   ENCRYPTION_KEY  64-hex-char AES-256 key
#
# Exit codes
# ----------
#   0  All selected tests passed
#   1  One or more test categories failed
#   2  Setup / dependency error
# =============================================================================

set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[PASS]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[FAIL]${NC}  $*"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ── Mode detection ────────────────────────────────────────────────────────────
# IN_CONTAINER=1 is set by the test-runner Docker image and tells us to run
# pytest directly. Anywhere else, we shell out to docker compose.
IN_CONTAINER="${IN_CONTAINER:-0}"
COMPOSE_BIN="${COMPOSE_BIN:-docker compose}"

# ── Argument parsing ─────────────────────────────────────────────────────────
CATEGORY="${1:-all}"

case "$CATEGORY" in
    -h|--help)
        sed -n '/^# Usage/,/^# Exit codes/p' "$0" | sed 's/^# \?//'
        exit 0
        ;;
esac

# `all-python` is an internal alias used by the in-container default so the
# Python image alone (which has no Rust / browsers) can still cover the part
# of `all` that lives in Python.
case "$CATEGORY" in
    unit|api|integration|frontend|e2e|all|all-python) ;;
    *)
        error "Unknown category: $CATEGORY"
        echo "Run '$0 --help' for usage."
        exit 2
        ;;
esac

# =============================================================================
# In-container execution: just run pytest for the requested Python category.
# =============================================================================
if [[ "$IN_CONTAINER" == "1" ]]; then
    export API_URL="${API_URL:-http://api-test:8081}"
    export DATABASE_URL="${DATABASE_URL:-postgresql://transitops_app:transitops_secret@db-test:5432/transitops}"
    export ENCRYPTION_KEY="${ENCRYPTION_KEY:-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef}"

    PYTEST_COMMON_ARGS=( --tb=short -v --timeout=30 )

    run_unit() {
        info "═══ UNIT tests (unit_tests/) ═══"
        python3 -m pytest "${PYTEST_COMMON_ARGS[@]}" "$SCRIPT_DIR/unit_tests/"
    }

    run_api() {
        info "═══ API tests (API_tests/) — API_URL=$API_URL ═══"
        # Wait for the API to answer 200/401 on /auth/session before launching.
        local deadline=$((SECONDS + 90))
        until curl -s -o /dev/null -w '%{http_code}' "$API_URL/auth/session" 2>/dev/null \
                | grep -qE '^(200|401)$'; do
            if (( SECONDS >= deadline )); then
                error "API at $API_URL did not become ready within 90s"
                return 2
            fi
            sleep 2
        done
        success "API is reachable at $API_URL"

        PYTHONPATH="$SCRIPT_DIR/API_tests:${PYTHONPATH:-}" \
            python3 -m pytest "${PYTEST_COMMON_ARGS[@]}" --tb=long "$SCRIPT_DIR/API_tests/"
    }

    overall=0
    case "$CATEGORY" in
        unit)        run_unit       || overall=$? ;;
        api)         run_api        || overall=$? ;;
        all-python)  run_unit       || overall=$?
                     run_api        || overall=$? ;;
        *)
            error "Category '$CATEGORY' is not runnable inside the Python test image."
            exit 2
            ;;
    esac
    exit $overall
fi

# =============================================================================
# Host execution: orchestrate via docker compose.
# =============================================================================

# Sanity-check Docker is available before doing anything destructive.
if ! command -v docker >/dev/null 2>&1; then
    error "docker is not installed or not on PATH."
    error "This script runs all tests inside containers — Docker is required."
    exit 2
fi
if ! $COMPOSE_BIN version >/dev/null 2>&1; then
    error "'$COMPOSE_BIN' did not respond. Install Docker Compose v2."
    exit 2
fi

KEEP_STACK="${KEEP_STACK:-0}"
TEARDOWN=true
if [[ "$KEEP_STACK" == "1" ]]; then
    TEARDOWN=false
fi

cleanup() {
    if $TEARDOWN; then
        info "Tearing down test stack..."
        # Stop containers but KEEP named volumes (cargo_cache, cargo_target,
        # cargo_target_frontend) so subsequent runs reuse the build cache.
        # The disposable db-test data lives in a tmpfs and dies automatically.
        $COMPOSE_BIN --profile test down --remove-orphans >/dev/null 2>&1 || true
    else
        warn "KEEP_STACK=1 — leaving test containers running."
    fi
}
trap cleanup EXIT

# ── Per-category orchestrators ───────────────────────────────────────────────

# Unit tests need nothing but the Python image.
run_unit_host() {
    info "═══════════════════════════════════════════"
    info " UNIT tests (containerised)"
    info "═══════════════════════════════════════════"
    $COMPOSE_BIN --profile test build test-runner
    $COMPOSE_BIN --profile test run --rm --no-deps test-runner unit
}

# API tests need the disposable db + api stack.
run_api_host() {
    info "═══════════════════════════════════════════"
    info " API tests (containerised, disposable DB)"
    info "═══════════════════════════════════════════"
    $COMPOSE_BIN --profile test build test-runner api-test
    $COMPOSE_BIN --profile test run --rm test-runner api
}

# Rust integration tests under tests/*.rs — runs cargo test against the
# disposable db-test, all inside a Rust container (compose service).
run_integration_host() {
    info "═══════════════════════════════════════════"
    info " RUST integration tests (containerised)"
    info "═══════════════════════════════════════════"
    $COMPOSE_BIN --profile test run --rm integration-runner
}

# Frontend (Yew) wasm-pack tests — headless Firefox in a purpose-built image.
run_frontend_host() {
    info "═══════════════════════════════════════════"
    info " FRONTEND wasm-pack tests (containerised)"
    info "═══════════════════════════════════════════"
    $COMPOSE_BIN --profile test build frontend-runner
    $COMPOSE_BIN --profile test run --rm --no-deps frontend-runner
}

# E2E — full stack (db-test + api-test + frontend-test) + Playwright runner.
run_e2e_host() {
    info "═══════════════════════════════════════════"
    info " E2E browser tests (Playwright, containerised)"
    info "═══════════════════════════════════════════"
    $COMPOSE_BIN --profile test build api-test frontend-test e2e
    $COMPOSE_BIN --profile test run --rm e2e
}

# ── Driver loop ──────────────────────────────────────────────────────────────
declare -A RESULTS=()

run_one() {
    local cat="$1"; local fn="$2"
    set +e
    "$fn"
    local rc=$?
    set -e
    RESULTS[$cat]=$rc
    if (( rc == 0 )); then
        success "$cat tests PASSED"
    else
        error   "$cat tests FAILED (exit $rc)"
    fi
}

case "$CATEGORY" in
    unit)        run_one unit        run_unit_host ;;
    api)         run_one api         run_api_host ;;
    integration) run_one integration run_integration_host ;;
    frontend)    run_one frontend    run_frontend_host ;;
    e2e)         run_one e2e         run_e2e_host ;;
    all)
        run_one unit        run_unit_host
        run_one api         run_api_host
        run_one integration run_integration_host
        run_one frontend    run_frontend_host
        run_one e2e         run_e2e_host
        ;;
esac

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
info "═══════════════════════════════════════════"
info " Test Summary"
info "═══════════════════════════════════════════"
OVERALL=0
for cat in "${!RESULTS[@]}"; do
    if (( RESULTS[$cat] == 0 )); then
        success "$cat: PASSED"
    else
        error   "$cat: FAILED"
        OVERALL=1
    fi
done

exit $OVERALL
