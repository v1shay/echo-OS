from __future__ import annotations

from typing import Any, TypedDict


class EchoGraphState(TypedDict, total=False):
    session_id: str
    user_request: str
    contextual_input: list[dict[str, Any]]
    recalled_memories: list[str]
    previous_response_id: str | None
    tool_calls: list[Any]
    pending_inputs: list[dict[str, Any]]
    execution_log: list[dict[str, Any]]
    assistant_response: str
    verification: dict[str, Any]
    completed: bool
    iteration: int
