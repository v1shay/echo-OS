from __future__ import annotations

import subprocess
from typing import Literal, Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition


class MacOSAutomationArgs(ToolArgs):
    operation: Literal["run_applescript", "activate_application", "list_applications"]
    script: Optional[str] = Field(
        default=None,
        description="AppleScript source for run_applescript. The script should perform the intended macOS action.",
    )
    application: Optional[str] = Field(
        default=None,
        description="Application name for activate_application.",
    )
    query: Optional[str] = Field(
        default=None,
        description="Optional search term for list_applications.",
    )


async def macos_automation_handler(context: ToolContext, args: MacOSAutomationArgs) -> dict:
    if args.operation == "activate_application":
        if not args.application:
            raise ValueError("application is required for activate_application")
        result = subprocess.run(
            ["open", "-a", args.application],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or f"Failed to activate {args.application}.")
        return {"activated_application": args.application}

    if args.operation == "list_applications":
        command = "mdfind 'kMDItemContentType == \"com.apple.application-bundle\"'"
        if args.query:
            escaped = args.query.replace('"', '\\"')
            command += f" | grep -i \"{escaped}\""
        result = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            check=False,
        )
        return {"applications": result.stdout.splitlines()[:200], "stderr": result.stderr}

    if args.operation == "run_applescript":
        if not args.script:
            raise ValueError("script is required for run_applescript")
        result = subprocess.run(
            ["osascript", "-e", args.script],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or "AppleScript execution failed.")
        return {"stdout": result.stdout.strip(), "stderr": result.stderr.strip()}

    raise ValueError(f"Unsupported macOS automation operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="macos_automation_tool",
        description=(
            "General macOS automation tool for AppleScript-based app control, activating applications, "
            "and discovering installed apps."
        ),
        args_model=MacOSAutomationArgs,
        handler=macos_automation_handler,
        supports_parallel=False,
        side_effecting=True,
    )
