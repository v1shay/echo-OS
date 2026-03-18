from __future__ import annotations

import json
import subprocess
import tempfile
import wave
from pathlib import Path
from urllib import request

import numpy
import sounddevice

from .config import VoiceConfig


class VoicePipeline:
    def __init__(self, config: VoiceConfig) -> None:
        self.config = config

    def record_wav(self, seconds: int | None = None) -> Path:
        seconds = seconds or self.config.chunk_seconds
        frames = int(seconds * self.config.sample_rate)
        audio = sounddevice.rec(
            frames,
            samplerate=self.config.sample_rate,
            channels=1,
            dtype="int16",
        )
        sounddevice.wait()
        path = self.config.recording_path
        path.parent.mkdir(parents=True, exist_ok=True)
        with wave.open(str(path), "wb") as wav_file:
            wav_file.setnchannels(1)
            wav_file.setsampwidth(2)
            wav_file.setframerate(self.config.sample_rate)
            wav_file.writeframes(numpy.asarray(audio).tobytes())
        return path

    def transcribe(self, wav_path: Path) -> str:
        if self.config.whisper_model_path is None:
            raise RuntimeError("Whisper model is not configured.")
        prefix = wav_path.with_suffix("")
        txt_path = prefix.with_suffix(".txt")
        if txt_path.exists():
            txt_path.unlink()
        cmd = [
            "whisper-cli",
            "-m",
            str(self.config.whisper_model_path),
            "-f",
            str(wav_path),
            "-ng",
            "-nt",
            "-otxt",
            "-of",
            str(prefix),
        ]
        subprocess.run(
            cmd,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return txt_path.read_text(encoding="utf-8").strip()

    def listen_once(self, seconds: int | None = None) -> str:
        wav_path = self.record_wav(seconds=seconds)
        return self.transcribe(wav_path)

    def synthesize_test_wav(self, text: str, output_path: Path) -> Path:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as temp_dir:
            aiff_path = Path(temp_dir) / "prompt.aiff"
            subprocess.run(
                ["/usr/bin/say", "-v", self.config.voice_name, "-o", str(aiff_path), text],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            subprocess.run(
                [
                    "/usr/bin/afconvert",
                    "-f",
                    "WAVE",
                    "-d",
                    "LEI16@16000",
                    "-c",
                    "1",
                    str(aiff_path),
                    str(output_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        return output_path


class Speaker:
    def __init__(self, config: VoiceConfig) -> None:
        self.config = config

    def speak(self, text: str) -> None:
        if self.config.elevenlabs_api_key and self.config.elevenlabs_voice_id:
            if self._speak_elevenlabs(text):
                return
        self._speak_macos(text)

    def _speak_elevenlabs(self, text: str) -> bool:
        payload = json.dumps(
            {
                "text": text,
                "model_id": "eleven_flash_v2_5",
                "voice_settings": {"stability": 0.35, "similarity_boost": 0.75},
            }
        ).encode("utf-8")
        req = request.Request(
            f"https://api.elevenlabs.io/v1/text-to-speech/{self.config.elevenlabs_voice_id}",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "xi-api-key": self.config.elevenlabs_api_key,
                "Accept": "audio/mpeg",
            },
            method="POST",
        )
        out_path = Path("/tmp/jarvis-tts.mp3")
        try:
            with request.urlopen(req, timeout=20) as response:
                out_path.write_bytes(response.read())
            subprocess.run(["afplay", str(out_path)], check=True)
            return True
        except Exception:
            return False

    def _speak_macos(self, text: str) -> None:
        subprocess.run(
            ["/usr/bin/say", "-v", self.config.voice_name, text],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
