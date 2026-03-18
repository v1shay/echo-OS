from __future__ import annotations

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def _env_path(name: str, default: Path) -> Path:
    value = os.environ.get(name)
    return Path(value).expanduser() if value else default


@dataclass(slots=True)
class BrowserConfig:
    sidecar_host: str
    sidecar_port: int
    sidecar_endpoint: str
    sidecar_dir: Path
    sidecar_entry: Path
    node_binary: Path
    chrome_executable: Path
    chrome_debug_port: int
    chrome_attach_url: str


@dataclass(slots=True)
class VoiceConfig:
    whisper_model_path: Path | None
    recording_path: Path
    sample_rate: int
    chunk_seconds: int
    voice_name: str
    elevenlabs_api_key: str | None
    elevenlabs_voice_id: str | None


@dataclass(slots=True)
class ModelConfig:
    enabled: bool
    llama_server_binary: str
    model_path: Path | None
    endpoint: str
    port: int
    model_name: str
    context_size: int


@dataclass(slots=True)
class PathsConfig:
    artifacts_dir: Path
    memory_path: Path
    ocr_source: Path
    ocr_binary: Path


@dataclass(slots=True)
class JarvisConfig:
    root_dir: Path
    browser: BrowserConfig
    voice: VoiceConfig
    model: ModelConfig
    paths: PathsConfig
    max_step_retries: int
    action_delay_seconds: float
    settle_delay_seconds: float

    @classmethod
    def from_env(cls) -> "JarvisConfig":
        root = repo_root()
        tooling = root / ".tooling"
        node_root = tooling / "node-v24.14.0-darwin-arm64" / "bin"
        models_dir = tooling / "models"
        artifacts_dir = _env_path("JARVIS_ARTIFACTS_DIR", root / "artifacts")
        ocr_bin = tooling / "bin" / "jarvis_ocr"
        sidecar_host = os.environ.get("JARVIS_BROWSER_HOST", "127.0.0.1")
        sidecar_port = int(os.environ.get("JARVIS_BROWSER_PORT", "4317"))
        model_port = int(os.environ.get("JARVIS_LLM_PORT", "8012"))

        browser = BrowserConfig(
            sidecar_host=sidecar_host,
            sidecar_port=sidecar_port,
            sidecar_endpoint=os.environ.get(
                "JARVIS_BROWSER_SIDECAR_ENDPOINT",
                f"http://{sidecar_host}:{sidecar_port}",
            ),
            sidecar_dir=_env_path("JARVIS_BROWSER_SIDECAR_DIR", root / "browser_sidecar"),
            sidecar_entry=_env_path(
                "JARVIS_BROWSER_SIDECAR_ENTRY", root / "browser_sidecar" / "server.mjs"
            ),
            node_binary=_env_path("JARVIS_NODE_BINARY", node_root / "node"),
            chrome_executable=_env_path(
                "JARVIS_CHROME_EXECUTABLE",
                Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            ),
            chrome_debug_port=int(os.environ.get("JARVIS_CHROME_DEBUG_PORT", "9222")),
            chrome_attach_url=os.environ.get(
                "JARVIS_CHROME_ATTACH_URL", "http://127.0.0.1:9222"
            ),
        )

        whisper_path = os.environ.get("JARVIS_WHISPER_MODEL_PATH")
        local_whisper = models_dir / "ggml-tiny.en.bin"
        voice = VoiceConfig(
            whisper_model_path=Path(whisper_path).expanduser()
            if whisper_path
            else (local_whisper if local_whisper.exists() else None),
            recording_path=_env_path(
                "JARVIS_RECORDING_PATH", Path(tempfile.gettempdir()) / "jarvis-input.wav"
            ),
            sample_rate=int(os.environ.get("JARVIS_SAMPLE_RATE", "16000")),
            chunk_seconds=int(os.environ.get("JARVIS_CAPTURE_SECONDS", "4")),
            voice_name=os.environ.get("JARVIS_VOICE_NAME", "Samantha"),
            elevenlabs_api_key=os.environ.get("ELEVENLABS_API_KEY"),
            elevenlabs_voice_id=os.environ.get("ELEVENLABS_VOICE_ID"),
        )

        model_path = os.environ.get("JARVIS_PLANNER_MODEL_PATH") or os.environ.get(
            "JARVIS_WORKER_MODEL_PATH"
        )
        local_model = models_dir / "qwen2.5-1.5b-instruct-q4_k_m.gguf"
        resolved_model = (
            Path(model_path).expanduser()
            if model_path
            else (local_model if local_model.exists() else None)
        )
        llm_enabled = os.environ.get("JARVIS_DISABLE_LLM", "0") != "1"
        model = ModelConfig(
            enabled=llm_enabled,
            llama_server_binary=os.environ.get("JARVIS_LLAMA_SERVER_BINARY", "llama-server"),
            model_path=resolved_model,
            endpoint=os.environ.get("JARVIS_LLM_ENDPOINT", f"http://127.0.0.1:{model_port}/v1"),
            port=model_port,
            model_name=os.environ.get("JARVIS_LLM_MODEL", "jarvis-local"),
            context_size=int(os.environ.get("JARVIS_MODEL_CONTEXT_SIZE", "4096")),
        )

        paths = PathsConfig(
            artifacts_dir=artifacts_dir,
            memory_path=artifacts_dir / "memory.json",
            ocr_source=root / "tools" / "ocr.swift",
            ocr_binary=ocr_bin,
        )

        return cls(
            root_dir=root,
            browser=browser,
            voice=voice,
            model=model,
            paths=paths,
            max_step_retries=int(os.environ.get("JARVIS_MAX_STEP_RETRIES", "3")),
            action_delay_seconds=float(os.environ.get("JARVIS_ACTION_DELAY_SECONDS", "0.35")),
            settle_delay_seconds=float(os.environ.get("JARVIS_SETTLE_DELAY_SECONDS", "1.0")),
        )
