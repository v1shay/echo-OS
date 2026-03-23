from __future__ import annotations

from collections import defaultdict, deque
from dataclasses import dataclass
from typing import DefaultDict


@dataclass(slots=True)
class ConversationTurn:
    role: str
    content: str


class ShortTermMemory:
    def __init__(self, window_size: int = 10):
        self.window_size = window_size
        self._sessions: DefaultDict[str, deque[ConversationTurn]] = defaultdict(
            lambda: deque(maxlen=self.window_size)
        )

    def append(self, session_id: str, role: str, content: str) -> None:
        self._sessions[session_id].append(ConversationTurn(role=role, content=content))

    def recent(self, session_id: str) -> list[ConversationTurn]:
        return list(self._sessions[session_id])
