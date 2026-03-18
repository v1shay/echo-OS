#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT_DIR"
"$ROOT_DIR/scripts/run_jarvis.sh" scenario-test
"$ROOT_DIR/scripts/run_jarvis.sh" voice-scenario-test
