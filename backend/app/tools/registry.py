from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass
from typing import Any

from app.config import Settings
from app.tools.base import ToolContext, ToolDefinition, ToolExecutionResult


@dataclass(slots=True)
class ToolCall:
    call_id: str
    name: str
    arguments: dict[str, Any]


class ToolRegistry:
    def __init__(self, settings: Settings):
        self.settings = settings
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        self._tools[tool.name] = tool

    def get(self, name: str) -> ToolDefinition:
        if name not in self._tools:
            raise KeyError(f"Tool '{name}' is not registered.")
        return self._tools[name]

    def schemas_for_model(self) -> list[dict[str, Any]]:
        return [tool.schema() for tool in self._tools.values()]

    async def execute_calls(
        self,
        session_id: str,
        calls: list[ToolCall],
    ) -> list[dict[str, Any]]:
        context = ToolContext(settings=self.settings, session_id=session_id)

        tool_defs = [self.get(call.name) for call in calls]
        parallel = len(calls) > 1 and all(tool.supports_parallel for tool in tool_defs)

        if parallel:
            results = await asyncio.gather(
                *[
                    self._execute_single(context=context, call=call)
                    for call in calls
                ]
            )
        else:
            results = []
            for call in calls:
                results.append(await self._execute_single(context=context, call=call))

        return results

    async def _execute_single(self, context: ToolContext, call: ToolCall) -> dict[str, Any]:
        tool = self.get(call.name)
        if (
            tool.requires_confirmation
            and self.settings.require_confirmation_for_side_effects
            and not self.settings.enable_ollama_fallback
        ):
            blocked = ToolExecutionResult(
                tool_name=call.name,
                success=False,
                error="Execution requires explicit confirmation before side effects.",
                duration_ms=0,
                requires_confirmation=True,
            )
            return self._format_tool_output(call.call_id, blocked)

        result = await tool.execute(context, call.arguments)
        return self._format_tool_output(call.call_id, result)

    def _format_tool_output(self, call_id: str, result: ToolExecutionResult) -> dict[str, Any]:
        return {
            "type": "function_call_output",
            "call_id": call_id,
            "output": json.dumps(result.model_dump(), ensure_ascii=True),
        }
