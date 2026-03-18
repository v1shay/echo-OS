from __future__ import annotations

from urllib.parse import quote_plus

from .schemas import Intent, Plan, PlanStep


class Planner:
    def create_plan(self, intent: Intent) -> Plan:
        if intent.kind == "workflow":
            actions = intent.metadata.get("actions") or []
            steps: list[PlanStep] = []
            messages: list[str] = []
            for index, action in enumerate(actions, start=1):
                sub_intent = self._intent_from_action(action, intent.original_text)
                sub_plan = self.create_plan(sub_intent)
                for step in sub_plan.steps:
                    if not step.description.lower().startswith(f"task {index}:"):
                        step.description = f"Task {index}: {step.description}"
                steps.extend(sub_plan.steps)
                if sub_plan.success_message:
                    messages.append(sub_plan.success_message)
            return Plan(
                goal=intent.original_text,
                success_message=" ".join(messages) if messages else "Completed the requested workflow.",
                steps=steps,
            )

        if intent.kind == "open_app":
            return Plan(
                goal=intent.original_text,
                success_message=f"{intent.app_name} is open and active.",
                steps=[
                    PlanStep(
                        kind="open_app",
                        description=f"Open {intent.app_name}",
                        params={"app_name": intent.app_name},
                        verification={"frontmost_app": intent.app_name},
                    )
                ],
            )

        if intent.kind == "browser_search":
            url = f"https://www.youtube.com/results?search_query={quote_plus(intent.query or '')}"
            return Plan(
                goal=intent.original_text,
                success_message=f"YouTube results for {intent.query} are visible.",
                steps=[
                    PlanStep(
                        kind="browser_attach",
                        description="Ensure browser session is ready",
                        verification={"browser_ready": True},
                    ),
                    PlanStep(
                        kind="browser_open",
                        description=f"Open YouTube results for {intent.query}",
                        params={"url": url},
                        verification={
                            "url_contains": "youtube.com/results",
                            "title_contains": "youtube",
                            "text_contains": intent.query,
                        },
                    ),
                ],
            )

        if intent.kind == "type_text":
            return Plan(
                goal=intent.original_text,
                success_message=f"Typed {intent.target_text} in TextEdit.",
                steps=[
                    PlanStep(
                        kind="hide_app",
                        description="Hide Google Chrome to prevent focus steal",
                        params={"app_name": "Google Chrome"},
                        verification={},
                    ),
                    PlanStep(
                        kind="textedit_new_document",
                        description="Create a new document",
                        params={},
                        verification={},
                    ),
                    PlanStep(
                        kind="textedit_set_text",
                        description=f"Set TextEdit document text to {intent.target_text}",
                        params={"text": intent.target_text or ""},
                        verification={
                            "textedit_document_contains": intent.target_text or "",
                        },
                    ),
                ],
            )

        if intent.kind == "send_email":
            recipient = intent.recipient_name or intent.recipient_email or "your contact"
            return Plan(
                goal=intent.original_text,
                success_message=f"Sent an email to {recipient}.",
                steps=[
                    PlanStep(
                        kind="gmail_send_email",
                        description=f"Send a Gmail message to {recipient}",
                        params={
                            "recipient_name": intent.recipient_name,
                            "recipient_email": intent.recipient_email,
                            "subject": intent.subject,
                            "body": intent.body,
                            "account_hint": intent.account_hint,
                            "source_command": intent.original_text,
                        },
                        verification={"gmail_sent": True},
                    )
                ],
            )

        if intent.kind == "send_message":
            recipient = intent.recipient_name or intent.recipient_phone or "your contact"
            return Plan(
                goal=intent.original_text,
                success_message=f"Sent a message to {recipient}.",
                steps=[
                    PlanStep(
                        kind="messages_send",
                        description=f"Send a Messages text to {recipient}",
                        params={
                            "recipient_name": intent.recipient_name,
                            "recipient_phone": intent.recipient_phone,
                            "body": intent.body,
                            "source_command": intent.original_text,
                        },
                        verification={"message_sent": True},
                    )
                ],
            )

        if intent.kind == "play_spotify":
            return Plan(
                goal=intent.original_text,
                success_message=f"Started Spotify playback for {intent.query}.",
                steps=[
                    PlanStep(
                        kind="spotify_play",
                        description=f"Play {intent.query} on Spotify",
                        params={"query": intent.query or ""},
                        verification={"spotify_playing": True, "query": intent.query or ""},
                    )
                ],
            )

        return Plan(
            goal=intent.original_text,
            success_message="I could not safely map that command to a supported action.",
            steps=[],
        )

    @staticmethod
    def _intent_from_action(action: dict, original_text: str) -> Intent:
        return Intent(
            kind=action.get("kind", "unsupported"),
            original_text=original_text,
            app_name=action.get("app_name"),
            query=action.get("query"),
            target_text=action.get("target_text"),
            target_app=action.get("target_app"),
            recipient_name=action.get("recipient_name"),
            recipient_email=action.get("recipient_email"),
            recipient_phone=action.get("recipient_phone"),
            subject=action.get("subject"),
            body=action.get("body"),
            account_hint=action.get("account_hint"),
            response_text=action.get("response_text"),
            confidence=float(action.get("confidence") or 0.6),
            used_llm=True,
            metadata=action.get("metadata") or {},
        )
