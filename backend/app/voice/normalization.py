from __future__ import annotations

from difflib import get_close_matches
from functools import lru_cache
from pathlib import Path
import re

from app.config import Settings

ACTION_PREFIXES = ("open ", "launch ", "start ", "play ")
WAKEWORD_ALIASES = (
    "i go",
    "igo",
    "ego",
    "eggo",
    "a go",
    "ay go",
)


@lru_cache(maxsize=1)
def _app_catalog() -> dict[str, str]:
    roots = [
        Path("/Applications"),
        Path("/System/Applications"),
        Path.home() / "Applications",
    ]
    catalog: dict[str, str] = {}
    for root in roots:
        if not root.exists():
            continue
        for app in root.glob("*.app"):
            catalog[app.stem.lower()] = app.stem

    for builtin in ["Spotify", "Messages", "Safari", "Google Chrome", "Finder", "Visual Studio Code"]:
        catalog.setdefault(builtin.lower(), builtin)
    return catalog


def normalize_spoken_command(text: str, settings: Settings) -> str:
    normalized_text = text.strip()
    lowered = normalized_text.lower()

    for alias in WAKEWORD_ALIASES:
        pattern = rf"^{re.escape(alias)}(?=\b|[\s,.:;!?-])"
        if re.match(pattern, lowered):
            remainder = re.sub(pattern, "", normalized_text, count=1, flags=re.IGNORECASE).lstrip(" ,:;.!?-")
            normalized_text = f"echo {remainder}".strip()
            lowered = normalized_text.lower()
            break

    wakeword_prefix = ""
    if lowered.startswith("echo "):
        wakeword_prefix = normalized_text[:5]
        normalized_text = normalized_text[5:]
        lowered = normalized_text.lower()

    for prefix in ACTION_PREFIXES:
        if not lowered.startswith(prefix):
            continue
        spoken_target = normalized_text[len(prefix) :].strip(" .,!?:;")
        if not spoken_target:
            return f"{wakeword_prefix}{normalized_text}".strip()
        catalog = _app_catalog()
        compact_catalog = {key.replace(" ", ""): value for key, value in catalog.items()}
        normalized_spoken = spoken_target.lower()
        candidates = list(catalog.keys()) + list(compact_catalog.keys())
        match = get_close_matches(
            normalized_spoken.replace(" ", ""),
            candidates,
            n=1,
            cutoff=0.58,
        )
        if not match:
            return f"{wakeword_prefix}{normalized_text}".strip()
        normalized_target = catalog.get(match[0], compact_catalog.get(match[0]))
        if not normalized_target:
            return f"{wakeword_prefix}{normalized_text}".strip()
        return f"{wakeword_prefix}{prefix.strip()} {normalized_target}".strip()
    return f"{wakeword_prefix}{normalized_text}".strip()
