#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLING_DIR="$ROOT_DIR/.tooling"
MODEL_DIR="$TOOLING_DIR/models"
BIN_DIR="$TOOLING_DIR/bin"
VENV_DIR="$ROOT_DIR/.venv"
NODE_VERSION="v24.14.0"
NODE_DIST="node-${NODE_VERSION}-darwin-arm64"
NODE_TARBALL="$TOOLING_DIR/${NODE_DIST}.tar.gz"
NODE_DIR="$TOOLING_DIR/${NODE_DIST}"
NODE_URL="https://nodejs.org/dist/${NODE_VERSION}/${NODE_DIST}.tar.gz"
QWEN_MODEL_PATH="$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
QWEN_MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf?download=true"
WHISPER_MODEL_PATH="$MODEL_DIR/ggml-tiny.en.bin"
WHISPER_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin"
CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
OCR_BIN="$BIN_DIR/jarvis_ocr"

log() {
  printf '\n[%s] %s\n' "jarvis-bootstrap" "$1"
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

need_cmd curl
need_cmd tar
need_cmd python3
need_cmd swiftc

mkdir -p "$TOOLING_DIR" "$MODEL_DIR" "$BIN_DIR"

NODE_BIN=""
NPM_BIN=""

if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
  NODE_BIN="$(command -v node)"
  NPM_BIN="$(command -v npm)"
  log "Using system Node at $NODE_BIN"
else
  if [[ ! -x "$NODE_DIR/bin/node" ]]; then
    log "Downloading local Node runtime into $NODE_DIR"
    curl -L "$NODE_URL" -o "$NODE_TARBALL"
    rm -rf "$NODE_DIR"
    tar -xzf "$NODE_TARBALL" -C "$TOOLING_DIR"
  fi
  NODE_BIN="$NODE_DIR/bin/node"
  NPM_BIN="$NODE_DIR/bin/npm"
  log "Using local Node at $NODE_BIN"
fi

export PATH="$(dirname "$NODE_BIN"):$PATH"

if [[ ! -d "$VENV_DIR" ]]; then
  log "Creating Python virtualenv"
  python3 -m venv "$VENV_DIR"
fi

log "Installing Python dependencies"
"$VENV_DIR/bin/python" -m pip install --upgrade pip
"$VENV_DIR/bin/python" -m pip install -r "$ROOT_DIR/requirements.txt"

log "Installing browser sidecar dependencies"
"$NPM_BIN" ci --prefix "$ROOT_DIR/browser_sidecar"

if [[ "${JARVIS_SETUP_LOCAL_MODELS:-1}" == "1" ]]; then
  if [[ ! -f "$QWEN_MODEL_PATH" ]]; then
    log "Downloading local Qwen planner model"
    curl -L "$QWEN_MODEL_URL" -o "$QWEN_MODEL_PATH"
  fi

  if [[ ! -f "$WHISPER_MODEL_PATH" ]]; then
    log "Downloading local Whisper model"
    curl -L "$WHISPER_MODEL_URL" -o "$WHISPER_MODEL_PATH"
  fi
else
  log "Skipping local model downloads because JARVIS_SETUP_LOCAL_MODELS=0"
fi

log "Compiling OCR helper"
swiftc -framework Vision -framework AppKit "$ROOT_DIR/tools/ocr.swift" -o "$OCR_BIN"

if [[ ! -x "$CHROME_PATH" ]]; then
  echo "Google Chrome was not found at $CHROME_PATH" >&2
  echo "Install Chrome or set JARVIS_CHROME_EXECUTABLE before launching Jarvis." >&2
fi

cat <<EOF

Bootstrap complete.

Launch:
  $ROOT_DIR/scripts/run_jarvis.sh run --voice

Run local verification:
  $ROOT_DIR/scripts/doctor.sh

Notes:
  - Grant Accessibility, Screen Recording, Microphone, and Automation permissions when macOS prompts.
  - If you have ElevenLabs credentials, export ELEVENLABS_API_KEY and ELEVENLABS_VOICE_ID for lower-latency cloud TTS.
  - The local Qwen model is optional fallback brain capacity; deterministic planning handles the demo-critical flows too.

EOF
