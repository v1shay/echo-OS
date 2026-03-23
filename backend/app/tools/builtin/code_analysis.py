from __future__ import annotations

import subprocess
from typing import Literal, Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition
from app.utils.paths import resolve_user_path


class CodeAnalysisArgs(ToolArgs):
    operation: Literal["repo_map", "search_symbol", "lint_path", "run_tests", "git_status"]
    path: Optional[str] = Field(default=None, description="Repository or file path.")
    query: Optional[str] = Field(default=None, description="Search symbol or text.")
    command: Optional[str] = Field(default=None, description="Custom test command for run_tests.")


async def code_analysis_handler(context: ToolContext, args: CodeAnalysisArgs) -> dict:
    root = (
        resolve_user_path(context.settings, args.path)
        if args.path
        else context.settings.workspace_root.resolve()
    )

    if args.operation == "repo_map":
        files = subprocess.run(
            ["rg", "--files", str(root)],
            capture_output=True,
            text=True,
            check=False,
        )
        return {"path": str(root), "files": files.stdout.splitlines()[:1000]}

    if args.operation == "search_symbol":
        if not args.query:
            raise ValueError("query is required for search_symbol")
        result = subprocess.run(
            ["rg", "-n", args.query, str(root)],
            capture_output=True,
            text=True,
            check=False,
        )
        return {"path": str(root), "matches": result.stdout.splitlines()[:500], "stderr": result.stderr}

    if args.operation == "lint_path":
        target = str(root)
        result = subprocess.run(
            ["ruff", "check", target],
            capture_output=True,
            text=True,
            check=False,
        )
        return {"path": target, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}

    if args.operation == "run_tests":
        command = args.command or "pytest"
        result = subprocess.run(
            command,
            shell=True,
            cwd=str(root),
            capture_output=True,
            text=True,
            check=False,
        )
        return {"path": str(root), "command": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}

    if args.operation == "git_status":
        result = subprocess.run(
            ["git", "status", "--short"],
            cwd=str(root),
            capture_output=True,
            text=True,
            check=False,
        )
        return {"path": str(root), "stdout": result.stdout, "stderr": result.stderr}

    raise ValueError(f"Unsupported code analysis operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="code_analysis_tool",
        description="Scan repositories, search code, run linting, inspect git status, and execute test suites.",
        args_model=CodeAnalysisArgs,
        handler=code_analysis_handler,
        side_effecting=False,
    )
