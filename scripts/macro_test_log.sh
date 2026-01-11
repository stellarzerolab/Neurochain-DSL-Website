#!/usr/bin/env bash
set -euo pipefail

# Run a `.nc` script and save stdout to a timestamped file under `logs/`.
# This is meant as a quick regression tool (easy to diff logs across runs).
#
# Usage:
#   bash scripts/macro_test_log.sh                  # defaults to examples/macro_test.nc (release)
#   bash scripts/macro_test_log.sh examples/macro_test_edge.nc
#   bash scripts/macro_test_log.sh -p debug -s examples/macro_test.nc
#
# Options:
#   -s, --script   Path to a .nc file
#   -p, --profile  build profile: release|debug (default: release)
#
# Env:
#   NC_INTENT_THRESHOLD      macro intent threshold (read by neurochain)
#   NEUROCHAIN_RAW_LOG=1     writes logs/macro_raw_latest.log
#   NEUROCHAIN_OUTPUT_LOG=1  writes logs/run_latest.log
#   CARGO_BIN=...            override cargo path (useful if `cargo` is not on PATH)

cd "$(dirname "$0")/.."

usage() {
  cat <<'EOF'
Usage:
  bash scripts/macro_test_log.sh [SCRIPT]
  bash scripts/macro_test_log.sh -s SCRIPT [-p release|debug]

Examples:
  bash scripts/macro_test_log.sh
  bash scripts/macro_test_log.sh examples/macro_test_edge.nc
  bash scripts/macro_test_log.sh -p debug -s examples/macro_test.nc

Environment:
  NC_INTENT_THRESHOLD      Macro intent threshold
  NEUROCHAIN_RAW_LOG=1     Writes logs/macro_raw_latest.log
  NEUROCHAIN_OUTPUT_LOG=1  Writes logs/run_latest.log
  CARGO_BIN=...            Override cargo path
EOF
}

PROFILE="release"
SCRIPT_PATH="examples/macro_test.nc"

while [ $# -gt 0 ]; do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    -p | --profile)
      PROFILE="${2:-}"
      shift 2
      ;;
    -s | --script)
      SCRIPT_PATH="${2:-}"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      SCRIPT_PATH="$1"
      shift
      ;;
  esac
done

# Normalize Windows-style path separators if needed (PowerShell → bash).
SCRIPT_PATH="${SCRIPT_PATH//\\//}"

TS=$(date +"%Y%m%d_%H%M%S")
LOG_DIR="logs"
LOG_FILE="${LOG_DIR}/macro_test_${PROFILE}_${TS}.log"

mkdir -p "$LOG_DIR"

echo "Writing log to: $LOG_FILE"

{
  echo "Script: $SCRIPT_PATH"
  echo "Profile: $PROFILE"
  echo "NC_INTENT_THRESHOLD=${NC_INTENT_THRESHOLD:-unset}"
  echo "NEUROCHAIN_RAW_LOG=${NEUROCHAIN_RAW_LOG:-unset}"
  echo "NEUROCHAIN_OUTPUT_LOG=${NEUROCHAIN_OUTPUT_LOG:-unset}"
} | tee "$LOG_FILE"

# Find cargo (Windows Git Bash may not have it on PATH)
CARGO_BIN="${CARGO_BIN:-cargo}"
if ! command -v "$CARGO_BIN" >/dev/null 2>&1; then
  for guess in \
    "$HOME/.cargo/bin/cargo" \
    "$HOME/.cargo/bin/cargo.exe" \
    "/c/Users/${USERNAME:-$USER}/.cargo/bin/cargo" \
    "/c/Users/${USERNAME:-$USER}/.cargo/bin/cargo.exe" \
    "/mnt/c/Users/${USERNAME:-$USER}/.cargo/bin/cargo" \
    "/mnt/c/Users/${USERNAME:-$USER}/.cargo/bin/cargo.exe"; do
    if [ -x "$guess" ]; then
      CARGO_BIN="$guess"
      break
    fi
  done
fi

echo "Using cargo: $CARGO_BIN" | tee -a "$LOG_FILE"

if [ ! -f "$SCRIPT_PATH" ]; then
  echo "Error: script not found: $SCRIPT_PATH" | tee -a "$LOG_FILE"
  exit 1
fi

case "$PROFILE" in
  release)
    "$CARGO_BIN" run --release --bin neurochain -- "$SCRIPT_PATH" | tee -a "$LOG_FILE"
    ;;
  debug)
    "$CARGO_BIN" run --bin neurochain -- "$SCRIPT_PATH" | tee -a "$LOG_FILE"
    ;;
  *)
    echo "Error: unknown profile: $PROFILE (expected: release|debug)" | tee -a "$LOG_FILE"
    exit 2
    ;;
esac

echo "Done. Log saved to $LOG_FILE"
