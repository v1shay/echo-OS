from __future__ import annotations

from collections import defaultdict
from typing import DefaultDict

from fastapi import WebSocket


class SessionBroadcaster:
    def __init__(self):
        self._sockets: DefaultDict[str, set[WebSocket]] = defaultdict(set)

    async def connect(self, session_id: str, websocket: WebSocket) -> None:
        await websocket.accept()
        self._sockets[session_id].add(websocket)

    def disconnect(self, session_id: str, websocket: WebSocket) -> None:
        self._sockets[session_id].discard(websocket)
        if not self._sockets[session_id]:
            self._sockets.pop(session_id, None)

    async def publish(self, session_id: str, event: dict) -> None:
        stale: list[WebSocket] = []
        for socket in list(self._sockets.get(session_id, set())):
            try:
                await socket.send_json(event)
            except Exception:
                stale.append(socket)
        for socket in stale:
            self.disconnect(session_id, socket)
