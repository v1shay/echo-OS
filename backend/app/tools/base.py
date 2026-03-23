from __future__ import annotations

import asyncio
import inspect
import time
from dataclasses import dataclass
from typing import Any, Awaitable, Callable, Optional, Type

from pydantic import BaseModel, ConfigDict

from app.config import Settings


class ToolArgs(BaseModel):
    model_config = ConfigDict(extra="forbid")


@dataclass(slots=True)
class ToolContext:
    settings: Settings
    session_id: str


class ToolExecutionResult(BaseModel):
    tool_name: str
    success: bool
    output: Any = None
    error: Optional[str] = None
    duration_ms: int
    requires_confirmation: bool = False


ToolHandler = Callable[[ToolContext, ToolArgs], Awaitable[Any] | Any]


@dataclass(slots=True)
class ToolDefinition:
    name: str
    description: str
    args_model: Type[ToolArgs]
    handler: ToolHandler
    supports_parallel: bool = True
    side_effecting: bool = False
    requires_confirmation: bool = False

    def schema(self) -> dict[str, Any]:
        schema = self.args_model.model_json_schema()
        return {
            "type": "function",
            "name": self.name,
            "description": self.description,
            "parameters": schema,
        }

    async def execute(self, context: ToolContext, raw_arguments: dict[str, Any]) -> ToolExecutionResult:
        started = time.perf_counter()
        try:
            validated = self.args_model.model_validate(raw_arguments)
            outcome = self.handler(context, validated)
            if inspect.isawaitable(outcome):
                outcome = await outcome
            elif asyncio.iscoroutine(outcome):
                outcome = await outcome
            duration_ms = int((time.perf_counter() - started) * 1000)
            return ToolExecutionResult(
                tool_name=self.name,
                success=True,
                output=outcome,
                duration_ms=duration_ms,
                requires_confirmation=self.requires_confirmation,
            )
        except Exception as exc:  # pragma: no cover - agent tools should fail soft
            duration_ms = int((time.perf_counter() - started) * 1000)
            return ToolExecutionResult(
                tool_name=self.name,
                success=False,
                error=str(exc),
                duration_ms=duration_ms,
                requires_confirmation=self.requires_confirmation,
            )
