from __future__ import annotations

from difflib import SequenceMatcher
import json
import subprocess
from pathlib import Path

from .config import JarvisConfig
from .runtime import ensure_dir
from .schemas import Observation, OcrBox


class VisionObserver:
    def __init__(self, config: JarvisConfig) -> None:
        self.config = config

    def ensure_ocr_binary(self) -> None:
        binary = self.config.paths.ocr_binary
        if binary.exists():
            return
        ensure_dir(binary.parent)
        subprocess.run(
            [
                "swiftc",
                "-framework",
                "Vision",
                "-framework",
                "AppKit",
                str(self.config.paths.ocr_source),
                "-o",
                str(binary),
            ],
            check=True,
        )

    def observe(self, run_dir: Path, label: str) -> Observation:
        self.ensure_ocr_binary()
        screenshot_path = run_dir / f"{label}.png"
        subprocess.run(["screencapture", "-x", str(screenshot_path)], check=True)
        ocr_payload = subprocess.check_output(
            [str(self.config.paths.ocr_binary), str(screenshot_path)],
            text=True,
        )
        data = json.loads(ocr_payload)
        boxes = [
            OcrBox(
                text=item["text"],
                confidence=float(item["confidence"]),
                x=float(item["x"]),
                y=float(item["y"]),
                width=float(item["width"]),
                height=float(item["height"]),
            )
            for item in data.get("boxes", [])
        ]
        return Observation(
            screenshot_path=screenshot_path,
            frontmost_app=self.frontmost_app(),
            window_title=self.front_window_title(),
            screen_text=data.get("full_text", ""),
            ocr_boxes=boxes,
        )

    def frontmost_app(self) -> str | None:
        script = (
            'tell application "System Events" to get name of first application process '
            "whose frontmost is true"
        )
        try:
            return (
                subprocess.check_output(["osascript", "-e", script], text=True).strip() or None
            )
        except Exception:
            return None

    def front_window_title(self) -> str | None:
        script = """
        tell application "System Events"
            try
                set frontApp to first application process whose frontmost is true
                tell front window of frontApp
                    return value of attribute "AXTitle"
                end tell
            on error
                return ""
            end try
        end tell
        """
        try:
            title = subprocess.check_output(["osascript", "-e", script], text=True).strip()
            return title or None
        except Exception:
            return None

    def find_text(self, observation: Observation, query: str) -> OcrBox | None:
        normalized_query = " ".join(query.lower().split())
        best_box: OcrBox | None = None
        best_score = 0.0
        for box in observation.ocr_boxes:
            candidate = " ".join(box.text.lower().split())
            if not candidate:
                continue
            score = self._score_candidate(candidate, normalized_query)
            if score > best_score:
                best_score = score
                best_box = box
        return best_box if best_score >= 0.55 else None

    @staticmethod
    def _score_candidate(candidate: str, query: str) -> float:
        if candidate == query:
            return 1.0
        if query in candidate:
            return 0.92
        if candidate in query and len(candidate) >= 4:
            return 0.78
        query_tokens = set(query.split())
        candidate_tokens = set(candidate.split())
        if not query_tokens:
            return 0.0
        overlap = len(query_tokens & candidate_tokens) / len(query_tokens)
        ratio = SequenceMatcher(None, candidate, query).ratio()
        return max(overlap, ratio * 0.9)
