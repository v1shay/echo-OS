from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path
from urllib import error, request

from .config import JarvisConfig


class LocalLlmClient:
    def __init__(self, config: JarvisConfig) -> None:
        self.config = config
        self._process: subprocess.Popen[str] | None = None

    def available(self) -> bool:
        return (
            self.config.model.enabled
            and self.config.model.model_path is not None
            and self.config.model.model_path.exists()
        )

    def ensure_server(self, timeout_seconds: float = 20.0) -> bool:
        if not self.available():
            return False
        if self._healthcheck():
            return True

        cmd = [
            self.config.model.llama_server_binary,
            "--host",
            "127.0.0.1",
            "--port",
            str(self.config.model.port),
            "-m",
            str(self.config.model.model_path),
            "-c",
            str(self.config.model.context_size),
        ]
        log_path = self.config.paths.artifacts_dir / "llama-server.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        log_file = log_path.open("a", encoding="utf-8")
        self._process = subprocess.Popen(
            cmd,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        deadline = time.time() + timeout_seconds
        while time.time() < deadline:
            if self._healthcheck():
                return True
            time.sleep(0.5)
        return False

    def _healthcheck(self) -> bool:
        try:
            with request.urlopen(
                f"{self.config.model.endpoint}/models", timeout=2
            ) as response:
                return response.status == 200
        except Exception:
            return False

    def json_completion(self, system_prompt: str, user_prompt: str) -> dict | None:
        content = self._chat_completion(system_prompt=system_prompt, user_prompt=user_prompt)
        if not content:
            return None
        try:
            start = content.find("{")
            end = content.rfind("}")
            if start == -1 or end == -1:
                return None
            return json.loads(content[start : end + 1])
        except Exception:
            return None

    def text_completion(self, system_prompt: str, user_prompt: str) -> str | None:
        return self._chat_completion(system_prompt=system_prompt, user_prompt=user_prompt)

    def _chat_completion(self, system_prompt: str, user_prompt: str) -> str | None:
        if not self.ensure_server():
            return None

        payload = {
            "model": self.config.model.model_name,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt},
            ],
        }
        req = request.Request(
            f"{self.config.model.endpoint}/chat/completions",
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with request.urlopen(req, timeout=30) as response:
                body = json.loads(response.read().decode("utf-8"))
        except (error.URLError, TimeoutError, json.JSONDecodeError):
            return None

        try:
            return str(body["choices"][0]["message"]["content"]).strip()
        except Exception:
            return None
