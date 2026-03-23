from __future__ import annotations

from contextlib import asynccontextmanager

from fastapi import FastAPI, WebSocket

from app.api.schemas import TaskRequest, TaskResponse
from app.config import get_settings
from app.demo.runtime import HardcodedDemoRuntime
from app.utils.logging import configure_logging
from app.voice.broker import SessionBroadcaster
from app.voice.manager import VoiceSessionManager

settings = get_settings()
configure_logging()


@asynccontextmanager
async def lifespan(app: FastAPI):
    broadcaster = SessionBroadcaster()
    runtime = HardcodedDemoRuntime(settings)
    voice_manager = VoiceSessionManager(settings, runtime, broadcaster)
    app.state.runtime = runtime
    app.state.voice_manager = voice_manager
    yield


app = FastAPI(title=settings.app_name, lifespan=lifespan)


@app.get("/health")
async def health() -> dict:
    return {
        "status": "ok",
        "app": settings.app_name,
        "workspace_root": str(settings.workspace_root),
        "memory_collection": settings.chroma_collection_name,
    }


@app.post("/api/tasks", response_model=TaskResponse)
async def run_task(payload: TaskRequest) -> TaskResponse:
    result = await app.state.runtime.run_task(payload.session_id, payload.input_text)
    return TaskResponse.model_validate(result)


@app.websocket("/ws/voice/{session_id}")
async def voice_socket(websocket: WebSocket, session_id: str) -> None:
    await app.state.voice_manager.handle_connection(session_id, websocket)
