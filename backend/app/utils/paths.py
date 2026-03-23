from __future__ import annotations

from pathlib import Path

from app.config import Settings


def resolve_user_path(settings: Settings, raw_path: str) -> Path:
    candidate = Path(raw_path).expanduser()
    if not candidate.is_absolute():
        candidate = (settings.workspace_root / candidate).resolve()
    else:
        candidate = candidate.resolve()

    if not any(_is_relative_to(candidate, root) for root in settings.allowed_root_paths):
        raise PermissionError(f"Path '{candidate}' is outside the allowed roots.")

    return candidate


def _is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False
