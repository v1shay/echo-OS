from __future__ import annotations

import json
import uuid
from typing import Any

import httpx
from openai import AsyncOpenAI

from app.config import Settings
from app.tools.registry import ToolCall


class OpenAIPlatformClient:
    def __init__(self, settings: Settings):
        if not settings.openai_api_key:
            raise RuntimeError("OPENAI_API_KEY or CHATGPT_KEY must be configured.")
        self.settings = settings
        self.client = AsyncOpenAI(
            api_key=settings.openai_api_key.get_secret_value(),
            max_retries=0,
            timeout=20,
        )

    async def create_agent_response(
        self,
        *,
        input_items: list[dict[str, Any]],
        tools: list[dict[str, Any]],
        previous_response_id: str | None = None,
    ) -> dict[str, Any]:
        if self.settings.enable_ollama_fallback:
            return await self._create_ollama_response(input_items=input_items, tools=tools)
        try:
            response = await self.client.responses.create(
                model=self.settings.openai_model,
                input=input_items,
                tools=tools,
                parallel_tool_calls=True,
                previous_response_id=previous_response_id,
            )
            payload = response.model_dump()
            payload["output_text"] = getattr(response, "output_text", "") or self.extract_text(payload)
            return payload
        except Exception:
            if not self.settings.enable_ollama_fallback:
                raise
            return await self._create_ollama_response(input_items=input_items, tools=tools)

    async def verify_completion(
        self,
        *,
        task: str,
        assistant_response: str,
        execution_log: list[dict[str, Any]],
    ) -> dict[str, Any]:
        prompt = f"""
        You are a strict task verifier for an agentic operating system assistant.
        Decide whether the task was fully completed end-to-end.

        Task:
        {task}

        Assistant response:
        {assistant_response}

        Execution log:
        {json.dumps(execution_log[-8:], ensure_ascii=True)}

        Return only valid JSON with keys:
        completed (boolean),
        confidence (number),
        reasoning (string),
        next_action (string, empty if none).
        """
        if self.settings.enable_ollama_fallback:
            raw = await self._chat_with_ollama(
                system_prompt=(
                    "You verify whether an assistant completed a task. "
                    "Return only JSON with keys completed, confidence, reasoning, next_action."
                ),
                user_prompt=prompt,
                json_mode=True,
            )
            return _parse_json(
                raw,
                fallback={"completed": True, "confidence": 0.5, "reasoning": raw, "next_action": ""},
            )
        try:
            response = await self.client.responses.create(
                model=self.settings.verifier_model,
                input=prompt,
            )
            raw = getattr(response, "output_text", "") or ""
            return _parse_json(raw, fallback={"completed": True, "confidence": 0.5, "reasoning": raw, "next_action": ""})
        except Exception:
            if not self.settings.enable_ollama_fallback:
                raise
            raw = await self._chat_with_ollama(
                system_prompt=(
                    "You verify whether an assistant completed a task. "
                    "Return only JSON with keys completed, confidence, reasoning, next_action."
                ),
                user_prompt=prompt,
                json_mode=True,
            )
            return _parse_json(
                raw,
                fallback={"completed": True, "confidence": 0.5, "reasoning": raw, "next_action": ""},
            )

    async def extract_memories(self, *, user_request: str, assistant_response: str) -> list[dict[str, str]]:
        prompt = f"""
        Extract durable long-term memories from this interaction.
        Only include facts that are likely useful in future tasks, such as preferences,
        recurring contacts, project names, or stable constraints.

        User request:
        {user_request}

        Assistant response:
        {assistant_response}

        Return only JSON as an array of objects with keys:
        text (string) and category (string).
        """
        if self.settings.enable_ollama_fallback:
            return []
        try:
            response = await self.client.responses.create(
                model=self.settings.verifier_model,
                input=prompt,
            )
            raw = getattr(response, "output_text", "") or "[]"
            parsed = _parse_json(raw, fallback=[])
            return parsed if isinstance(parsed, list) else []
        except Exception:
            if not self.settings.enable_ollama_fallback:
                raise
            return []

    def extract_tool_calls(self, response: dict[str, Any]) -> list[ToolCall]:
        calls: list[ToolCall] = []
        for item in response.get("output", []):
            if item.get("type") != "function_call":
                continue
            arguments = item.get("arguments", "{}")
            calls.append(
                ToolCall(
                    call_id=item["call_id"],
                    name=item["name"],
                    arguments=json.loads(arguments) if isinstance(arguments, str) else arguments,
                )
            )
        return calls

    def extract_text(self, response: dict[str, Any]) -> str:
        chunks: list[str] = []
        for item in response.get("output", []):
            if item.get("type") != "message":
                continue
            for content in item.get("content", []):
                if content.get("type") in {"output_text", "text"}:
                    chunks.append(content.get("text", ""))
        return "\n".join(chunk for chunk in chunks if chunk).strip()

    async def _create_ollama_response(
        self,
        *,
        input_items: list[dict[str, Any]],
        tools: list[dict[str, Any]],
    ) -> dict[str, Any]:
        tool_results = self._extract_tool_results(input_items)
        success_results = [result for result in tool_results if result.get("success") is True]
        failure_results = [result for result in tool_results if result.get("success") is False]
        tool_summaries = [
            {
                "name": tool.get("name"),
                "description": tool.get("description"),
                "parameters": tool.get("parameters", {}),
            }
            for tool in tools
        ]
        user_prompt = json.dumps(
            {
                "conversation": self._normalize_input_items(input_items),
                "available_tools": tool_summaries,
                "latest_tool_results": tool_results,
            },
            ensure_ascii=True,
        )
        tool_result_guidance = ""
        if success_results and not failure_results:
            tool_result_guidance = (
                "You already have successful tool results. Do not call more tools unless an obvious additional step is still required. "
                "Usually you should now return a final assistant_response with tool_calls as an empty array."
            )
        elif failure_results:
            tool_result_guidance = (
                "Some tool calls failed. Use the failure details to choose corrected arguments or a better tool. "
                "Do not repeat the exact same failed call unless you are changing the arguments."
            )
        raw = await self._chat_with_ollama(
            system_prompt=(
                "You are Echo OS's local fallback reasoning engine. "
                "You are controlling a real computer and you must act when the user asks for an action. "
                "If the user asks to open apps, open folders, search the web, use the browser, use the keyboard or mouse, "
                "send a message, send email, inspect files, or run commands, you should choose tool_calls instead of refusing. "
                "Only say you cannot do something when credentials are missing, the requested information is missing, or the task is unsafe. "
                "When a task can be partially completed, do the doable steps first. "
                "If a communication task is missing a required recipient or destination, ask one short clarifying question instead of refusing. "
                "If a provider credential is missing, say exactly which credential or account is missing. Do not try to configure secrets yourself via terminal commands. "
                "Use app_control_tool for opening applications, folders, and URLs. "
                "Use desktop_tool for screenshots, keyboard, mouse, and clipboard. "
                "Use macos_automation_tool when AppleScript or general macOS app automation is the best path. "
                "Use macos_ui_tool for generic computer control in the frontmost app: inspect UI, click named controls, focus fields, and drive menus. "
                "Use browser_tool for web navigation and interactive websites. "
                "Return only valid JSON with keys assistant_response and tool_calls. "
                "tool_calls must be an array of objects with keys name and arguments. "
                "Only choose tools from available_tools. If no tool is needed, return an empty array. "
                "Example 1: {\"assistant_response\":\"Opening Spotify.\",\"tool_calls\":[{\"name\":\"app_control_tool\",\"arguments\":{\"operation\":\"open_application\",\"application\":\"Spotify\"}}]} "
                "Example 2: {\"assistant_response\":\"Opening your Downloads folder.\",\"tool_calls\":[{\"name\":\"app_control_tool\",\"arguments\":{\"operation\":\"open_path\",\"path\":\"~/Downloads\"}}]} "
                "Example 3: {\"assistant_response\":\"Who should I send that message to?\",\"tool_calls\":[]} "
                "Example 4: {\"assistant_response\":\"Starting Spotify playback.\",\"tool_calls\":[{\"name\":\"macos_automation_tool\",\"arguments\":{\"operation\":\"run_applescript\",\"script\":\"tell application \\\"Spotify\\\" to play\"}}]} "
                "Example 5: {\"assistant_response\":\"Inspecting the current app to continue the task.\",\"tool_calls\":[{\"name\":\"macos_ui_tool\",\"arguments\":{\"operation\":\"inspect_front_window\"}}]}"
                f" {tool_result_guidance}"
            ),
            user_prompt=user_prompt,
            json_mode=True,
        )
        parsed = _parse_json(raw, fallback={"assistant_response": raw, "tool_calls": []})
        assistant_response = parsed.get("assistant_response", "") if isinstance(parsed, dict) else raw
        tool_calls_raw = parsed.get("tool_calls", []) if isinstance(parsed, dict) else []

        output: list[dict[str, Any]] = []
        for tool_call in tool_calls_raw:
            name = tool_call.get("name")
            arguments = tool_call.get("arguments", {})
            if not name or not isinstance(arguments, dict):
                continue
            output.append(
                {
                    "type": "function_call",
                    "call_id": f"ollama_{uuid.uuid4().hex}",
                    "name": name,
                    "arguments": json.dumps(arguments, ensure_ascii=True),
                }
            )
        if assistant_response:
            output.insert(
                0,
                {
                    "type": "message",
                    "content": [{"type": "output_text", "text": assistant_response}],
                },
            )
        return {
            "id": f"ollama_{uuid.uuid4().hex}",
            "output_text": assistant_response,
            "output": output,
        }

    async def _chat_with_ollama(self, *, system_prompt: str, user_prompt: str, json_mode: bool) -> str:
        payload: dict[str, Any] = {
            "model": self.settings.ollama_model,
            "stream": False,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt},
            ],
            "options": {"temperature": 0.1, "num_predict": 256},
        }
        if json_mode:
            payload["format"] = "json"
        async with httpx.AsyncClient(timeout=180) as client:
            response = await client.post(
                f"{self.settings.ollama_base_url.rstrip('/')}/api/chat",
                json=payload,
            )
            response.raise_for_status()
            data = response.json()
        return data.get("message", {}).get("content", "")

    def _normalize_input_items(self, input_items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        normalized: list[dict[str, Any]] = []
        for item in input_items:
            item_type = item.get("type")
            if item_type == "function_call_output":
                normalized.append(
                    {
                        "role": "tool",
                        "call_id": item.get("call_id"),
                        "content": item.get("output", ""),
                    }
                )
                continue

            role = item.get("role", "user")
            content: list[str] = []
            for chunk in item.get("content", []):
                if chunk.get("type") in {"input_text", "output_text", "text"}:
                    content.append(chunk.get("text", ""))
            normalized.append({"role": role, "content": "\n".join(part for part in content if part)})
        return normalized

    def _extract_tool_results(self, input_items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        results: list[dict[str, Any]] = []
        for item in input_items:
            if item.get("type") != "function_call_output":
                continue
            raw = item.get("output", "")
            if not isinstance(raw, str):
                continue
            parsed = _parse_json(raw, fallback=None)
            if isinstance(parsed, dict):
                results.append(parsed)
        return results


def _parse_json(raw: str, fallback: Any) -> Any:
    cleaned = raw.strip()
    if cleaned.startswith("```"):
        cleaned = cleaned.strip("`")
        if cleaned.startswith("json"):
            cleaned = cleaned[4:].strip()
    try:
        return json.loads(cleaned)
    except json.JSONDecodeError:
        return fallback
