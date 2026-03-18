from __future__ import annotations

import re
import time
from pathlib import Path

from .browser import BrowserSidecarClient
from .config import JarvisConfig
from .control import GuiController
from .intent import IntentResolver
from .llm import LocalLlmClient
from .memory import MemoryStore
from .planner import Planner
from .runtime import ensure_dir, make_run_dir, write_json
from .schemas import ExecutionResult, Intent, Observation, PlanStep, StepRecord
from .verifier import Verifier
from .vision import VisionObserver
from .voice import Speaker, VoicePipeline


class DesktopAgent:
    def __init__(self, config: JarvisConfig) -> None:
        self.config = config
        self.llm = LocalLlmClient(config)
        self.intent_resolver = IntentResolver(self.llm)
        self.planner = Planner()
        self.controller = GuiController(config)
        self.browser = BrowserSidecarClient(config)
        self.observer = VisionObserver(config)
        self.verifier = Verifier()
        self.memory = MemoryStore(config.paths.memory_path)
        self.voice = VoicePipeline(config.voice)
        self.speaker = Speaker(config.voice)
        self.last_spoken_text = ""
        self.last_spoken_at = 0.0

    def run_text_command(self, command: str, speak: bool = True) -> ExecutionResult:
        run_dir = make_run_dir(self.config.paths.artifacts_dir, command)
        ensure_dir(run_dir)
        intent = self.intent_resolver.resolve(command)
        plan = self.planner.create_plan(intent)
        steps: list[StepRecord] = []

        if intent.kind == "unsupported" or not plan.steps:
            message = "I could not safely execute that request."
            result = ExecutionResult(
                command=command,
                success=False,
                message=message,
                intent=intent,
                plan=plan,
                steps=steps,
                artifacts_dir=run_dir,
            )
            self._persist_result(result)
            if speak:
                self._speak(message)
            return result

        if speak:
            self._speak(intent.response_text or "Working on it.")

        success = True
        message = plan.success_message
        for index, step in enumerate(plan.steps, start=1):
            record = self._execute_step(run_dir, index, step)
            steps.append(record)
            if not record.success:
                success = False
                message = record.summary
                break

        if speak:
            self._speak(message if success else f"I hit a problem. {message}")

        result = ExecutionResult(
            command=command,
            success=success,
            message=message,
            intent=intent,
            plan=plan,
            steps=steps,
            artifacts_dir=run_dir,
        )
        self._persist_result(result)
        return result

    def run_voice_command(
        self,
        seconds: int | None = None,
        speak: bool = True,
        notify_on_empty: bool = True,
        suppress_short_unsupported: bool = False,
    ) -> ExecutionResult:
        transcript = self.voice.listen_once(seconds=seconds).strip()
        return self._run_transcript(
            transcript,
            speak=speak,
            notify_on_empty=notify_on_empty,
            suppress_short_unsupported=suppress_short_unsupported,
        )

    def run_audio_command(
        self,
        audio_path: Path,
        speak: bool = False,
        notify_on_empty: bool = False,
        suppress_short_unsupported: bool = False,
    ) -> ExecutionResult:
        transcript = self.voice.transcribe(Path(audio_path)).strip()
        return self._run_transcript(
            transcript,
            speak=speak,
            notify_on_empty=notify_on_empty,
            suppress_short_unsupported=suppress_short_unsupported,
        )

    def _run_transcript(
        self,
        transcript: str,
        speak: bool,
        notify_on_empty: bool,
        suppress_short_unsupported: bool,
    ) -> ExecutionResult:
        if not transcript or transcript.upper() == "[BLANK_AUDIO]":
            return self._voice_non_command_result(
                label="blank-audio",
                command="",
                message="I didn't catch that. Please try again.",
                speak=speak and notify_on_empty,
            )
        if self._is_self_echo(transcript):
            return self._voice_non_command_result(
                label="assistant-echo",
                command=transcript,
                message="Ignoring assistant echo.",
                speak=False,
            )
        if suppress_short_unsupported:
            intent = self.intent_resolver.resolve(transcript)
            if intent.kind == "unsupported" and len(transcript.split()) <= 6:
                return self._voice_non_command_result(
                    label="ignored-unsupported",
                    command=transcript,
                    message="Ignoring unsupported voice input.",
                    speak=False,
                )
        return self.run_text_command(transcript, speak=speak)

    def _voice_non_command_result(
        self,
        label: str,
        command: str,
        message: str,
        speak: bool,
    ) -> ExecutionResult:
        run_dir = make_run_dir(self.config.paths.artifacts_dir, label)
        ensure_dir(run_dir)
        intent = Intent(kind="unsupported", original_text=command, confidence=1.0)
        plan = self.planner.create_plan(intent)
        result = ExecutionResult(
            command=command,
            success=False,
            message=message,
            intent=intent,
            plan=plan,
            steps=[],
            artifacts_dir=run_dir,
        )
        self._persist_result(result)
        if speak:
            self._speak(message)
        return result

    def _speak(self, text: str) -> None:
        self.last_spoken_text = text
        self.last_spoken_at = time.time()
        self.speaker.speak(text)

    def _is_self_echo(self, transcript: str) -> bool:
        if not self.last_spoken_text:
            return False
        if time.time() - self.last_spoken_at > 15:
            return False
        heard = self._normalize_phrase(transcript)
        spoken = self._normalize_phrase(self.last_spoken_text)
        if not heard or not spoken:
            return False
        return heard == spoken or heard in spoken or spoken in heard

    @staticmethod
    def _normalize_phrase(text: str) -> str:
        return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()

    def _execute_step(self, run_dir, index: int, step: PlanStep) -> StepRecord:
        label = f"step-{index:02d}"
        attempts = 0
        last_summary = "Unknown failure"
        last_verification: dict = {}
        last_details: dict = {}
        before_path = None
        after_path = None

        while attempts < self.config.max_step_retries:
            attempts += 1
            before_obs = self.observer.observe(run_dir, f"{label}-before-{attempts}")
            before_path = str(before_obs.screenshot_path)
            try:
                action_details = self._perform_action(step, before_obs)
            except Exception as exc:
                last_summary = f"{step.description} raised {exc}"
                last_details = {"error": str(exc)}
                continue
            time.sleep(self.config.settle_delay_seconds)
            after_obs = self.observer.observe(run_dir, f"{label}-after-{attempts}")
            after_path = str(after_obs.screenshot_path)

            success, verification = self._verify_step(step, after_obs, action_details)
            if success:
                return StepRecord(
                    description=step.description,
                    success=True,
                    summary="Verified",
                    before_screenshot=before_path,
                    after_screenshot=after_path,
                    verification=verification,
                    details=action_details,
                )
            last_summary = f"{step.description} did not verify after attempt {attempts}"
            last_verification = verification
            last_details = action_details

        return StepRecord(
            description=step.description,
            success=False,
            summary=last_summary,
            before_screenshot=before_path,
            after_screenshot=after_path,
            verification=last_verification,
            details=last_details,
        )

    def _perform_action(self, step: PlanStep, before_obs: Observation) -> dict:
        if step.kind == "open_app":
            self.controller.open_app(step.params["app_name"])
            return {"tool": "open_app", "app_name": step.params["app_name"]}
        if step.kind == "hide_app":
            self.controller.hide_app(step.params["app_name"])
            return {"tool": "hide_app", "app_name": step.params["app_name"]}
        if step.kind == "textedit_new_document":
            self.controller.new_textedit_document()
            return {"tool": "textedit_new_document"}
        if step.kind == "textedit_set_text":
            return {
                "tool": "textedit_set_text",
                "text": step.params["text"],
                "textedit_document_text": self.controller.set_textedit_text(step.params["text"]),
            }
        if step.kind == "press_hotkey":
            if step.verification.get("frontmost_app"):
                self.controller.activate_app(step.verification["frontmost_app"])
            self.controller.hotkey(*step.params["keys"])
            return {"tool": "press_hotkey", "keys": step.params["keys"]}
        if step.kind == "type_text":
            if step.verification.get("frontmost_app"):
                self.controller.activate_app(step.verification["frontmost_app"])
            self.controller.type_text(step.params["text"])
            return {
                "tool": "type_text",
                "text": step.params["text"],
                "captured_text": self.controller.copy_frontmost_text(),
            }
        if step.kind == "browser_attach":
            state = self.browser.post("/browser/attach_or_launch")
            return {"tool": "browser_attach", "browser_state": state}
        if step.kind == "browser_open":
            state = self.browser.post("/browser/open", {"url": step.params["url"]})
            return {"tool": "browser_open", "browser_state": state, "url": step.params["url"]}
        if step.kind == "browser_assert":
            state = self.browser.post("/browser/assert", step.verification)
            return {"tool": "browser_assert", "browser_state": state}
        if step.kind == "gmail_send_email":
            return self._execute_gmail_send(step, before_obs)
        if step.kind == "messages_send":
            return self._execute_messages_send(step)
        if step.kind == "spotify_play":
            return self._execute_spotify_play(step, before_obs)
        raise RuntimeError(f"Unsupported step kind: {step.kind}")

    def _verify_step(self, step: PlanStep, after_obs: Observation, action_details: dict) -> tuple[bool, dict]:
        if step.kind.startswith("browser_"):
            browser_state = action_details.get("browser_state") or self.browser.post("/browser/snapshot")
            ok, details = self.verifier.verify_browser(browser_state, step.verification)
            return ok, {"browser": details}
        if step.kind == "gmail_send_email":
            ok = bool(action_details.get("gmail_sent"))
            details = {
                "gmail_sent": ok,
                "recipient_email": action_details.get("recipient_email"),
                "subject": action_details.get("subject"),
                "message_sent_detected": action_details.get("message_sent_detected"),
            }
            return ok, details
        if step.kind == "messages_send":
            ok = bool(action_details.get("message_sent"))
            details = {
                "message_sent": ok,
                "recipient_handle": action_details.get("recipient_handle"),
                "recipient_name": action_details.get("recipient_name"),
            }
            return ok, details
        if step.kind == "spotify_play":
            player = action_details.get("player_state") or {}
            query = str(step.verification.get("query", ""))
            combined = " ".join(
                [
                    str(player.get("state", "")),
                    str(player.get("track", "")),
                    str(player.get("artist", "")),
                ]
            ).lower()
            ok = "playing" in combined and self._query_matches_player_state(query, player)
            details = {"player_state": player, "query": query, "matched": ok}
            return ok, details
        ok, details = self.verifier.verify(after_obs, step.verification, action_details=action_details)
        return ok, details

    def _execute_gmail_send(self, step: PlanStep, before_obs: Observation) -> dict:
        recipient = self._resolve_contact(step.params)
        recipient_email = step.params.get("recipient_email") or recipient.get("email") or ""
        recipient_name = step.params.get("recipient_name") or recipient.get("name") or "your contact"
        if not recipient_email:
            raise RuntimeError(f"Could not resolve an email address for {recipient_name}.")

        subject, body = self._draft_message(
            channel="email",
            recipient_name=recipient_name,
            source_command=str(step.params.get("source_command", "")),
            subject=step.params.get("subject"),
            body=step.params.get("body"),
        )
        compose_url = self.controller.gmail_compose_url(
            to_email=recipient_email,
            subject=subject,
            body=body,
            account_hint=step.params.get("account_hint"),
        )
        tab_info = self.controller.chrome_open_url(compose_url)
        time.sleep(3)

        run_dir = before_obs.screenshot_path.parent
        compose_obs = self.observer.observe(run_dir, f"gmail-compose-{int(time.time())}")
        send_box = self.observer.find_text(compose_obs, "Send")
        ocr_text = self._observation_text(compose_obs)
        if send_box is None:
            raise RuntimeError("Gmail compose view did not expose a Send button.")
        compose_ready = recipient_email.lower() in compose_url.lower() and "mail.google.com" in str(
            tab_info.get("url", "")
        ).lower()
        if not compose_ready and recipient_email.lower() not in ocr_text.lower():
            raise RuntimeError("Gmail compose view did not show the resolved recipient.")
        self.controller.click_point(*send_box.center)
        time.sleep(3)
        sent_obs = self.observer.observe(run_dir, f"gmail-sent-{int(time.time())}")
        sent_text = self._observation_text(sent_obs)
        sent_detected = "message sent" in sent_text.lower()
        return {
            "tool": "gmail_send_email",
            "recipient_name": recipient_name,
            "recipient_email": recipient_email,
            "subject": subject,
            "body": body,
            "compose_url": compose_url,
            "tab_info": tab_info,
            "compose_text": ocr_text,
            "sent_text": sent_text,
            "message_sent_detected": sent_detected,
            "gmail_sent": sent_detected,
        }

    def _execute_messages_send(self, step: PlanStep) -> dict:
        recipient = self._resolve_contact(step.params)
        recipient_name = step.params.get("recipient_name") or recipient.get("name") or "your contact"
        recipient_handle = (
            step.params.get("recipient_phone")
            or recipient.get("phone")
            or recipient.get("email")
            or ""
        )
        if not recipient_handle:
            raise RuntimeError(f"Could not resolve a Messages handle for {recipient_name}.")

        _subject, body = self._draft_message(
            channel="message",
            recipient_name=recipient_name,
            source_command=str(step.params.get("source_command", "")),
            body=step.params.get("body"),
        )
        send_result = self.controller.send_imessage(recipient_handle, body)
        return {
            "tool": "messages_send",
            "recipient_name": recipient_name,
            "recipient_handle": send_result.get("handle") or recipient_handle,
            "body": body,
            "message_sent": bool(send_result.get("handle")),
        }

    def _execute_spotify_play(self, step: PlanStep, before_obs: Observation) -> dict:
        query = str(step.params.get("query", "")).strip()
        if not query:
            raise RuntimeError("Spotify command is missing a query.")

        self.controller.spotify_open_search(query)
        time.sleep(2)
        run_dir = before_obs.screenshot_path.parent
        search_obs = self.observer.observe(run_dir, f"spotify-search-{int(time.time())}")
        primary_target = self._spotify_primary_target(query)
        result_box = self.observer.find_text(search_obs, primary_target) or self.observer.find_text(
            search_obs, query
        )
        if result_box is not None:
            self.controller.click_point(*result_box.center)
            time.sleep(0.5)
            self.controller.hotkey("enter")
        player_state = self.controller.spotify_play()
        return {
            "tool": "spotify_play",
            "query": query,
            "selected_target": primary_target,
            "player_state": player_state,
        }

    def _resolve_contact(self, params: dict) -> dict:
        resolved: dict[str, str] = {}
        if params.get("recipient_name"):
            resolved = self.controller.resolve_contact(str(params["recipient_name"])) or {}
        if params.get("recipient_email"):
            resolved["email"] = str(params["recipient_email"])
        if params.get("recipient_phone"):
            resolved["phone"] = str(params["recipient_phone"])
        if params.get("recipient_name"):
            resolved.setdefault("name", str(params["recipient_name"]))
        return resolved

    def _draft_message(
        self,
        channel: str,
        recipient_name: str,
        source_command: str,
        subject: str | None = None,
        body: str | None = None,
    ) -> tuple[str, str]:
        cleaned_subject = (subject or "").strip()
        cleaned_body = (body or "").strip()
        if cleaned_body and cleaned_body.lower() not in {"something nice", "say something nice"}:
            return cleaned_subject or f"Note for {recipient_name}", cleaned_body

        llm_payload = self.llm.json_completion(
            system_prompt=(
                "Return only JSON with keys subject and body. "
                "Write a short, warm, specific message for the named recipient. "
                "Do not mention AI, automation, or that this is a test unless the command says so. "
                "Do not invent birthdays, trips, deadlines, or other context that the user did not mention. "
                "Keep subject under 8 words and body under 60 words."
            ),
            user_prompt=(
                f"Channel: {channel}\n"
                f"Recipient: {recipient_name}\n"
                f"User command: {source_command}\n"
                "Generate a ready-to-send message."
            ),
        )
        if llm_payload:
            drafted_subject = str(llm_payload.get("subject") or cleaned_subject or f"Note for {recipient_name}").strip()
            drafted_body = str(llm_payload.get("body") or "").strip()
            if drafted_body:
                return drafted_subject, drafted_body

        fallback_body = (
            f"Hey {recipient_name}, I just wanted to say I appreciate you and I hope your day is going really well."
        )
        return cleaned_subject or f"Hello {recipient_name}", cleaned_body or fallback_body

    @staticmethod
    def _observation_text(observation: Observation) -> str:
        combined = [observation.screen_text]
        combined.extend(box.text for box in observation.ocr_boxes)
        return " ".join(part for part in combined if part).strip()

    @staticmethod
    def _spotify_primary_target(query: str) -> str:
        normalized = re.sub(r"\s+by\s+.+$", "", query, flags=re.IGNORECASE).strip()
        return normalized or query

    @staticmethod
    def _query_matches_player_state(query: str, player_state: dict) -> bool:
        normalized_query = query.lower()
        combined = " ".join(
            [
                str(player_state.get("track", "")),
                str(player_state.get("artist", "")),
            ]
        ).lower()
        if not normalized_query:
            return False
        if normalized_query in combined:
            return True
        terms = [term for term in re.split(r"[^a-z0-9]+", normalized_query) if term and term not in {"play", "song", "spotify", "by"}]
        if not terms:
            return False
        return sum(term in combined for term in terms) >= max(1, len(terms) - 1)

    def _persist_result(self, result: ExecutionResult) -> None:
        payload = {
            "command": result.command,
            "success": result.success,
            "message": result.message,
            "intent": {
                "kind": result.intent.kind,
                "app_name": result.intent.app_name,
                "query": result.intent.query,
                "target_text": result.intent.target_text,
                "target_app": result.intent.target_app,
                "recipient_name": result.intent.recipient_name,
                "recipient_email": result.intent.recipient_email,
                "recipient_phone": result.intent.recipient_phone,
                "subject": result.intent.subject,
                "body": result.intent.body,
                "account_hint": result.intent.account_hint,
                "confidence": result.intent.confidence,
                "used_llm": result.intent.used_llm,
                "metadata": result.intent.metadata,
            },
            "plan": {
                "goal": result.plan.goal,
                "success_message": result.plan.success_message,
                "steps": [
                    {
                        "kind": step.kind,
                        "description": step.description,
                        "params": step.params,
                        "verification": step.verification,
                    }
                    for step in result.plan.steps
                ],
            },
            "steps": [
                {
                    "description": step.description,
                    "success": step.success,
                    "summary": step.summary,
                    "before_screenshot": step.before_screenshot,
                    "after_screenshot": step.after_screenshot,
                    "verification": step.verification,
                    "details": step.details,
                }
                for step in result.steps
            ],
        }
        write_json(result.artifacts_dir / "result.json", payload)
        self.memory.append_execution(
            command=result.command,
            success=result.success,
            message=result.message,
            intent_kind=result.intent.kind,
        )
