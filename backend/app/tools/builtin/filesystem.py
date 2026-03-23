from __future__ import annotations

import shutil
import subprocess
from pathlib import Path
from typing import Literal, Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition
from app.utils.paths import resolve_user_path


class FileSystemArgs(ToolArgs):
    operation: Literal[
        "list",
        "read_text",
        "write_text",
        "append_text",
        "search",
        "mkdir",
        "move",
        "copy",
        "delete",
        "stat",
    ]
    path: str = Field(description="Target file or directory path.")
    destination: Optional[str] = Field(default=None, description="Required for copy or move.")
    content: Optional[str] = Field(default=None, description="Required for write or append.")
    pattern: Optional[str] = Field(default=None, description="Search regex for operation='search'.")
    recursive: bool = True


async def filesystem_handler(context: ToolContext, args: FileSystemArgs) -> dict:
    path = resolve_user_path(context.settings, args.path)

    if args.operation == "list":
        if not path.is_dir():
            raise NotADirectoryError(str(path))
        entries = [
            {"name": item.name, "path": str(item), "is_dir": item.is_dir()}
            for item in sorted(path.iterdir(), key=lambda entry: (not entry.is_dir(), entry.name.lower()))
        ]
        return {"path": str(path), "entries": entries}

    if args.operation == "read_text":
        return {"path": str(path), "content": path.read_text(encoding="utf-8")}

    if args.operation == "write_text":
        if args.content is None:
            raise ValueError("content is required for write_text")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(args.content, encoding="utf-8")
        return {"path": str(path), "bytes_written": len(args.content.encode("utf-8"))}

    if args.operation == "append_text":
        if args.content is None:
            raise ValueError("content is required for append_text")
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as handle:
            handle.write(args.content)
        return {"path": str(path), "bytes_appended": len(args.content.encode("utf-8"))}

    if args.operation == "search":
        if not args.pattern:
            raise ValueError("pattern is required for search")
        command = ["rg", "-n", args.pattern, str(path)]
        if not args.recursive:
            command.insert(1, "--max-depth")
            command.insert(2, "1")
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        return {"path": str(path), "matches": result.stdout.splitlines(), "stderr": result.stderr}

    if args.operation == "mkdir":
        path.mkdir(parents=True, exist_ok=True)
        return {"path": str(path), "created": True}

    if args.operation in {"move", "copy"}:
        if not args.destination:
            raise ValueError("destination is required for move/copy")
        destination = resolve_user_path(context.settings, args.destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if args.operation == "move":
            shutil.move(str(path), str(destination))
        else:
            if path.is_dir():
                shutil.copytree(path, destination, dirs_exist_ok=True)
            else:
                shutil.copy2(path, destination)
        return {"source": str(path), "destination": str(destination)}

    if args.operation == "delete":
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
        return {"path": str(path), "deleted": True}

    if args.operation == "stat":
        stat = path.stat()
        return {
            "path": str(path),
            "exists": path.exists(),
            "is_dir": path.is_dir(),
            "size": stat.st_size,
            "modified": stat.st_mtime,
        }

    raise ValueError(f"Unsupported filesystem operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="filesystem_tool",
        description="Inspect or modify files and directories on disk, including search, reads, writes, and moves.",
        args_model=FileSystemArgs,
        handler=filesystem_handler,
        side_effecting=True,
        requires_confirmation=False,
    )
