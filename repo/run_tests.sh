#!/usr/bin/env bash
# run_tests.sh — run all TransitOps unit and API tests
#
# Usage
# -----
#   ./run_tests.sh                  Run all tests (unit + API)
#   ./run_tests.sh unit             Run unit tests only
#   ./run_tests.sh api              Run API tests only
#   ./run_tests.sh api --no-deps    Skip dependency/reachability check
#
# Environment variables
# ---------------------
#   API_URL        Base URL for the API  (default: http://localhost:8081)
#   DATABASE_URL   PostgreSQL DSN        (default: postgresql://transitops_app:transitops_secret@localhost:5432/transitops)
#   ENCRYPTION_KEY 64-char hex key       (default: all-zeros dev key)
#
# Exit codes
# ----------
#   0  All selected tests passed
#   1  One or more tests failed
#   2  Setup/dependency error (missing Python, unreachable API, etc.)

set -euo pipefail

# ── Colours ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[PASS]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[FAIL]${NC}  $*"; }

# ── Defaults ───────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUN_UNIT=true
RUN_API=true
CHECK_DEPS=true

export API_URL="${API_URL:-http://localhost:8081}"
export DATABASE_URL="${DATABASE_URL:-postgresql://transitops_app:transitops_secret@localhost:5432/transitops}"
export ENCRYPTION_KEY="${ENCRYPTION_KEY:-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef}"

# ── Argument parsing ───────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        unit)        RUN_UNIT=true;  RUN_API=false ;;
        api)         RUN_UNIT=false; RUN_API=true  ;;
        --no-deps)   CHECK_DEPS=false ;;
        -h|--help)
            sed -n '/^# Usage/,/^$/p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            error "Unknown argument: $arg"
            exit 2
            ;;
    esac
done

# ── Dependency check ───────────────────────────────────────────────────────────
if $CHECK_DEPS; then
    info "Checking dependencies..."

    if ! command -v python3 &>/dev/null; then
        error "python3 not found. Install Python 3.10+ and try again."
        exit 2
    fi

    PYTHON_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
    info "Python version: $PYTHON_VERSION"

    # Verify required packages are installed
    MISSING_PKGS=()
    for pkg in pytest requests psycopg2 argon2 cryptography; do
        if ! python3 -c "import ${pkg//-/_}" &>/dev/null 2>&1; then
            MISSING_PKGS+=("$pkg")
        fi
    done

    if [[ ${#MISSING_PKGS[@]} -gt 0 ]]; then
        warn "Missing Python packages: ${MISSING_PKGS[*]}"
        info "Installing from requirements.txt..."
        pip install -q -r "$SCRIPT_DIR/requirements.txt" || {
            error "pip install failed. Run: pip install -r requirements.txt"
            exit 2
        }
    fi

    if $RUN_API; then
        info "Waiting for API at $API_URL ..."
        MAX_WAIT=60
        DEADLINE=$((SECONDS + MAX_WAIT))
        until curl -s -o /dev/null -w "%{http_code}" "$API_URL/auth/session" \
              2>/dev/null | grep -qE '^(200|401)$'; do
            if [[ $SECONDS -ge $DEADLINE ]]; then
                error "API at $API_URL did not become ready within ${MAX_WAIT}s."
                error "Start the stack with: docker compose up -d"
                exit 2
            fi
            sleep 2
        done
        success "API is reachable."
    fi
fi

# ── Test execution ─────────────────────────────────────────────────────────────
PYTEST_COMMON_ARGS=(
    --tb=short
    -v
    --timeout=30
)

UNIT_EXIT=0
API_EXIT=0

if $RUN_UNIT; then
    echo ""
    info "═══════════════════════════════════════════"
    info " Running UNIT tests  (unit_tests/)"
    info "═══════════════════════════════════════════"
    python3 -m pytest "${PYTEST_COMMON_ARGS[@]}" \
        "$SCRIPT_DIR/unit_tests/" \
        || UNIT_EXIT=$?

    if [[ $UNIT_EXIT -eq 0 ]]; then
        success "Unit tests PASSED"
    else
        error "Unit tests FAILED (exit $UNIT_EXIT)"
    fi
fi

if $RUN_API; then
    echo ""
    info "═══════════════════════════════════════════"
    info " Running API tests   (API_tests/)"
    info " API_URL: $API_URL"
    info "═══════════════════════════════════════════"
    PYTHONPATH="$SCRIPT_DIR/API_tests:${PYTHONPATH:-}" python3 -m pytest "${PYTEST_COMMON_ARGS[@]}" --tb=long \
        "$SCRIPT_DIR/API_tests/" \
        || API_EXIT=$?

    if [[ $API_EXIT -eq 0 ]]; then
        success "API tests PASSED"
    else
        error "API tests FAILED (exit $API_EXIT)"
    fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
info "═══════════════════════════════════════════"
info " Test Summary"
info "═══════════════════════════════════════════"

OVERALL_EXIT=0

if $RUN_UNIT; then
    if [[ $UNIT_EXIT -eq 0 ]]; then
        success "Unit tests:  PASSED"
    else
        error   "Unit tests:  FAILED"
        OVERALL_EXIT=1
    fi
fi

if $RUN_API; then
    if [[ $API_EXIT -eq 0 ]]; then
        success "API tests:   PASSED"
    else
        error   "API tests:   FAILED"
        OVERALL_EXIT=1
    fi
fi

exit $OVERALL_EXIT
