from __future__ import annotations

import asyncio
import base64
import io
import json
import ssl
import wave
from collections.abc import Awaitable, Callable

import certifi
import httpx
import websockets
from openai import AsyncOpenAI
from vosk import KaldiRecognizer, Model

from app.config import Settings

PartialCallback = Callable[[str], Awaitable[None]]
CommittedCallback = Callable[[str], Awaitable[None]]


class SpeechTranscriber:
    provider_name: str = "unknown"

    async def connect(self) -> None:
        return None

    async def send_audio(self, pcm16_mono: bytes, *, commit: bool = False) -> None:
        raise NotImplementedError

    async def close(self) -> None:
        return None


class ElevenLabsRealtimeTranscriber(SpeechTranscriber):
    provider_name = "elevenlabs_realtime"

    def __init__(
        self,
        settings: Settings,
        *,
        on_partial: PartialCallback,
        on_committed: CommittedCallback,
    ):
        if not settings.elevenlabs_api_key:
            raise RuntimeError("ELEVENLABS_API_KEY or VOICE_KEY must be configured.")
        self.settings = settings
        self.on_partial = on_partial
        self.on_committed = on_committed
        self.websocket = None
        self.receiver_task = None

    async def connect(self) -> None:
        ssl_context = ssl.create_default_context(cafile=certifi.where())
        url = (
            "wss://api.elevenlabs.io/v1/speech-to-text/realtime"
            f"?model_id={self.settings.elevenlabs_stt_model}"
            "&audio_format=pcm_16000"
            "&include_timestamps=false"
            "&commit_strategy=vad"
            "&vad_silence_threshold_secs=1.0"
        )
        self.websocket = await websockets.connect(
            url,
            additional_headers={"xi-api-key": self.settings.elevenlabs_api_key.get_secret_value()},
            max_size=None,
            ssl=ssl_context,
        )
        self.receiver_task = asyncio.create_task(self._listen())

    async def send_audio(self, pcm16_mono: bytes, *, commit: bool = False) -> None:
        if not self.websocket:
            raise RuntimeError("Realtime transcriber is not connected.")
        payload = {
            "message_type": "input_audio_chunk",
            "audio_base_64": base64.b64encode(pcm16_mono).decode("utf-8"),
            "sample_rate": 16000,
        }
        if commit:
            payload["commit"] = True
        await self.websocket.send(json.dumps(payload))

    async def close(self) -> None:
        if self.receiver_task:
            self.receiver_task.cancel()
        if self.websocket:
            await self.websocket.close()

    async def _listen(self) -> None:
        assert self.websocket is not None
        async for message in self.websocket:
            payload = json.loads(message)
            message_type = payload.get("message_type")
            if message_type == "partial_transcript":
                await self.on_partial(payload.get("text", ""))
            elif message_type in {"committed_transcript", "committed_transcript_with_timestamps"}:
                await self.on_committed(payload.get("text", ""))


