from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path
from urllib import error, request

from .config import JarvisConfig


class BrowserSidecarClient:
    def __init__(self, config: JarvisConfig) -> None:
        self.config = config
        self._process: subprocess.Popen[str] | None = None

    @property
    def health_url(self) -> str:
        return f"{self.config.browser.sidecar_endpoint}/health"

    def ensure_running(self, timeout_seconds: float = 20.0) -> None:
        if self._healthy():
            return
        env = {
            **dict(__import__("os").environ),
            "JARVIS_BROWSER_HOST": self.config.browser.sidecar_host,
            "JARVIS_BROWSER_PORT": str(self.config.browser.sidecar_port),
            "JARVIS_CHROME_EXECUTABLE": str(self.config.browser.chrome_executable),
            "JARVIS_CHROME_DEBUG_PORT": str(self.config.browser.chrome_debug_port),
            "JARVIS_CHROME_ATTACH_URL": self.config.browser.chrome_attach_url,
        }
        log_path = self.config.paths.artifacts_dir / "browser-sidecar.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        log_file = log_path.open("a", encoding="utf-8")
        self._process = subprocess.Popen(
            [str(self.config.browser.node_binary), str(self.config.browser.sidecar_entry)],
            cwd=str(self.config.browser.sidecar_dir),
            env=env,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        deadline = time.time() + timeout_seconds
        while time.time() < deadline:
            if self._healthy():
                return
            time.sleep(0.5)
        raise RuntimeError("Browser sidecar did not become healthy.")

    def _healthy(self) -> bool:
        try:
            with request.urlopen(self.health_url, timeout=2) as response:
                return response.status == 200
        except Exception:
            return False

    def post(self, route: str, payload: dict | None = None) -> dict:
        self.ensure_running()
        req = request.Request(
            f"{self.config.browser.sidecar_endpoint}{route}",
            data=json.dumps(payload or {}).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with request.urlopen(req, timeout=45) as response:
                return json.loads(response.read().decode("utf-8"))
        except error.HTTPError as exc:
            if exc.code >= 500 and route != "/browser/reset":
                reset_req = request.Request(
                    f"{self.config.browser.sidecar_endpoint}/browser/reset",
                    data=b"{}",
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
                with request.urlopen(reset_req, timeout=45):
                    pass
                with request.urlopen(req, timeout=45) as response:
                    return json.loads(response.read().decode("utf-8"))
            raise
