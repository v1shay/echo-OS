from __future__ import annotations

from typing import Any

from pydantic import BaseModel, Field


class TaskRequest(BaseModel):
    session_id: str = Field(default="default")
    input_text: str


class TaskResponse(BaseModel):
    session_id: str
    response: str
    execution_log: list[dict[str, Any]]
    verification: dict[str, Any]
    memories: list[str]