class OpenAIBufferedTranscriber(SpeechTranscriber):
    provider_name = "openai_batch"

    def __init__(
        self,
        settings: Settings,
        *,
        on_partial: PartialCallback,
        on_committed: CommittedCallback,
    ):
        if not settings.openai_api_key:
            raise RuntimeError("OPENAI_API_KEY or CHATGPT_KEY must be configured.")
        self.settings = settings
        self.on_partial = on_partial
        self.on_committed = on_committed
        self.client = AsyncOpenAI(api_key=settings.openai_api_key.get_secret_value())
        self.buffer = bytearray()
        self.in_speech = False
        self.speech_ms = 0.0
        self.silence_ms = 0.0
        self.energy_threshold = 700.0
        self.min_speech_ms = 250.0
        self.commit_silence_ms = 900.0
        self.max_utterance_ms = 12_000.0
        self._transcription_task: asyncio.Task | None = None

    async def send_audio(self, pcm16_mono: bytes, *, commit: bool = False) -> None:
        import numpy as np

        samples = np.frombuffer(pcm16_mono, dtype=np.int16)
        if samples.size == 0 and not commit:
            return
        chunk_ms = (samples.size / 16000.0) * 1000.0
        rms = float(np.sqrt(np.mean(np.square(samples.astype(np.float32))))) if samples.size else 0.0

        if rms >= self.energy_threshold:
            if not self.in_speech:
                self.buffer = bytearray()
                self.speech_ms = 0.0
                self.silence_ms = 0.0
                self.in_speech = True
            self.buffer.extend(pcm16_mono)
            self.speech_ms += chunk_ms
            self.silence_ms = 0.0
            await self.on_partial("listening...")
        elif self.in_speech:
            self.buffer.extend(pcm16_mono)
            self.silence_ms += chunk_ms

        should_commit = commit
        if self.in_speech and self.speech_ms >= self.min_speech_ms and self.silence_ms >= self.commit_silence_ms:
            should_commit = True
        if self.in_speech and self.speech_ms >= self.max_utterance_ms:
            should_commit = True

        if should_commit and self.in_speech and self.buffer:
            audio_bytes = bytes(self.buffer)
            self.buffer = bytearray()
            self.in_speech = False
            self.speech_ms = 0.0
            self.silence_ms = 0.0
            await self._transcribe(audio_bytes)

    async def close(self) -> None:
        if self._transcription_task and not self._transcription_task.done():
            self._transcription_task.cancel()

    async def _transcribe(self, pcm16_mono: bytes) -> None:
        wav_bytes = _pcm16_to_wav(pcm16_mono, sample_rate=16000)
        response = await self.client.audio.transcriptions.create(
            file=("utterance.wav", wav_bytes, "audio/wav"),
            model=self.settings.openai_stt_model,
            response_format="json",
            language="en",
        )
        text = response.text if hasattr(response, "text") else str(response)
        text = text.strip()
        if text:
            await self.on_committed(text)


class VoskStreamingTranscriber(SpeechTranscriber):
    provider_name = "vosk_local"
    _model_cache: dict[str, Model] = {}

    def __init__(
        self,
        settings: Settings,
        *,
        on_partial: PartialCallback,
        on_committed: CommittedCallback,
    ):
        self.settings = settings
        self.on_partial = on_partial
        self.on_committed = on_committed
        model_path = str(settings.resolve_path(settings.vosk_model_path))
        if model_path not in self._model_cache:
            self._model_cache[model_path] = Model(model_path)
        self.recognizer = KaldiRecognizer(self._model_cache[model_path], 16000)
        self.recognizer.SetWords(False)

    async def send_audio(self, pcm16_mono: bytes, *, commit: bool = False) -> None:
        import json as _json

        if commit:
            result = _json.loads(self.recognizer.FinalResult())
            text = (result.get("text") or "").strip()
            if text:
                await self.on_committed(text)
            return

        if not pcm16_mono:
            return

        accepted = self.recognizer.AcceptWaveform(pcm16_mono)
        if accepted:
            result = _json.loads(self.recognizer.Result())
            text = (result.get("text") or "").strip()
            if text:
                await self.on_committed(text)
            return

        partial = _json.loads(self.recognizer.PartialResult()).get("partial", "").strip()
        if partial:
            await self.on_partial(partial)


class ElevenLabsBatchTranscriber:
    def __init__(self, settings: Settings):
        if not settings.elevenlabs_api_key:
            raise RuntimeError("ELEVENLABS_API_KEY or VOICE_KEY must be configured.")
        self.settings = settings

    async def transcribe_wav(self, wav_bytes: bytes) -> str:
        async with httpx.AsyncClient(timeout=60) as client:
            files = {"file": ("utterance.wav", wav_bytes, "audio/wav")}
            data = {"model_id": self.settings.elevenlabs_stt_model}
            response = await client.post(
                "https://api.elevenlabs.io/v1/speech-to-text",
                headers={"xi-api-key": self.settings.elevenlabs_api_key.get_secret_value()},
                files=files,
                data=data,
            )
            response.raise_for_status()
            return response.json().get("text", "")


def _pcm16_to_wav(pcm16_mono: bytes, *, sample_rate: int) -> bytes:
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav_file:
        wav_file.setnchannels(1)
        wav_file.setsampwidth(2)
        wav_file.setframerate(sample_rate)
        wav_file.writeframes(pcm16_mono)
    return buffer.getvalue()
