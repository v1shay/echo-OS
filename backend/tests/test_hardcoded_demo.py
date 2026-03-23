from __future__ import annotations

from pathlib import Path

import pytest

from app.demo.runtime import (
    DemoMacOSExecutor,
    HardcodedIntent,
    build_teacher_email,
    match_hardcoded_intent,
)
from app.config import Settings
from app.voice.normalization import normalize_spoken_command


def test_matches_exact_demo_phrases() -> None:
    assert match_hardcoded_intent("echo hello").intent is HardcodedIntent.HELLO
    assert match_hardcoded_intent("play some happy music for me").intent is HardcodedIntent.PLAY_HAPPY
    assert match_hardcoded_intent("great, can you pause it for me").intent is HardcodedIntent.PAUSE_HAPPY
    assert match_hardcoded_intent(
        "email my teacher telling her im sick and cant come"
    ).intent is HardcodedIntent.EMAIL_TEACHER
    assert match_hardcoded_intent(
        "text my friend vishwesh asking him if he wants to go to the mall"
    ).intent is HardcodedIntent.TEXT_VISHWESH
    assert match_hardcoded_intent("can you put all my files in folders").intent is HardcodedIntent.ORGANIZE_FILES


def test_matches_expected_paraphrases() -> None:
    assert match_hardcoded_intent("echo play happy by pharrell").intent is HardcodedIntent.PLAY_HAPPY
    assert match_hardcoded_intent("pause the spotify music").intent is HardcodedIntent.PAUSE_HAPPY
    assert match_hardcoded_intent("organize my files into folders").intent is HardcodedIntent.ORGANIZE_FILES


def test_unsupported_input_returns_none() -> None:
    assert match_hardcoded_intent("open safari") is None


def test_normalize_spoken_echo_aliases() -> None:
    settings = Settings()
    assert normalize_spoken_command("i go hello", settings) == "echo hello"
    assert normalize_spoken_command("ego play spotify", settings) == "echo play Spotify"


def test_teacher_email_copy_is_polite_and_not_raw_dump() -> None:
    subject, body = build_teacher_email()
    assert subject == "Absence Tomorrow"
    assert "I am not feeling well" in body
    assert "email my teacher" not in body.lower()
    assert "yo bro" not in body.lower()


def test_acknowledgements_stay_short() -> None:
    for intent, line in DemoMacOSExecutor.ACKNOWLEDGEMENTS.items():
        assert intent is not HardcodedIntent.HELLO
        assert len(line.split()) <= 7
        assert line.endswith(".")


def test_play_happy_verifies_track(monkeypatch: pytest.MonkeyPatch) -> None:
    executor = DemoMacOSExecutor(settings=object())
    calls: list[tuple[str, str]] = []

    monkeypatch.setattr(executor, "_ensure_app_exists", lambda app_name: None)
    monkeypatch.setattr(executor, "_run_process", lambda command, check=True: calls.append(("process", " ".join(command))))
    monkeypatch.setattr(
        executor,
        "_run_osascript",
        lambda script: calls.append(("osascript", script)) or "",
    )
    responses = iter(["Happy - From \"Despicable Me 2\"", "Pharrell Williams"])
    monkeypatch.setattr(executor, "_spotify_property", lambda expression: next(responses))
    monkeypatch.setattr("app.demo.runtime.time.sleep", lambda _: None)

    result = executor.handle_play_happy()

    assert result.verification["completed"] is True
    assert "Pharrell Williams" in result.verification["reasoning"]
    assert any("Spotify" in entry[1] for entry in calls)


def test_pause_happy_verifies_paused_state(monkeypatch: pytest.MonkeyPatch) -> None:
    executor = DemoMacOSExecutor(settings=object())
    seen_scripts: list[str] = []

    monkeypatch.setattr(executor, "_ensure_app_exists", lambda app_name: None)
    monkeypatch.setattr(
        executor,
        "_run_osascript",
        lambda script: seen_scripts.append(script) or ("paused" if "player state" in script else ""),
    )

    result = executor.handle_pause_happy()

    assert result.verification["completed"] is True
    assert any("pause" in script for script in seen_scripts)


def test_email_flow_opens_gmail_and_issues_send(monkeypatch: pytest.MonkeyPatch) -> None:
    executor = DemoMacOSExecutor(settings=object())
    commands: list[list[str]] = []
    send_called = {"value": False}

    monkeypatch.setattr(executor, "_ensure_app_exists", lambda app_name: None)
    monkeypatch.setattr(
        executor,
        "_run_process",
        lambda command, check=True: commands.append(command),
    )
    monkeypatch.setattr(executor, "_chrome_active_tab_url", lambda: "https://mail.google.com/mail/u/0/#inbox")
    monkeypatch.setattr(executor, "_send_command_enter", lambda: send_called.__setitem__("value", True))
    monkeypatch.setattr("app.demo.runtime.time.sleep", lambda _: None)

    result = executor.handle_email_teacher()

    assert result.verification["completed"] is True
    assert commands and commands[0][0] == "open"
    assert "mail.google.com" in commands[0][-1]
    assert send_called["value"] is True


def test_messages_flow_uses_native_app_and_verifies_chat(monkeypatch: pytest.MonkeyPatch) -> None:
    executor = DemoMacOSExecutor(settings=object())
    scripts: list[str] = []

    monkeypatch.setattr(executor, "_ensure_system_app", lambda path, app_name: None)
    monkeypatch.setattr(
        executor,
        "_run_osascript",
        lambda script: scripts.append(script) or ("Vishwesh" if "front window" in script else ""),
    )
    monkeypatch.setattr("app.demo.runtime.time.sleep", lambda _: None)

    result = executor.handle_text_vishwesh()

    assert result.verification["completed"] is True
    assert any("Messages" in script for script in scripts)
    assert any("yo bro lets go to the mall" in script for script in scripts)


def test_file_organization_creates_folders_and_moves_files(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    executor = DemoMacOSExecutor(settings=object())
    recent_dir = tmp_path / "recent"
    recent_dir.mkdir()
    doc = recent_dir / "notes.txt"
    code = recent_dir / "app.ts"
    archive = recent_dir / "bundle.zip"
    media = recent_dir / "clip.mov"
    for path in [doc, code, archive, media]:
        path.write_text("demo", encoding="utf-8")

    desktop = tmp_path / "Desktop"
    desktop.mkdir()
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: tmp_path))
    monkeypatch.setattr(executor, "_recent_files", lambda limit=40: [doc, code, archive, media])
    opened_paths: list[list[str]] = []
    monkeypatch.setattr(executor, "_run_process", lambda command, check=True: opened_paths.append(command))

    result = executor.handle_organize_files()

    root = desktop / "Echo Organized"
    assert result.verification["completed"] is True
    assert (root / "Documents" / "notes.txt").exists()
    assert (root / "Code" / "app.ts").exists()
    assert (root / "Archives" / "bundle.zip").exists()
    assert (root / "Media" / "clip.mov").exists()
    assert opened_paths and opened_paths[0][0] == "open"
