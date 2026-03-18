from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal


IntentKind = Literal[
    "open_app",
    "browser_search",
    "type_text",
    "send_email",
    "send_message",
    "play_spotify",
    "workflow",
    "unsupported",
]
StepKind = Literal[
    "open_app",
    "hide_app",
    "textedit_new_document",
    "textedit_set_text",
    "browser_attach",
    "browser_open",
    "browser_assert",
    "press_hotkey",
    "type_text",
    "chrome_open_url",
    "gmail_send_email",
    "messages_send",
    "spotify_play",
]


@dataclass(slots=True)
class OcrBox:
    text: str
    confidence: float
    x: float
    y: float
    width: float
    height: float

    @property
    def center(self) -> tuple[int, int]:
        return (
            int(round(self.x + (self.width / 2))),
            int(round(self.y + (self.height / 2))),
        )


@dataclass(slots=True)
class Observation:
    screenshot_path: Path
    frontmost_app: str | None
    window_title: str | None
    screen_text: str
    ocr_boxes: list[OcrBox] = field(default_factory=list)
    browser_state: dict[str, Any] | None = None


@dataclass(slots=True)
class Intent:
    kind: IntentKind
    original_text: str
    app_name: str | None = None
    query: str | None = None
    target_text: str | None = None
    target_app: str | None = None
    recipient_name: str | None = None
    recipient_email: str | None = None
    recipient_phone: str | None = None
    subject: str | None = None
    body: str | None = None
    account_hint: str | None = None
    response_text: str | None = None
    confidence: float = 0.0
    used_llm: bool = False
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class PlanStep:
    kind: StepKind
    description: str
    params: dict[str, Any] = field(default_factory=dict)
    verification: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class Plan:
    goal: str
    success_message: str
    steps: list[PlanStep]


@dataclass(slots=True)
class StepRecord:
    description: str
    success: bool
    summary: str
    before_screenshot: str | None = None
    after_screenshot: str | None = None
    verification: dict[str, Any] = field(default_factory=dict)
    details: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class ExecutionResult:
    command: str
    success: bool
    message: str
    intent: Intent
    plan: Plan
    steps: list[StepRecord]
    artifacts_dir: Path
