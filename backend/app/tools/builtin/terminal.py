from __future__ import annotations

import asyncio
from typing import Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition
from app.utils.paths import resolve_user_path


class TerminalArgs(ToolArgs):
    command: str = Field(description="Shell command to run.")
    cwd: Optional[str] = Field(default=None, description="Working directory for the command.")
    timeout_seconds: int = Field(default=90, ge=1, le=900)


async def terminal_handler(context: ToolContext, args: TerminalArgs) -> dict:
    cwd = (
        str(resolve_user_path(context.settings, args.cwd))
        if args.cwd
        else str(context.settings.workspace_root.resolve())
    )
    process = await asyncio.create_subprocess_shell(
        args.command,
        cwd=cwd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )

    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=args.timeout_seconds)
    except asyncio.TimeoutError:
        process.kill()
        await process.communicate()
        raise TimeoutError(f"Command timed out after {args.timeout_seconds} seconds.")

    return {
        "command": args.command,
        "cwd": cwd,
        "exit_code": process.returncode,
        "stdout": stdout.decode("utf-8", errors="replace"),
        "stderr": stderr.decode("utf-8", errors="replace"),
    }


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="terminal_tool",
        description="Run terminal commands when the task requires shell access, diagnostics, or local automation.",
        args_model=TerminalArgs,
        handler=terminal_handler,
        supports_parallel=False,
        side_effecting=True,
    )
