from __future__ import annotations

import argparse
import json
import sys
import time

from .agent import DesktopAgent
from .config import JarvisConfig
from .runtime import make_run_dir, write_json


SCENARIO_COMMANDS = [
    "Open Google Chrome",
    "Search YouTube for NBA highlights",
    "Type hello world in a text editor",
    "Open my personal account in Google Chrome and send an email to Vishay saying something nice",
    "Send a text message to Vishay saying have a great day",
    "Play Sticky by Drake on Spotify",
]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Jarvis Voice OS")
    subparsers = parser.add_subparsers(dest="command", required=False)

    run_parser = subparsers.add_parser("run", help="Run a single command")
    run_parser.add_argument("--text", help="Text command to execute")
    run_parser.add_argument("--voice", action="store_true", help="Capture a voice command")
    run_parser.add_argument("--audio-file", help="Transcribe and execute a WAV command")
    run_parser.add_argument("--seconds", type=int, default=None, help="Voice capture length")
    run_parser.add_argument("--mute", action="store_true", help="Disable spoken responses")

    loop_parser = subparsers.add_parser("voice-loop", help="Continuous voice loop")
    loop_parser.add_argument("--seconds", type=int, default=None, help="Voice capture length")
    loop_parser.add_argument("--mute", action="store_true", help="Disable spoken responses")

    subparsers.add_parser("scenario-test", help="Run the required local test flows")
    subparsers.add_parser(
        "voice-scenario-test",
        help="Run the required local test flows through synthesized spoken audio",
    )
    return parser


def run_scenarios(agent: DesktopAgent, mute: bool = True) -> int:
    results = []
    for command in SCENARIO_COMMANDS:
        agent.controller.hide_app("jarvis_rs")
        result = agent.run_text_command(command, speak=not mute)
        print(f"[{'PASS' if result.success else 'FAIL'}] {command} -> {result.message}")
        results.append(
            {
                "command": command,
                "success": result.success,
                "message": result.message,
                "artifacts_dir": str(result.artifacts_dir),
            }
        )
    report_dir = make_run_dir(agent.config.paths.artifacts_dir, "scenario-test")
    write_json(report_dir / "scenario-report.json", {"results": results})
    failures = [item for item in results if not item["success"]]
    return 0 if not failures else 1


def run_voice_scenarios(agent: DesktopAgent) -> int:
    report_dir = make_run_dir(agent.config.paths.artifacts_dir, "voice-scenario-test")
    audio_dir = report_dir / "audio"
    audio_dir.mkdir(parents=True, exist_ok=True)
    results = []

    for index, command in enumerate(SCENARIO_COMMANDS, start=1):
        agent.controller.hide_app("jarvis_rs")
        audio_path = agent.voice.synthesize_test_wav(command, audio_dir / f"{index:02d}.wav")
        result = agent.run_audio_command(audio_path, speak=False, notify_on_empty=False)
        print(f"[{'PASS' if result.success else 'FAIL'}] {command} -> {result.message}")
        results.append(
            {
                "command": command,
                "audio_file": str(audio_path),
                "success": result.success,
                "message": result.message,
                "artifacts_dir": str(result.artifacts_dir),
            }
        )

    echo_audio = agent.voice.synthesize_test_wav(
        "I could not safely execute that request.",
        audio_dir / "echo.wav",
    )
    agent.last_spoken_text = "I could not safely execute that request."
    agent.last_spoken_at = time.time()
    echo_result = agent.run_audio_command(echo_audio, speak=False, notify_on_empty=False)
    results.append(
        {
            "command": "assistant echo suppression",
            "audio_file": str(echo_audio),
            "success": echo_result.message == "Ignoring assistant echo.",
            "message": echo_result.message,
            "artifacts_dir": str(echo_result.artifacts_dir),
        }
    )
    print(
        f"[{'PASS' if echo_result.message == 'Ignoring assistant echo.' else 'FAIL'}] "
        f"Assistant echo suppression -> {echo_result.message}"
    )

    write_json(report_dir / "voice-scenario-report.json", {"results": results})
    failures = [item for item in results if not item["success"]]
    return 0 if not failures else 1


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    config = JarvisConfig.from_env()
    agent = DesktopAgent(config)

    if args.command in (None, "run"):
        if getattr(args, "voice", False):
            result = agent.run_voice_command(
                seconds=args.seconds,
                speak=not args.mute,
            )
        elif getattr(args, "audio_file", None):
            result = agent.run_audio_command(
                audio_path=args.audio_file,
                speak=not args.mute,
                notify_on_empty=False,
            )
        elif getattr(args, "text", None):
            result = agent.run_text_command(args.text, speak=not args.mute)
        else:
            print("Use `run --text ...`, `run --voice`, or `run --audio-file ...`.", file=sys.stderr)
            return 2
        print(json.dumps({"success": result.success, "message": result.message, "artifacts_dir": str(result.artifacts_dir)}, indent=2))
        return 0 if result.success else 1

    if args.command == "voice-loop":
        print("Jarvis voice loop is live. Press Ctrl+C to stop.")
        try:
            while True:
                result = agent.run_voice_command(
                    seconds=args.seconds,
                    speak=not args.mute,
                    notify_on_empty=False,
                    suppress_short_unsupported=True,
                )
                if result.message in {
                    "I didn't catch that. Please try again.",
                    "Ignoring assistant echo.",
                    "Ignoring unsupported voice input.",
                }:
                    continue
                print(
                    json.dumps(
                        {
                            "success": result.success,
                            "message": result.message,
                            "artifacts_dir": str(result.artifacts_dir),
                        }
                    )
                )
                time.sleep(0.5)
        except KeyboardInterrupt:
            return 0

    if args.command == "scenario-test":
        return run_scenarios(agent, mute=True)

    if args.command == "voice-scenario-test":
        return run_voice_scenarios(agent)

    return 0
