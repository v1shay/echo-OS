#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_DIR="$ROOT_DIR/.venv"
LOCAL_NODE_DIR="$ROOT_DIR/.tooling/node-v24.14.0-darwin-arm64/bin"
LOCAL_QWEN_MODEL="$ROOT_DIR/.tooling/models/qwen2.5-1.5b-instruct-q4_k_m.gguf"
LOCAL_WHISPER_MODEL="$ROOT_DIR/.tooling/models/ggml-tiny.en.bin"
ENV_FILE="$ROOT_DIR/.env.jarvis"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

if [[ ! -x "$VENV_DIR/bin/python" ]]; then
  echo "Jarvis is not bootstrapped yet. Run $ROOT_DIR/scripts/bootstrap_macos.sh first." >&2
  exit 1
fi

if [[ -d "$LOCAL_NODE_DIR" ]]; then
  export PATH="$LOCAL_NODE_DIR:$PATH"
  export JARVIS_NODE_BINARY="${JARVIS_NODE_BINARY:-$LOCAL_NODE_DIR/node}"
fi

if [[ -f "$LOCAL_QWEN_MODEL" ]]; then
  export JARVIS_PLANNER_MODEL_PATH="${JARVIS_PLANNER_MODEL_PATH:-$LOCAL_QWEN_MODEL}"
  export JARVIS_WORKER_MODEL_PATH="${JARVIS_WORKER_MODEL_PATH:-$LOCAL_QWEN_MODEL}"
fi

if [[ -f "$LOCAL_WHISPER_MODEL" ]]; then
  export JARVIS_WHISPER_MODEL_PATH="${JARVIS_WHISPER_MODEL_PATH:-$LOCAL_WHISPER_MODEL}"
fi

cd "$ROOT_DIR"
exec "$VENV_DIR/bin/python" main.py "$@"
