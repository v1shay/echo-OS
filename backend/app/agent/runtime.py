from __future__ import annotations

import json
from typing import Any, Awaitable, Callable

from langgraph.errors import GraphRecursionError

from app.agent.graph import build_graph
from app.agent.prompts import build_system_prompt, format_contextual_request
from app.agent.state import EchoGraphState
from app.integrations.openai_client import OpenAIPlatformClient
from app.memory.short_term import ShortTermMemory
from app.memory.store import ChromaMemoryStore
from app.tools import build_registry

EventSink = Callable[[dict[str, Any]], Awaitable[None]]


class EchoAgentRuntime:
    def __init__(self, settings):
        self.settings = settings
        self.registry = build_registry(settings)
        self.llm = OpenAIPlatformClient(settings)
        self.short_term = ShortTermMemory(window_size=settings.short_term_memory_window)
        self.long_term = ChromaMemoryStore(settings)
        self._event_sinks: dict[str, EventSink] = {}
        self.graph = build_graph(self)

    def attach_event_sink(self, session_id: str, sink: EventSink) -> None:
        self._event_sinks[session_id] = sink

    def detach_event_sink(self, session_id: str) -> None:
        self._event_sinks.pop(session_id, None)

    async def publish(self, session_id: str, event: dict[str, Any]) -> None:
        sink = self._event_sinks.get(session_id)
        if sink:
            await sink(event)

    async def run_task(self, session_id: str, user_request: str) -> dict[str, Any]:
        if self.settings.enable_ollama_fallback:
            return await self._run_local_task(session_id, user_request)

        initial_state: EchoGraphState = {
            "session_id": session_id,
            "user_request": user_request,
            "execution_log": [],
            "previous_response_id": None,
            "iteration": 0,
            "completed": False,
        }
        try:
            result = await self.graph.ainvoke(
                initial_state,
                config={"recursion_limit": max(10, self.settings.agent_iteration_limit * 3)},
            )
        except GraphRecursionError:
            result = {
                "assistant_response": (
                    "I couldn't complete that request reliably with the current local model. "
                    "Try a shorter phrasing or a more specific instruction."
                ),
                "execution_log": [],
                "verification": {
                    "completed": False,
                    "confidence": 0.0,
                    "reasoning": "The agent hit its recursion limit before finishing the task.",
                    "next_action": "",
                },
                "recalled_memories": [],
            }
        return {
            "session_id": session_id,
            "response": result.get("assistant_response", ""),
            "execution_log": result.get("execution_log", []),
            "verification": result.get("verification", {}),
            "memories": result.get("recalled_memories", []),
        }

    async def _run_local_task(self, session_id: str, user_request: str) -> dict[str, Any]:
        state: EchoGraphState = {
            "session_id": session_id,
            "user_request": user_request,
            "execution_log": [],
            "previous_response_id": None,
            "iteration": 0,
            "completed": False,
        }
        state.update(await self.prepare_context(state))

        assistant_response = ""
        verification: dict[str, Any] = {
            "completed": False,
            "confidence": 0.0,
            "reasoning": "The task did not finish.",
            "next_action": "",
        }
        seen_call_signatures: set[str] = set()
        had_successful_actions = False

        for step in range(self.settings.agent_iteration_limit):
            reason_update = await self.reason(state)
            state.update(reason_update)
            assistant_response = state.get("assistant_response", "") or assistant_response
            tool_calls = state.get("tool_calls", [])

            if not tool_calls:
                verification = {
                    "completed": True,
                    "confidence": 0.8,
                    "reasoning": (
                        "The local planner produced a direct response after completing the available steps."
                        if had_successful_actions
                        else "The local planner produced a direct response without requiring more tools."
                    ),
                    "next_action": "",
                }
                break

            call_signature = json.dumps(
                [(call.name, call.arguments) for call in tool_calls],
                sort_keys=True,
                ensure_ascii=True,
            )
            if call_signature in seen_call_signatures:
                verification = {
                    "completed": had_successful_actions,
                    "confidence": 0.4 if had_successful_actions else 0.0,
                    "reasoning": (
                        "The planner started repeating the same tool actions, so execution was stopped."
                    ),
                    "next_action": "",
                }
                if not assistant_response:
                    assistant_response = (
                        "I stopped because the local planner kept repeating the same action."
                    )
                break
            seen_call_signatures.add(call_signature)

            outputs = await self.registry.execute_calls(session_id, tool_calls)
            state["execution_log"] = state.get("execution_log", []) + [
                {
                    "phase": "execute_tools",
                    "outputs": outputs,
                }
            ]
            await self.publish(session_id, {"type": "tool_outputs", "outputs": outputs})

            parsed_outputs: list[dict[str, Any]] = []
            for output in outputs:
                parsed = json.loads(output.get("output", "{}"))
                parsed_outputs.append(parsed)

            failures = [item for item in parsed_outputs if not item.get("success", False)]
            if failures:
                failure_text = " | ".join(
                    f"{item.get('tool_name')}: {item.get('error')}" for item in failures
                )
                if step < self.settings.agent_iteration_limit - 1:
                    state["pending_inputs"] = outputs + [
                        {
                            "role": "user",
                            "content": [
                                {
                                    "type": "input_text",
                                    "text": (
                                        "The previous tool call failed. Choose a corrected tool or corrected arguments.\n"
                                        f"Failure details: {failure_text}"
                                    ),
                                }
                            ],
                        }
                    ]
                    continue
                assistant_response = f"I couldn't complete the task because: {failure_text}"
                verification = {
                    "completed": False,
                    "confidence": 0.2,
                    "reasoning": assistant_response,
                    "next_action": "",
                }
                break

            if parsed_outputs:
                had_successful_actions = True
            state["pending_inputs"] = outputs

        else:
            assistant_response = (
                "I couldn't complete that request reliably with the current local model. "
                "Try a shorter phrasing or a more specific instruction."
            )
            verification = {
                "completed": False,
                "confidence": 0.0,
                "reasoning": "The local planner exceeded its step limit.",
                "next_action": "",
            }

        self.short_term.append(session_id, "user", user_request)
        self.short_term.append(session_id, "assistant", assistant_response)

        return {
            "session_id": session_id,
            "response": assistant_response,
            "execution_log": state.get("execution_log", []),
            "verification": verification,
            "memories": [],
        }

    async def prepare_context(self, state: EchoGraphState) -> EchoGraphState:
        session_id = state["session_id"]
        await self.publish(session_id, {"type": "status", "state": "thinking"})
        memories = self.long_term.recall(state["user_request"], session_id=session_id)
        recalled_text = [item["text"] for item in memories]
        history = [(turn.role, turn.content) for turn in self.short_term.recent(session_id)]
        contextual_request = format_contextual_request(
            user_request=state["user_request"],
            short_term_history=history,
            recalled_memories=recalled_text,
        )
        return {
            "recalled_memories": recalled_text,
            "pending_inputs": [
                {
                    "role": "system",
                    "content": [{"type": "input_text", "text": build_system_prompt(str(self.settings.workspace_root))}],
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": contextual_request}],
                },
            ],
        }

    async def reason(self, state: EchoGraphState) -> EchoGraphState:
        session_id = state["session_id"]
        pending_inputs = state.get("pending_inputs", [])
        response = await self.llm.create_agent_response(
            input_items=pending_inputs,
            tools=self.registry.schemas_for_model(),
            previous_response_id=state.get("previous_response_id"),
        )
        tool_calls = self.llm.extract_tool_calls(response)
        assistant_text = response.get("output_text") or self.llm.extract_text(response)
        if assistant_text:
            await self.publish(session_id, {"type": "assistant_text", "text": assistant_text})
        if tool_calls:
            await self.publish(
                session_id,
                {
                    "type": "status",
                    "state": "executing",
                    "tools": [call.name for call in tool_calls],
                },
            )
        return {
            "previous_response_id": response["id"],
            "tool_calls": tool_calls,
            "assistant_response": assistant_text,
            "pending_inputs": [],
            "iteration": state.get("iteration", 0) + 1,
            "execution_log": state.get("execution_log", []) + [
                {
                    "phase": "reason",
                    "assistant_response": assistant_text,
                    "tool_calls": [call.name for call in tool_calls],
                }
            ],
        }

    async def execute_tools(self, state: EchoGraphState) -> EchoGraphState:
        tool_calls = state.get("tool_calls", [])
        outputs = await self.registry.execute_calls(state["session_id"], tool_calls)
        execution_log = state.get("execution_log", []) + [
            {
                "phase": "execute_tools",
                "outputs": outputs,
            }
        ]
        await self.publish(state["session_id"], {"type": "tool_outputs", "outputs": outputs})
        return {
            "pending_inputs": outputs,
            "execution_log": execution_log,
        }

    async def verify(self, state: EchoGraphState) -> EchoGraphState:
        if self.settings.enable_ollama_fallback:
            execution_log = state.get("execution_log", [])
            recent_reason = next(
                (entry for entry in reversed(execution_log) if entry.get("phase") == "reason"),
                {},
            )
            tool_calls = recent_reason.get("tool_calls", [])
            recent_execute = next(
                (entry for entry in reversed(execution_log) if entry.get("phase") == "execute_tools"),
                {},
            )
            outputs = recent_execute.get("outputs", [])
            failures: list[str] = []
            for output in outputs:
                raw = output.get("output", "")
                try:
                    parsed = __import__("json").loads(raw)
                except Exception:
                    continue
                if not parsed.get("success", False):
                    failures.append(f"{parsed.get('tool_name')}: {parsed.get('error')}")

            if failures:
                if state.get("iteration", 0) >= self.settings.agent_iteration_limit:
                    summary = (
                        "I couldn't complete the task because the available tools kept failing with: "
                        + " | ".join(failures)
                    )
                    return {
                        "assistant_response": summary,
                        "verification": {
                            "completed": True,
                            "confidence": 0.2,
                            "reasoning": summary,
                            "next_action": "",
                        },
                        "completed": True,
                        "pending_inputs": [],
                        "execution_log": execution_log
                        + [
                            {
                                "phase": "verify",
                                "verification": {
                                    "completed": True,
                                    "confidence": 0.2,
                                    "reasoning": summary,
                                    "next_action": "",
                                },
                            }
                        ],
                    }
                feedback = (
                    "The previous tool attempt failed. Choose better tools or corrected arguments and continue.\n"
                    f"Failures: {' | '.join(failures)}"
                )
                return {
                    "verification": {
                        "completed": False,
                        "confidence": 0.2,
                        "reasoning": feedback,
                        "next_action": feedback,
                    },
                    "completed": False,
                    "pending_inputs": [
                        {
                            "role": "user",
                            "content": [{"type": "input_text", "text": feedback}],
                        }
                    ],
                    "execution_log": execution_log
                    + [
                        {
                            "phase": "verify",
                            "verification": {
                                "completed": False,
                                "confidence": 0.2,
                                "reasoning": feedback,
                                "next_action": feedback,
                            },
                        }
                    ],
                }

            assistant_response = (state.get("assistant_response") or "").strip()
            if not tool_calls and assistant_response:
                lowered_request = state["user_request"].lower()
                action_markers = [
                    "open",
                    "send",
                    "text",
                    "email",
                    "play",
                    "book",
                    "organize",
                    "fix",
                    "click",
                    "type",
                    "search",
                ]
                refusal_markers = [
                    "can't assist",
                    "cannot assist",
                    "i'm sorry",
                    "unable to",
                ]
                if any(marker in lowered_request for marker in action_markers) and any(
                    marker in assistant_response.lower() for marker in refusal_markers
                ):
                    if state.get("iteration", 0) >= self.settings.agent_iteration_limit:
                        summary = assistant_response
                        return {
                            "verification": {
                                "completed": True,
                                "confidence": 0.2,
                                "reasoning": summary,
                                "next_action": "",
                            },
                            "completed": True,
                            "pending_inputs": [],
                            "execution_log": execution_log
                            + [
                                {
                                    "phase": "verify",
                                    "verification": {
                                        "completed": True,
                                        "confidence": 0.2,
                                        "reasoning": summary,
                                        "next_action": "",
                                    },
                                }
                            ],
                        }
                    feedback = (
                        "The user asked you to perform a task on the computer. "
                        "Do not refuse when tools could help. If a required detail is missing, ask one short clarification."
                    )
                    return {
                        "verification": {
                            "completed": False,
                            "confidence": 0.2,
                            "reasoning": feedback,
                            "next_action": feedback,
                        },
                        "completed": False,
                        "pending_inputs": [
                            {
                                "role": "user",
                                "content": [{"type": "input_text", "text": feedback}],
                            }
                        ],
                        "execution_log": execution_log
                        + [
                            {
                                "phase": "verify",
                                "verification": {
                                    "completed": False,
                                    "confidence": 0.2,
                                    "reasoning": feedback,
                                    "next_action": feedback,
                                },
                            }
                        ],
                    }

            if not tool_calls:
                return {
                    "verification": {
                        "completed": True,
                        "confidence": 0.8,
                        "reasoning": "Local mode accepted the assistant response without an extra verifier pass.",
                        "next_action": "",
                    },
                    "completed": True,
                    "pending_inputs": [],
                    "execution_log": execution_log
                    + [
                        {
                            "phase": "verify",
                            "verification": {
                                "completed": True,
                                "confidence": 0.8,
                                "reasoning": "Local mode accepted the assistant response without an extra verifier pass.",
                                "next_action": "",
                            },
                        }
                    ],
                }
        verification = await self.llm.verify_completion(
            task=state["user_request"],
            assistant_response=state.get("assistant_response", ""),
            execution_log=state.get("execution_log", []),
        )
        completed = bool(verification.get("completed")) or state.get("iteration", 0) >= self.settings.agent_iteration_limit
        if not completed:
            feedback = verification.get("next_action") or "The task is not complete yet. Continue until it is completed or clearly blocked."
            pending_inputs = [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": feedback}],
                }
            ]
        else:
            pending_inputs = []
        return {
            "verification": verification,
            "completed": completed,
            "pending_inputs": pending_inputs,
            "execution_log": state.get("execution_log", []) + [{"phase": "verify", "verification": verification}],
        }

    async def store_outcome(self, state: EchoGraphState) -> EchoGraphState:
        session_id = state["session_id"]
        user_request = state["user_request"]
        assistant_response = state.get("assistant_response", "")

        self.short_term.append(session_id, "user", user_request)
        self.short_term.append(session_id, "assistant", assistant_response)

        summary = f"User asked: {user_request}\nOutcome: {assistant_response}"
        self.long_term.remember(summary, {"session_id": session_id, "category": "task"})

        for item in await self.llm.extract_memories(
            user_request=user_request,
            assistant_response=assistant_response,
        ):
            self.long_term.remember(
                item["text"],
                {"session_id": session_id, "category": item.get("category", "memory")},
            )

        await self.publish(session_id, {"type": "status", "state": "idle"})
        return state

    def route_after_reason(self, state: EchoGraphState) -> str:
        return "execute_tools" if state.get("tool_calls") else "verify"

    def route_after_verify(self, state: EchoGraphState) -> str:
        return "store_outcome" if state.get("completed") else "reason"
