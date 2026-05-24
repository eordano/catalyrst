#!/usr/bin/env bash
# scripts/schemathesis/run.sh
#
# Bootstrap a venv and run schemathesis against a catalyrst (or peer) instance,
# using the OpenAPI spec at docs/openapi.yaml by default.
#
# Usage:
#   ./scripts/schemathesis/run.sh --target http://127.0.0.1:5141
#   ./scripts/schemathesis/run.sh --target https://peer.decentraland.org \
#       --hypothesis-max-examples 5 --workers 1
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
VENV_DIR="${SCRIPT_DIR}/.venv"
REQUIREMENTS="${SCRIPT_DIR}/requirements.txt"

# Defaults
TARGET=""
SPEC="${REPO_ROOT}/docs/openapi.yaml"
CHECKS="all"
MAX_EXAMPLES="50"
WORKERS="2"
REPORT=""

usage() {
    cat >&2 <<EOF
Usage: $(basename "$0") --target <url> [options]

Required:
  --target <url>                 Base URL of the running API (e.g. http://127.0.0.1:5141)

Options:
  --spec <path>                  OpenAPI spec path (default: docs/openapi.yaml)
  --checks <list>                Comma-separated schemathesis checks (default: all)
  --hypothesis-max-examples <N>  Max examples per operation (default: 50)
  --workers <N>                  Parallel workers (default: 2)
  --report <path>                Optional JUnit XML report path
  -h, --help                     Show this help

Examples:
  $(basename "$0") --target http://127.0.0.1:5141
  $(basename "$0") --target https://peer.decentraland.org --hypothesis-max-examples 5 --workers 1
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --spec)
            SPEC="$2"
            shift 2
            ;;
        --checks)
            CHECKS="$2"
            shift 2
            ;;
        --hypothesis-max-examples)
            MAX_EXAMPLES="$2"
            shift 2
            ;;
        --workers)
            WORKERS="$2"
            shift 2
            ;;
        --report)
            REPORT="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown arg: $1" >&2
            usage
            exit 64
            ;;
    esac
done

if [[ -z "${TARGET}" ]]; then
    echo "error: --target is required" >&2
    usage
    exit 64
fi

if [[ ! -f "${SPEC}" ]]; then
    echo "error: spec not found at ${SPEC}" >&2
    echo "       docs/openapi.yaml not found - generate it first" >&2
    exit 2
fi

# Bootstrap venv on first run.
if [[ ! -d "${VENV_DIR}" ]]; then
    echo "==> creating venv at ${VENV_DIR}" >&2
    python3 -m venv "${VENV_DIR}"
fi

# shellcheck disable=SC1091
source "${VENV_DIR}/bin/activate"

# Install/upgrade requirements if schemathesis is missing or requirements changed.
NEED_INSTALL=0
if ! python3 -c "import schemathesis" >/dev/null 2>&1; then
    NEED_INSTALL=1
elif [[ -f "${VENV_DIR}/.requirements.sha" ]]; then
    CURRENT_SHA="$(sha256sum "${REQUIREMENTS}" | awk '{print $1}')"
    CACHED_SHA="$(cat "${VENV_DIR}/.requirements.sha")"
    if [[ "${CURRENT_SHA}" != "${CACHED_SHA}" ]]; then
        NEED_INSTALL=1
    fi
else
    NEED_INSTALL=1
fi

if [[ "${NEED_INSTALL}" -eq 1 ]]; then
    echo "==> installing requirements" >&2
    python3 -m pip install --quiet --upgrade pip
    python3 -m pip install --quiet -r "${REQUIREMENTS}"
    sha256sum "${REQUIREMENTS}" | awk '{print $1}' > "${VENV_DIR}/.requirements.sha"
fi

# Build args. Custom checks live in checks.py and are auto-loaded via
# SCHEMATHESIS_HOOKS so the @schemathesis.check decorators register before run.
export SCHEMATHESIS_HOOKS="checks"
export PYTHONPATH="${SCRIPT_DIR}${PYTHONPATH:+:${PYTHONPATH}}"

ARGS=(
    run
    "${SPEC}"
    --base-url "${TARGET}"
    --checks "${CHECKS}"
    --hypothesis-max-examples "${MAX_EXAMPLES}"
    --workers "${WORKERS}"
)

if [[ -n "${REPORT}" ]]; then
    ARGS+=(--junit-xml "${REPORT}")
fi

echo "==> schemathesis ${ARGS[*]}" >&2
exec python3 -m schemathesis "${ARGS[@]}"
