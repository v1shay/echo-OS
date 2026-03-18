from __future__ import annotations

from dataclasses import asdict
from pathlib import Path
from typing import Any

from .runtime import ensure_dir, read_json, write_json


class MemoryStore:
    def __init__(self, path: Path) -> None:
        self.path = path
        ensure_dir(path.parent)

    def append_run(self, record: dict[str, Any]) -> None:
        payload = read_json(self.path, {"runs": []})
        payload.setdefault("runs", []).append(record)
        write_json(self.path, payload)

    def append_execution(
        self, command: str, success: bool, message: str, intent_kind: str
    ) -> None:
        self.append_run(
            {
                "command": command,
                "success": success,
                "message": message,
                "intent_kind": intent_kind,
            }
        )
