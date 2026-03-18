from __future__ import annotations

import re

from .llm import LocalLlmClient
from .schemas import Intent


class IntentResolver:
    def __init__(self, llm: LocalLlmClient) -> None:
        self.llm = llm

    def resolve(self, text: str) -> Intent:
        normalized = " ".join(text.strip().split())
        heuristic = self._resolve_heuristic(normalized)
        if heuristic.kind != "unsupported" and heuristic.confidence >= 0.95:
            return heuristic

        llm_result = self._resolve_llm(normalized)
        if llm_result is not None and llm_result.kind != "unsupported":
            return llm_result

        return heuristic

    def _resolve_llm(self, text: str) -> Intent | None:
        payload = self.llm.json_completion(
            system_prompt=(
                "You convert voice commands for a desktop agent into compact JSON. "
                "Return only JSON with keys: kind, app_name, query, target_text, target_app, "
                "recipient_name, recipient_email, recipient_phone, subject, body, account_hint, "
                "response_text, confidence, metadata. "
                "Supported kinds are open_app, browser_search, type_text, send_email, send_message, "
                "play_spotify, workflow, unsupported. "
                "For send_email and send_message, infer the recipient name if the user supplied one. "
                "If the user asks to say something nice, draft a natural message body instead of repeating "
                "the literal phrase 'something nice'. "
                "If the user requests more than one distinct task in one sentence, return kind=workflow and "
                "put an actions array inside metadata. Each action item should include its own kind and fields."
            ),
            user_prompt=text,
        )
        if not payload:
            return None
        kind = str(payload.get("kind", "unsupported"))
        if kind not in {
            "open_app",
            "browser_search",
            "type_text",
            "send_email",
            "send_message",
            "play_spotify",
            "workflow",
            "unsupported",
        }:
            kind = "unsupported"
        return Intent(
            kind=kind,
            original_text=text,
            app_name=payload.get("app_name"),
            query=payload.get("query"),
            target_text=payload.get("target_text"),
            target_app=payload.get("target_app"),
            recipient_name=payload.get("recipient_name"),
            recipient_email=payload.get("recipient_email"),
            recipient_phone=payload.get("recipient_phone"),
            subject=payload.get("subject"),
            body=payload.get("body"),
            account_hint=payload.get("account_hint"),
            response_text=payload.get("response_text"),
            confidence=float(payload.get("confidence") or 0.35),
            used_llm=True,
            metadata=payload.get("metadata") or {},
        )

    def _resolve_heuristic(self, text: str) -> Intent:
        lower = text.lower()

        match = re.search(r"\bopen\s+(?:google\s+)?chrome\b", lower)
        if match:
            return Intent(
                kind="open_app",
                original_text=text,
                app_name="Google Chrome",
                response_text="Opening Google Chrome.",
                confidence=0.99,
            )

        match = re.search(r"\bsearch\s+youtube\s+for\s+(.+)$", lower)
        if match:
            query = text[match.start(1) :].strip()
            return Intent(
                kind="browser_search",
                original_text=text,
                app_name="Google Chrome",
                query=query,
                target_app="YouTube",
                response_text=f"Searching YouTube for {query}.",
                confidence=0.99,
            )

        match = re.search(r"\btype\s+(.+?)\s+in\s+(?:a\s+)?text\s+editor\b", lower)
        if match:
            start, end = match.span(1)
            target_text = text[start:end].strip().strip("\"'")
            return Intent(
                kind="type_text",
                original_text=text,
                app_name="TextEdit",
                target_text=target_text,
                target_app="TextEdit",
                response_text=f"Typing {target_text} in TextEdit.",
                confidence=0.99,
            )

        if any(token in lower for token in {"email", "gmail", "compose"}):
            recipient_name = self._extract_recipient_name(text, lower)
            if not recipient_name:
                return Intent(kind="unsupported", original_text=text, confidence=0.0)
            body = self._extract_message_body(text) or "something nice"
            return Intent(
                kind="send_email",
                original_text=text,
                app_name="Google Chrome",
                target_app="Gmail",
                recipient_name=recipient_name,
                body=body,
                account_hint=self._extract_account_hint(lower),
                response_text=f"Sending an email to {recipient_name}.",
                confidence=0.97,
            )

        if ("message" in lower or "text" in lower):
            recipient_name = self._extract_recipient_name(text, lower)
            if not recipient_name:
                return Intent(kind="unsupported", original_text=text, confidence=0.0)
            body = self._extract_message_body(text) or f"Hey {recipient_name}, hope you're having a great day."
            return Intent(
                kind="send_message",
                original_text=text,
                app_name="Messages",
                target_app="Messages",
                recipient_name=recipient_name,
                body=body,
                response_text=f"Sending a message to {recipient_name}.",
                confidence=0.97,
            )

        match = re.search(r"\b(?:play|place)\s+(.+?)\s+on\s+spotify\b", text, re.IGNORECASE)
        if match:
            query = match.group(1).strip().strip("\"'")
            return Intent(
                kind="play_spotify",
                original_text=text,
                app_name="Spotify",
                target_app="Spotify",
                query=query,
                response_text=f"Playing {query} on Spotify.",
                confidence=0.98,
            )

        return Intent(kind="unsupported", original_text=text, confidence=0.0)

    @staticmethod
    def _extract_recipient_name(text: str, lower: str) -> str | None:
        match = re.search(
            r"\bto\s+([a-zA-Z][a-zA-Z0-9 .'-]+?)(?=\s+(?:saying|about|with|from|using|in|on)\b|$)",
            text,
            re.IGNORECASE,
        )
        if match:
            return match.group(1).strip().strip(",.")
        if "vishay" in lower:
            return "Vishay"
        return None

    @staticmethod
    def _extract_message_body(text: str) -> str | None:
        match = re.search(
            r"\b(?:saying|say|that says|with the message)\s+(.+)$",
            text,
            re.IGNORECASE,
        )
        if not match:
            return None
        body = match.group(1).strip().strip("\"'.")
        body = re.sub(r"\s+from\s+my\s+(?:personal|school(?:ogy)?)\s+g?mail\b.*$", "", body, flags=re.IGNORECASE)
        return body.strip()

    @staticmethod
    def _extract_account_hint(lower: str) -> str | None:
        if "schoology" in lower or "school" in lower:
            return "schoology"
        if "personal" in lower:
            return "personal"
        return None
