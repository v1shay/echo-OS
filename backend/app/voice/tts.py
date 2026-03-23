from __future__ import annotations

from collections.abc import AsyncIterator

import httpx

from app.config import Settings


class ElevenLabsTTSClient:
    def __init__(self, settings: Settings):
        if not settings.elevenlabs_api_key:
            raise RuntimeError("ELEVENLABS_API_KEY or VOICE_KEY must be configured.")
        if not settings.elevenlabs_voice_id:
            raise RuntimeError("ELEVENLABS_VOICE_ID must be configured.")
        self.settings = settings

    async def stream(self, text: str) -> AsyncIterator[bytes]:
        url = (
            f"https://api.elevenlabs.io/v1/text-to-speech/{self.settings.elevenlabs_voice_id}/stream"
            "?output_format=mp3_44100_128"
        )
        payload = {
            "text": text,
            "model_id": self.settings.elevenlabs_tts_model,
        }
        headers = {
            "xi-api-key": self.settings.elevenlabs_api_key.get_secret_value(),
            "Accept": "audio/mpeg",
        }
        async with httpx.AsyncClient(timeout=90) as client:
            async with client.stream("POST", url, headers=headers, json=payload) as response:
                response.raise_for_status()
                async for chunk in response.aiter_bytes():
                    if chunk:
                        yield chunk
