from __future__ import annotations

from functools import lru_cache
from pathlib import Path
from typing import List, Optional

from pydantic import AliasChoices, Field, SecretStr
from pydantic_settings import BaseSettings, SettingsConfigDict

ROOT_DIR = Path(__file__).resolve().parents[2]


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=str(ROOT_DIR / ".env"),
        env_file_encoding="utf-8",
        extra="ignore",
    )

    app_name: str = "Echo OS"
    environment: str = "development"

    backend_host: str = Field(default="127.0.0.1", validation_alias=AliasChoices("BACKEND_HOST"))
    backend_port: int = Field(default=8000, validation_alias=AliasChoices("BACKEND_PORT"))

    openai_api_key: Optional[SecretStr] = Field(
        default=None,
        validation_alias=AliasChoices("OPENAI_API_KEY", "CHATGPT_KEY"),
    )
    openai_model: str = Field(default="gpt-4.1", validation_alias=AliasChoices("OPENAI_MODEL"))
    openai_stt_model: str = Field(
        default="gpt-4o-mini-transcribe",
        validation_alias=AliasChoices("OPENAI_STT_MODEL"),
    )
    verifier_model: str = Field(
        default="gpt-4o-mini",
        validation_alias=AliasChoices("OPENAI_VERIFIER_MODEL"),
    )
    embedding_model: str = Field(
        default="text-embedding-3-small",
        validation_alias=AliasChoices("OPENAI_EMBEDDING_MODEL"),
    )
    enable_ollama_fallback: bool = Field(
        default=True,
        validation_alias=AliasChoices("ECHO_ENABLE_OLLAMA_FALLBACK"),
    )
    ollama_base_url: str = Field(
        default="http://127.0.0.1:11434",
        validation_alias=AliasChoices("OLLAMA_BASE_URL"),
    )
    ollama_model: str = Field(
        default="deepseek-coder-v2:16b",
        validation_alias=AliasChoices("OLLAMA_MODEL"),
    )

    elevenlabs_api_key: Optional[SecretStr] = Field(
        default=None,
        validation_alias=AliasChoices("ELEVENLABS_API_KEY", "VOICE_KEY"),
    )
    elevenlabs_voice_id: Optional[str] = Field(
        default=None,
        validation_alias=AliasChoices("ELEVENLABS_VOICE_ID"),
    )
    elevenlabs_tts_model: str = Field(
        default="eleven_multilingual_v2",
        validation_alias=AliasChoices("ELEVENLABS_TTS_MODEL"),
    )
    elevenlabs_stt_model: str = Field(
        default="scribe_v2_realtime",
        validation_alias=AliasChoices("ELEVENLABS_STT_MODEL"),
    )
    vosk_model_path: Path = Field(
        default_factory=lambda: ROOT_DIR / "backend" / "models" / "vosk-model-small-en-us-0.15",
        validation_alias=AliasChoices("VOSK_MODEL_PATH"),
    )

    chroma_persist_directory: Path = Field(
        default=Path("./backend/chroma"),
        validation_alias=AliasChoices("CHROMA_PERSIST_DIRECTORY"),
    )
    chroma_collection_name: str = Field(
        default="echo_memory",
        validation_alias=AliasChoices("CHROMA_COLLECTION_NAME"),
    )

    require_confirmation_for_side_effects: bool = Field(
        default=True,
        validation_alias=AliasChoices("ECHO_REQUIRE_CONFIRMATION_FOR_SIDE_EFFECTS"),
    )
    allowed_roots: str = Field(
        default=str(Path.home()),
        validation_alias=AliasChoices("ECHO_ALLOWED_ROOTS"),
    )
    workspace_root: Path = Field(
        default_factory=lambda: ROOT_DIR,
        validation_alias=AliasChoices("ECHO_WORKSPACE_ROOT"),
    )
    playwright_headless: bool = Field(
        default=False,
        validation_alias=AliasChoices("ECHO_PLAYWRIGHT_HEADLESS"),
    )

    wakeword_provider: str = Field(
        default="openwakeword",
        validation_alias=AliasChoices("ECHO_WAKEWORD_PROVIDER"),
    )
    wakeword_model_path: Optional[Path] = Field(
        default=None,
        validation_alias=AliasChoices("ECHO_WAKEWORD_MODEL_PATH"),
    )
    wakeword_threshold: float = Field(
        default=0.5,
        validation_alias=AliasChoices("ECHO_WAKEWORD_THRESHOLD"),
    )

    twilio_account_sid: Optional[str] = Field(
        default=None,
        validation_alias=AliasChoices("TWILIO_ACCOUNT_SID"),
    )
    twilio_auth_token: Optional[SecretStr] = Field(
        default=None,
        validation_alias=AliasChoices("TWILIO_AUTH_TOKEN"),
    )
    twilio_sms_from: Optional[str] = Field(
        default=None,
        validation_alias=AliasChoices("TWILIO_SMS_FROM"),
    )
    twilio_whatsapp_from: Optional[str] = Field(
        default=None,
        validation_alias=AliasChoices("TWILIO_WHATSAPP_FROM"),
    )

    google_client_secret_path: Optional[Path] = Field(
        default=None,
        validation_alias=AliasChoices("GOOGLE_CLIENT_SECRET_PATH"),
    )
    google_token_path: Optional[Path] = Field(
        default=None,
        validation_alias=AliasChoices("GOOGLE_TOKEN_PATH"),
    )
    gmail_sender: Optional[str] = Field(
        default=None,
        validation_alias=AliasChoices("GMAIL_SENDER"),
    )
    google_calendar_id: str = Field(
        default="primary",
        validation_alias=AliasChoices("GOOGLE_CALENDAR_ID"),
    )

    agent_iteration_limit: int = 8
    short_term_memory_window: int = 10

    @property
    def allowed_root_paths(self) -> List[Path]:
        raw = [part.strip() for part in self.allowed_roots.split(",") if part.strip()]
        paths = [Path(value).expanduser().resolve() for value in raw]
        workspace = self.workspace_root.expanduser().resolve()
        if workspace not in paths:
            paths.append(workspace)
        return paths

    def resolve_path(self, path: Path) -> Path:
        candidate = path.expanduser()
        if candidate.is_absolute():
            return candidate.resolve()
        return (ROOT_DIR / candidate).resolve()


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()
