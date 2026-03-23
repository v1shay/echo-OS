from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, Awaitable, Callable
from urllib.parse import urlencode

EventSink = Callable[[dict[str, Any]], Awaitable[None]]


class HardcodedIntent(str, Enum):
    HELLO = "hello"
    PLAY_HAPPY = "play_happy"
    PAUSE_HAPPY = "pause_happy"
    EMAIL_TEACHER = "email_teacher"
    TEXT_VISHWESH = "text_vishwesh"
    ORGANIZE_FILES = "organize_files"


@dataclass(slots=True)
class IntentMatch:
    intent: HardcodedIntent
    reason: str


@dataclass(slots=True)
class DemoActionResult:
    response: str
    execution_log: list[dict[str, Any]]
    verification: dict[str, Any]


def normalize_demo_input(text: str) -> str:
    normalized = " ".join(text.strip().lower().split())
    if normalized.startswith("echo "):
        return normalized[5:].strip()
    if normalized == "echo":
        return ""
    return normalized


def build_teacher_email() -> tuple[str, str]:
    subject = "Absence Tomorrow"
    body = (
        "Hello,\n\n"
        "I wanted to let you know that I am not feeling well and will not be able to come in tomorrow. "
        "I wanted to give you notice as early as possible.\n\n"
        "Thank you for understanding.\n\n"
        "Best,\n"
        "Vishay"
    )
    return subject, body


def match_hardcoded_intent(text: str) -> IntentMatch | None:
    normalized = normalize_demo_input(text)
    if not normalized:
        return None

    if normalized in {"hello", "hi", "hey"} or (
        "hello" in normalized and "how are you" not in normalized
    ):
        return IntentMatch(HardcodedIntent.HELLO, "matched greeting")

    if "pause" in normalized and any(token in normalized for token in {"spotify", "music", "song", "it"}):
        return IntentMatch(HardcodedIntent.PAUSE_HAPPY, "matched spotify pause request")

    if "play" in normalized and (
        "happy" in normalized
        or "pharrell" in normalized
        or "happy music" in normalized
    ):
        return IntentMatch(HardcodedIntent.PLAY_HAPPY, "matched happy playback request")

    if "email" in normalized and (
        "teacher" in normalized
        or "school" in normalized
        or "rushilcpm02@gmail.com" in normalized
    ):
        return IntentMatch(HardcodedIntent.EMAIL_TEACHER, "matched teacher email request")

    if "text" in normalized and ("vishwesh" in normalized or "mall" in normalized):
        return IntentMatch(HardcodedIntent.TEXT_VISHWESH, "matched mall text request")

    if any(phrase in normalized for phrase in {"put all my files in folders", "organize my files", "sort my files"}):
        return IntentMatch(HardcodedIntent.ORGANIZE_FILES, "matched file organization request")
    if "files" in normalized and "folder" in normalized:
        return IntentMatch(HardcodedIntent.ORGANIZE_FILES, "matched generic file organization request")

    return None


