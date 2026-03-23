from __future__ import annotations

import asyncio
import base64
from dataclasses import dataclass
from typing import Any, Optional

import numpy as np
from fastapi import WebSocket
from fastapi.websockets import WebSocketDisconnect

from app.config import Settings
from app.voice.broker import SessionBroadcaster
from app.voice.normalization import normalize_spoken_command
from app.voice.stt import (
    ElevenLabsRealtimeTranscriber,
    OpenAIBufferedTranscriber,
    SpeechTranscriber,
    VoskStreamingTranscriber,
)
from app.voice.tts import ElevenLabsTTSClient
from app.voice.wakeword import AcousticWakeWordDetector


@dataclass
class VoiceSession:
    session_id: str
    transcriber: SpeechTranscriber
    awake: bool = False
    active_task: Optional[asyncio.Task] = None


class VoiceSessionManager:
    def __init__(self, settings: Settings, runtime: Any, broadcaster: SessionBroadcaster):
        self.settings = settings
        self.runtime = runtime
        self.broadcaster = broadcaster
        self.wakeword = AcousticWakeWordDetector(settings)
        self.tts = ElevenLabsTTSClient(settings) if settings.elevenlabs_voice_id else None
        self.sessions: dict[str, VoiceSession] = {}

    async def handle_connection(self, session_id: str, websocket: WebSocket) -> None:
        await self.broadcaster.connect(session_id, websocket)
        self.runtime.attach_event_sink(session_id, lambda event: self.broadcaster.publish(session_id, event))
        session = await self._build_session(session_id)
        self.sessions[session_id] = session
        await self.broadcaster.publish(session_id, {"type": "status", "state": "idle"})

        try:
            while True:
                payload = await websocket.receive_json()
                message_type = payload.get("type")
                if message_type == "audio_chunk":
                    audio = base64.b64decode(payload["audio_base64"])
                    sample_rate = int(payload.get("sample_rate", 16000))
                    await self._handle_audio_chunk(session, audio, sample_rate)
                elif message_type == "audio_commit":
                    await self._handle_audio_commit(session)
                elif message_type == "text_input":
                    await self._run_agent(session, payload.get("text", ""))
                elif message_type == "reset":
                    session.awake = False
                    await self.broadcaster.publish(session_id, {"type": "status", "state": "idle"})
        except (WebSocketDisconnect, RuntimeError):
            pass
        finally:
            self.runtime.detach_event_sink(session_id)
            self.broadcaster.disconnect(session_id, websocket)
            await session.transcriber.close()
            self.sessions.pop(session_id, None)

    async def _build_session(self, session_id: str) -> VoiceSession:
        async def on_partial(text: str) -> None:
            if text and text != "listening...":
                await self.broadcaster.publish(session_id, {"type": "transcript_partial", "text": text})

        async def on_committed(text: str) -> None:
            if text:
                await self._handle_committed_text(session_id, text)

        transcriber = await self._create_transcriber(on_partial=on_partial, on_committed=on_committed)
        await self.broadcaster.publish(
            session_id,
            {"type": "stt_provider", "provider": transcriber.provider_name},
        )
        return VoiceSession(session_id=session_id, transcriber=transcriber)

    async def _create_transcriber(
        self,
        *,
        on_partial,
        on_committed,
    ) -> SpeechTranscriber:
        if self.settings.enable_ollama_fallback:
            fallback = VoskStreamingTranscriber(
                self.settings,
                on_partial=on_partial,
                on_committed=on_committed,
            )
            await fallback.connect()
            return fallback
        try:
            transcriber = ElevenLabsRealtimeTranscriber(
                self.settings,
                on_partial=on_partial,
                on_committed=on_committed,
            )
            await transcriber.connect()
            return transcriber
        except Exception:
            try:
                fallback = VoskStreamingTranscriber(
                    self.settings,
                    on_partial=on_partial,
                    on_committed=on_committed,
                )
                await fallback.connect()
                return fallback
            except Exception:
                fallback = OpenAIBufferedTranscriber(
                    self.settings,
                    on_partial=on_partial,
                    on_committed=on_committed,
                )
                await fallback.connect()
                return fallback

    async def _handle_audio_chunk(self, session: VoiceSession, audio_bytes: bytes, sample_rate: int) -> None:
        pcm16_mono = self._to_pcm16_mono(audio_bytes, sample_rate)
        if not session.awake and self.wakeword.detect(pcm16_mono):
            session.awake = True
            await self.broadcaster.publish(session.session_id, {"type": "status", "state": "listening"})
        try:
            await session.transcriber.send_audio(pcm16_mono)
        except Exception:
            if session.transcriber.provider_name == "elevenlabs_realtime":
                await session.transcriber.close()
                try:
                    session.transcriber = VoskStreamingTranscriber(
                        self.settings,
                        on_partial=lambda text: self._publish_partial(session.session_id, text),
                        on_committed=lambda text: self._handle_committed_text(session.session_id, text),
                    )
                except Exception:
                    session.transcriber = OpenAIBufferedTranscriber(
                        self.settings,
                        on_partial=lambda text: self._publish_partial(session.session_id, text),
                        on_committed=lambda text: self._handle_committed_text(session.session_id, text),
                    )
                await self.broadcaster.publish(
                    session.session_id,
                    {"type": "stt_provider", "provider": session.transcriber.provider_name},
                )
                try:
                    await session.transcriber.send_audio(pcm16_mono)
                except Exception as fallback_error:
                    await self.broadcaster.publish(
                        session.session_id,
                        {
                            "type": "voice_error",
                            "message": str(fallback_error),
                            "provider": session.transcriber.provider_name,
                        },
                    )
                return
            await self.broadcaster.publish(
                session.session_id,
                {
                    "type": "voice_error",
                    "message": "Speech transcription is unavailable. Falling back to local text input.",
                    "provider": session.transcriber.provider_name,
                },
            )

    async def _publish_partial(self, session_id: str, text: str) -> None:
        if text and text != "listening...":
            await self.broadcaster.publish(session_id, {"type": "transcript_partial", "text": text})

    async def _handle_audio_commit(self, session: VoiceSession) -> None:
        try:
            await session.transcriber.send_audio(b"", commit=True)
        except Exception as error:
            await self.broadcaster.publish(
                session.session_id,
                {
                    "type": "voice_error",
                    "message": str(error),
                    "provider": session.transcriber.provider_name,
                },
            )

    async def _handle_committed_text(self, session_id: str, text: str) -> None:
        session = self.sessions[session_id]
        normalized_text = normalize_spoken_command(text, self.settings)
        await self.broadcaster.publish(session_id, {"type": "transcript_committed", "text": normalized_text})

        normalized = normalized_text.strip()
        lowered = normalized.lower()

        if not session.awake:
            if not self.wakeword.enabled:
                if not lowered.startswith("echo"):
                    await self.broadcaster.publish(session_id, {"type": "status", "state": "idle"})
                    return
                normalized = normalized[4:].lstrip(" ,:.-")
                await self.broadcaster.publish(session_id, {"type": "status", "state": "listening"})
                await self._run_agent(session, normalized or text.strip())
                return
            if lowered.startswith("echo"):
                session.awake = True
                normalized = normalized[4:].lstrip(" ,:.-")
                await self.broadcaster.publish(session_id, {"type": "status", "state": "listening"})
                if normalized:
                    await self._run_agent(session, normalized)
            return

        if session.active_task and not session.active_task.done():
            return

        await self._run_agent(session, normalized)

    async def _run_agent(self, session: VoiceSession, utterance: str) -> None:
        utterance = utterance.strip()
        if not utterance:
            return
        session.awake = False
        if session.active_task and not session.active_task.done():
            return

        async def runner() -> None:
            try:
                await self.broadcaster.publish(session.session_id, {"type": "status", "state": "thinking"})
                result = await self.runtime.run_task(session.session_id, utterance)
                response_text = result["response"]
                await self.broadcaster.publish(
                    session.session_id,
                    {
                        "type": "task_result",
                        "response": response_text,
                        "execution_log": result["execution_log"],
                        "verification": result["verification"],
                    },
                )

                if response_text:
                    if self.tts:
                        await self.broadcaster.publish(session.session_id, {"type": "status", "state": "speaking"})
                        try:
                            audio_chunks: list[bytes] = []
                            async for chunk in self.tts.stream(response_text):
                                audio_chunks.append(chunk)
                                await self.broadcaster.publish(
                                    session.session_id,
                                    {"type": "tts_chunk", "audio_base64": base64.b64encode(chunk).decode("utf-8")},
                                )
                            await self.broadcaster.publish(
                                session.session_id,
                                {
                                    "type": "tts_complete",
                                    "audio_base64": base64.b64encode(b"".join(audio_chunks)).decode("utf-8"),
                                },
                            )
                        except Exception as error:
                            await self.broadcaster.publish(
                                session.session_id,
                                {
                                    "type": "tts_error",
                                    "provider": "elevenlabs_tts",
                                    "message": str(error),
                                },
                            )
                    else:
                        await self.broadcaster.publish(
                            session.session_id,
                            {
                                "type": "tts_error",
                                "provider": "local_speech_synthesis",
                                "message": "Cloud TTS is not configured. Falling back to local speech synthesis.",
                            },
                        )
            except Exception as error:
                error_text = str(error)
                spoken_response = "I couldn't complete that request because the language model is unavailable right now."
                if "insufficient_quota" in error_text or "quota" in error_text.lower():
                    spoken_response = (
                        "I couldn't complete that request because the current OpenAI API key has no available quota."
                    )
                await self.broadcaster.publish(
                    session.session_id,
                    {
                        "type": "task_result",
                        "response": spoken_response,
                        "execution_log": [],
                        "verification": {"completed": False, "error": error_text},
                    },
                )
                await self.broadcaster.publish(
                    session.session_id,
                    {
                        "type": "assistant_text",
                        "text": f"I hit an error while completing that request: {error_text}",
                    },
                )
            finally:
                await self.broadcaster.publish(session.session_id, {"type": "status", "state": "idle"})

        session.active_task = asyncio.create_task(runner())
        await session.active_task

    def _to_pcm16_mono(self, audio_bytes: bytes, sample_rate: int) -> bytes:
        if sample_rate == 16000:
            return audio_bytes
        samples = np.frombuffer(audio_bytes, dtype=np.int16)
        if samples.size == 0:
            return audio_bytes

        duration_seconds = samples.size / float(sample_rate)
        target_length = max(1, int(duration_seconds * 16000))
        source_positions = np.linspace(0, samples.size - 1, num=samples.size)
        target_positions = np.linspace(0, samples.size - 1, num=target_length)
        resampled = np.interp(target_positions, source_positions, samples).astype(np.int16)
        return resampled.tobytes()
