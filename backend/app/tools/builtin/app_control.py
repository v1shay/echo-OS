from __future__ import annotations

import subprocess
from typing import Literal, Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition
from app.utils.paths import resolve_user_path


class AppControlArgs(ToolArgs):
    operation: Literal["open_application", "open_path", "open_url", "reveal_in_finder"]
    application: Optional[str] = Field(default=None, description="Application name, such as 'Visual Studio Code'.")
    path: Optional[str] = Field(default=None, description="File or directory path.")
    url: Optional[str] = Field(default=None, description="URL to open.")


async def app_control_handler(context: ToolContext, args: AppControlArgs) -> dict:
    if args.operation == "open_application":
        if not args.application:
            raise ValueError("application is required for open_application")
        subprocess.run(["open", "-a", args.application], check=True)
        return {"opened_application": args.application}

    if args.operation == "open_path":
        if not args.path:
            raise ValueError("path is required for open_path")
        path = resolve_user_path(context.settings, args.path)
        subprocess.run(["open", str(path)], check=True)
        return {"opened_path": str(path)}

    if args.operation == "open_url":
        if not args.url:
            raise ValueError("url is required for open_url")
        subprocess.run(["open", args.url], check=True)
        return {"opened_url": args.url}

    if args.operation == "reveal_in_finder":
        if not args.path:
            raise ValueError("path is required for reveal_in_finder")
        path = resolve_user_path(context.settings, args.path)
        subprocess.run(["open", "-R", str(path)], check=True)
        return {"revealed_path": str(path)}

    raise ValueError(f"Unsupported app control operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="app_control_tool",
        description="Open native applications, files, folders, or URLs on the local machine.",
        args_model=AppControlArgs,
        handler=app_control_handler,
        supports_parallel=False,
        side_effecting=True,
    )