class DemoMacOSExecutor:
    HAPPY_TRACK_URIS = [
        "spotify:track:60nZcImufyMA1MKQY3dcCH",
        "spotify:track:0eMxgAHmuvoqpLyYQrbKvQ",
    ]

    ACKNOWLEDGEMENTS = {
        HardcodedIntent.PLAY_HAPPY: "Playing Happy for you now.",
        HardcodedIntent.PAUSE_HAPPY: "Paused it for you.",
        HardcodedIntent.EMAIL_TEACHER: "I handled the teacher email.",
        HardcodedIntent.TEXT_VISHWESH: "I texted Vishwesh for you.",
        HardcodedIntent.ORGANIZE_FILES: "I sorted your files into folders.",
    }

    def __init__(self, settings):
        self.settings = settings

    def execute(self, intent: HardcodedIntent) -> DemoActionResult:
        handler = {
            HardcodedIntent.HELLO: self.handle_hello,
            HardcodedIntent.PLAY_HAPPY: self.handle_play_happy,
            HardcodedIntent.PAUSE_HAPPY: self.handle_pause_happy,
            HardcodedIntent.EMAIL_TEACHER: self.handle_email_teacher,
            HardcodedIntent.TEXT_VISHWESH: self.handle_text_vishwesh,
            HardcodedIntent.ORGANIZE_FILES: self.handle_organize_files,
        }[intent]
        return handler()

    def handle_hello(self) -> DemoActionResult:
        return DemoActionResult(
            response="Hello Vishay, how are you doing?",
            execution_log=[
                {"phase": "greeting", "intent": HardcodedIntent.HELLO.value, "detail": "Returned scripted greeting."}
            ],
            verification={
                "completed": True,
                "intent": HardcodedIntent.HELLO.value,
                "confidence": 1.0,
                "reasoning": "Greeting is a fully scripted response with no external side effects.",
                "next_action": "",
            },
        )

    def handle_play_happy(self) -> DemoActionResult:
        execution_log: list[dict[str, Any]] = []
        self._ensure_app_exists("Spotify")
        self._run_process(["open", "-a", "Spotify"])
        execution_log.append({"phase": "activate", "application": "Spotify"})

        track_match: tuple[str, str, str] | None = None
        for uri in self.HAPPY_TRACK_URIS:
            self._run_osascript(f'tell application "Spotify" to play track "{uri}"')
            time.sleep(2)
            track_name = self._spotify_property("name of current track")
            track_artist = self._spotify_property("artist of current track")
            execution_log.append(
                {
                    "phase": "play_attempt",
                    "uri": uri,
                    "track_name": track_name,
                    "track_artist": track_artist,
                }
            )
            if "happy" in track_name.lower() and "pharrell" in track_artist.lower():
                track_match = (uri, track_name, track_artist)
                break

        if not track_match:
            raise RuntimeError("Spotify did not start Happy by Pharrell Williams.")

        uri, track_name, track_artist = track_match
        return DemoActionResult(
            response=self.ACKNOWLEDGEMENTS[HardcodedIntent.PLAY_HAPPY],
            execution_log=execution_log,
            verification={
                "completed": True,
                "intent": HardcodedIntent.PLAY_HAPPY.value,
                "confidence": 1.0,
                "reasoning": f'Playing "{track_name}" by {track_artist} from Spotify.',
                "track_uri": uri,
                "next_action": "",
            },
        )

    def handle_pause_happy(self) -> DemoActionResult:
        self._ensure_app_exists("Spotify")
        self._run_osascript('tell application "Spotify" to pause')
        player_state = self._run_osascript('tell application "Spotify" to get player state')
        if "paused" not in player_state.lower():
            raise RuntimeError("Spotify did not pause.")

        return DemoActionResult(
            response=self.ACKNOWLEDGEMENTS[HardcodedIntent.PAUSE_HAPPY],
            execution_log=[
                {"phase": "pause", "application": "Spotify", "player_state": player_state},
            ],
            verification={
                "completed": True,
                "intent": HardcodedIntent.PAUSE_HAPPY.value,
                "confidence": 1.0,
                "reasoning": "Spotify is paused.",
                "next_action": "",
            },
        )

    def handle_email_teacher(self) -> DemoActionResult:
        self._ensure_app_exists("Google Chrome")
        subject, body = build_teacher_email()
        compose_url = self._gmail_compose_url(
            to="rushilcpm02@gmail.com",
            subject=subject,
            body=body,
        )

        self._run_process(["open", "-a", "Google Chrome", compose_url])
        time.sleep(3)
        current_url = self._chrome_active_tab_url()
        if "mail.google.com" not in current_url:
            raise RuntimeError("Gmail is not open in the signed-in Chrome session.")

        self._send_command_enter()
        time.sleep(1)
        follow_up_url = self._chrome_active_tab_url()

        return DemoActionResult(
            response=self.ACKNOWLEDGEMENTS[HardcodedIntent.EMAIL_TEACHER],
            execution_log=[
                {"phase": "open_compose", "url": compose_url},
                {"phase": "verify_compose", "active_url": current_url},
                {"phase": "send_shortcut", "shortcut": "command+enter"},
                {"phase": "post_send_state", "active_url": follow_up_url},
            ],
            verification={
                "completed": True,
                "intent": HardcodedIntent.EMAIL_TEACHER.value,
                "confidence": 0.75,
                "reasoning": "Opened Gmail in Chrome, verified the Gmail session, and issued the send shortcut.",
                "subject": subject,
                "recipient": "rushilcpm02@gmail.com",
                "next_action": "",
            },
        )

    def handle_text_vishwesh(self) -> DemoActionResult:
        message = "yo bro lets go to the mall"
        self._ensure_system_app("/System/Applications/Messages.app", "Messages")
        script = f'''
        tell application "Messages" to activate
        delay 0.6
        tell application "System Events"
          keystroke "n" using command down
          delay 0.6
          keystroke "Vishwesh"
          delay 1.0
          key code 36
          delay 0.4
          key code 48
          delay 0.2
          keystroke "{self._escape_applescript_string(message)}"
          delay 0.2
          key code 36
        end tell
        '''
        self._run_osascript(script)
        time.sleep(1)
        front_chat = self._run_osascript('tell application "Messages" to get name of front window')
        if "vish" not in front_chat.lower():
            raise RuntimeError("Messages did not focus the Vishwesh conversation.")

        return DemoActionResult(
            response=self.ACKNOWLEDGEMENTS[HardcodedIntent.TEXT_VISHWESH],
            execution_log=[
                {"phase": "activate_messages", "application": "Messages"},
                {"phase": "compose_message", "recipient": "Vishwesh", "message": message},
                {"phase": "verify_chat", "front_window": front_chat},
            ],
            verification={
                "completed": True,
                "intent": HardcodedIntent.TEXT_VISHWESH.value,
                "confidence": 0.8,
                "reasoning": "Messages focused a Vishwesh chat and submitted the scripted message.",
                "next_action": "",
            },
        )

    def handle_organize_files(self) -> DemoActionResult:
        recent_files = self._recent_files(limit=40)
        if not recent_files:
            raise RuntimeError("No recent files were found to organize.")

        target_root = Path.home() / "Desktop" / "Echo Organized"
        categories = {
            "Documents": target_root / "Documents",
            "Media": target_root / "Media",
            "Code": target_root / "Code",
            "Archives": target_root / "Archives",
        }
        for directory in [target_root, *categories.values()]:
            directory.mkdir(parents=True, exist_ok=True)

        moved: list[dict[str, str]] = []
        for source in recent_files:
            if target_root in source.parents:
                continue
            category = self._classify_path(source)
            destination = self._dedupe_destination(categories[category] / source.name)
            destination.parent.mkdir(parents=True, exist_ok=True)
            source.rename(destination)
            moved.append(
                {
                    "source": str(source),
                    "destination": str(destination),
                    "category": category,
                }
            )

        if not moved:
            raise RuntimeError("Recent files were found, but none could be moved safely.")

        self._run_process(["open", str(target_root)])
        return DemoActionResult(
            response=self.ACKNOWLEDGEMENTS[HardcodedIntent.ORGANIZE_FILES],
            execution_log=[
                {"phase": "discover_recent_files", "count": len(recent_files)},
                {"phase": "move_files", "count": len(moved), "moved": moved[:20]},
                {"phase": "reveal", "path": str(target_root)},
            ],
            verification={
                "completed": True,
                "intent": HardcodedIntent.ORGANIZE_FILES.value,
                "confidence": 0.9,
                "reasoning": f"Moved {len(moved)} recent files into four Desktop folders and opened the result in Finder.",
                "organized_root": str(target_root),
                "next_action": "",
            },
        )

    def _recent_files(self, limit: int) -> list[Path]:
        query = "kMDItemLastUsedDate >= $time.today(-30)"
        result = self._run_process(
            ["mdfind", "-onlyin", str(Path.home()), query],
            check=False,
        )
        paths: list[Path] = []
        seen: set[Path] = set()
        for raw_line in result.stdout.splitlines():
            candidate = Path(raw_line).expanduser()
            if candidate in seen:
                continue
            if not candidate.exists() or not candidate.is_file():
                continue
            if candidate.name.startswith(".") or candidate.suffix == ".app":
                continue
            seen.add(candidate)
            paths.append(candidate)
        paths.sort(key=lambda item: max(item.stat().st_atime, item.stat().st_mtime), reverse=True)
        return paths[:limit]

    def _classify_path(self, path: Path) -> str:
        suffix = path.suffix.lower()
        if suffix in {
            ".jpg", ".jpeg", ".png", ".gif", ".heic", ".webp", ".svg", ".mp4", ".mov", ".m4v", ".mp3", ".wav",
            ".aiff", ".aac", ".m4a",
        }:
            return "Media"
        if suffix in {
            ".py", ".ts", ".tsx", ".js", ".jsx", ".json", ".html", ".css", ".scss", ".md", ".yml", ".yaml",
            ".java", ".kt", ".swift", ".c", ".cc", ".cpp", ".h", ".hpp", ".rs", ".go", ".sh", ".zsh", ".toml",
        }:
            return "Code"
        if suffix in {".zip", ".rar", ".7z", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".dmg", ".pkg"}:
            return "Archives"
        return "Documents"

    def _dedupe_destination(self, destination: Path) -> Path:
        if not destination.exists():
            return destination
        stem = destination.stem
        suffix = destination.suffix
        counter = 2
        while True:
            candidate = destination.with_name(f"{stem} ({counter}){suffix}")
            if not candidate.exists():
                return candidate
            counter += 1

    def _gmail_compose_url(self, *, to: str, subject: str, body: str) -> str:
        query = urlencode(
            {
                "view": "cm",
                "fs": "1",
                "tf": "1",
                "to": to,
                "su": subject,
                "body": body,
            }
        )
        return f"https://mail.google.com/mail/?{query}"

    def _chrome_active_tab_url(self) -> str:
        return self._run_osascript('tell application "Google Chrome" to get URL of active tab of front window')

    def _spotify_property(self, expression: str) -> str:
        return self._run_osascript(f'tell application "Spotify" to get {expression}')

    def _send_command_enter(self) -> None:
        script = '''
        tell application "Google Chrome" to activate
        delay 0.2
        tell application "System Events"
          key code 36 using command down
        end tell
        '''
        self._run_osascript(script)

    def _ensure_app_exists(self, app_name: str) -> None:
        for root in (Path("/Applications"), Path.home() / "Applications"):
            if (root / f"{app_name}.app").exists():
                return
        raise RuntimeError(f"{app_name} is not installed.")

    def _ensure_system_app(self, path: str, app_name: str) -> None:
        if not Path(path).exists():
            raise RuntimeError(f"{app_name} is not installed.")

    def _run_process(self, command: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        if check and result.returncode != 0:
            stderr = result.stderr.strip() or result.stdout.strip()
            raise RuntimeError(stderr or f"Command failed: {' '.join(command)}")
        return result

    def _run_osascript(self, script: str) -> str:
        result = subprocess.run(
            ["osascript", "-e", script],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or "AppleScript execution failed.")
        return result.stdout.strip()

    def _escape_applescript_string(self, value: str) -> str:
        return value.replace("\\", "\\\\").replace('"', '\\"')


class HardcodedDemoRuntime:
    def __init__(self, settings):
        self.settings = settings
        self.executor = DemoMacOSExecutor(settings)
        self._event_sinks: dict[str, EventSink] = {}

    def attach_event_sink(self, session_id: str, sink: EventSink) -> None:
        self._event_sinks[session_id] = sink

    def detach_event_sink(self, session_id: str) -> None:
        self._event_sinks.pop(session_id, None)

    async def publish(self, session_id: str, event: dict[str, Any]) -> None:
        sink = self._event_sinks.get(session_id)
        if sink:
            await sink(event)

    async def run_task(self, session_id: str, user_request: str) -> dict[str, Any]:
        normalized = normalize_demo_input(user_request)
        await self.publish(session_id, {"type": "status", "state": "thinking"})
        match = match_hardcoded_intent(normalized)
        if not match:
            return {
                "session_id": session_id,
                "response": "Is there anything else I can help you with?",
                "execution_log": [
                    {"phase": "match_intent", "matched": False, "input": normalized},
                ],
                "verification": {
                    "completed": False,
                    "confidence": 1.0,
                    "reasoning": "No supported hardcoded demo intent matched the request.",
                    "next_action": "",
                },
                "memories": [],
            }

        await self.publish(
            session_id,
            {
                "type": "status",
                "state": "executing",
                "tools": [match.intent.value],
            },
        )

        try:
            result = self.executor.execute(match.intent)
        except Exception as exc:
            return {
                "session_id": session_id,
                "response": self._failure_line(match.intent, str(exc)),
                "execution_log": [
                    {
                        "phase": "match_intent",
                        "matched": True,
                        "intent": match.intent.value,
                        "reason": match.reason,
                    },
                    {
                        "phase": "error",
                        "intent": match.intent.value,
                        "error": str(exc),
                    },
                ],
                "verification": {
                    "completed": False,
                    "intent": match.intent.value,
                    "confidence": 0.0,
                    "reasoning": str(exc),
                    "next_action": "",
                },
                "memories": [],
            }

        return {
            "session_id": session_id,
            "response": result.response,
            "execution_log": [
                {
                    "phase": "match_intent",
                    "matched": True,
                    "intent": match.intent.value,
                    "reason": match.reason,
                },
                *result.execution_log,
            ],
            "verification": result.verification,
            "memories": [],
        }

    def _failure_line(self, intent: HardcodedIntent, error: str) -> str:
        if intent is HardcodedIntent.PLAY_HAPPY:
            return "Spotify did not start the right song."
        if intent is HardcodedIntent.PAUSE_HAPPY:
            return "Spotify would not pause."
        if intent is HardcodedIntent.EMAIL_TEACHER:
            return "Gmail was not ready in Chrome."
        if intent is HardcodedIntent.TEXT_VISHWESH:
            return "I could not lock onto Vishwesh in Messages."
        if intent is HardcodedIntent.ORGANIZE_FILES:
            return "I could not organize the recent files."
        return error or "I could not complete that."
