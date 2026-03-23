from __future__ import annotations

import json
import subprocess
from typing import Literal, Optional

from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition


class MacOSUIArgs(ToolArgs):
    operation: Literal[
        "frontmost_app",
        "inspect_front_window",
        "click_named",
        "focus_named",
        "menu_click",
    ]
    name: Optional[str] = Field(
        default=None,
        description="Name or partial label of the target UI element.",
    )
    role: Optional[str] = Field(
        default=None,
        description="Optional accessibility role description filter such as button, text field, checkbox, or menu item.",
    )
    menu_path: Optional[str] = Field(
        default=None,
        description="Menu path as a comma-separated sequence like File,New Window.",
    )
    max_results: int = Field(default=40, ge=1, le=200)


def _run_osascript(script: str) -> str:
    result = subprocess.run(
        ["osascript", "-e", script],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "AppleScript execution failed.")
    return result.stdout.strip()


def _escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def _frontmost_app() -> str:
    return _run_osascript(
        """
        tell application "System Events"
          return name of first application process whose frontmost is true
        end tell
        """
    )


def _inspect_front_window(max_results: int) -> dict:
    script = f"""
    set outputLines to {{}}
    tell application "System Events"
      set frontApp to name of first application process whose frontmost is true
      tell process frontApp
        set windowTitle to ""
        try
          if (count of windows) > 0 then
            set windowTitle to name of window 1
            set allItems to entire contents of window 1
            set seenCount to 0
            repeat with uiItem in allItems
              if seenCount >= {max_results} then exit repeat
              try
                set roleDesc to role description of uiItem
                set itemName to name of uiItem
                if itemName is missing value then set itemName to ""
                if roleDesc is missing value then set roleDesc to ""
                if itemName is not "" or roleDesc is not "" then
                  set end of outputLines to (roleDesc & "||" & itemName)
                  set seenCount to seenCount + 1
                end if
              end try
            end repeat
          end if
        end try
      end tell
    end tell
    return frontApp & "\\n" & windowTitle & "\\n" & (outputLines as string)
    """
    raw = _run_osascript(script)
    lines = raw.splitlines()
    app_name = lines[0] if lines else ""
    window_title = lines[1] if len(lines) > 1 else ""
    elements_raw = "\n".join(lines[2:]) if len(lines) > 2 else ""
    elements: list[dict[str, str]] = []
    for chunk in elements_raw.split(", "):
        if "||" not in chunk:
            continue
        role_desc, item_name = chunk.split("||", 1)
        elements.append({"role": role_desc.strip(), "name": item_name.strip()})
    return {
        "application": app_name,
        "window_title": window_title,
        "elements": elements,
    }


def _click_or_focus_named(name: str, role: str | None, action: str) -> dict:
    action_line = "click uiItem"
    if action == "focus":
        action_line = "set focused of uiItem to true"
    role_filter = ""
    if role:
        role_filter = f"""
                if roleDesc does not contain "{_escape(role.lower())}" then
                  set matchesRole to false
                end if
        """
    script = f"""
    tell application "System Events"
      set frontApp to name of first application process whose frontmost is true
      tell process frontApp
        if (count of windows) is 0 then error "No front window is available."
        set allItems to entire contents of window 1
        repeat with uiItem in allItems
          try
            set itemName to name of uiItem
            if itemName is missing value then set itemName to ""
            set roleDesc to role description of uiItem
            if roleDesc is missing value then set roleDesc to ""
            set lowerName to do shell script "printf %s " & quoted form of itemName & " | tr '[:upper:]' '[:lower:]'"
            set matchesRole to true
            {role_filter}
            if matchesRole and lowerName contains "{_escape(name.lower())}" then
              {action_line}
              return frontApp & "||" & roleDesc & "||" & itemName
            end if
          end try
        end repeat
      end tell
    end tell
    error "No matching UI element was found."
    """
    raw = _run_osascript(script)
    app_name, matched_role, matched_name = raw.split("||", 2)
    return {
        "application": app_name,
        "role": matched_role,
        "name": matched_name,
        "action": action,
    }


def _menu_click(menu_path: str) -> dict:
    parts = [part.strip() for part in menu_path.split(",") if part.strip()]
    if len(parts) < 2:
        raise ValueError("menu_path must include at least a menu and a menu item.")
    menu_name = parts[0]
    item_name = parts[-1]
    script = f"""
    tell application "System Events"
      set frontApp to name of first application process whose frontmost is true
      tell process frontApp
        click menu item "{_escape(item_name)}" of menu "{_escape(menu_name)}" of menu bar item "{_escape(menu_name)}" of menu bar 1
      end tell
      return frontApp
    end tell
    """
    app_name = _run_osascript(script)
    return {"application": app_name, "menu_path": parts}


async def macos_ui_handler(context: ToolContext, args: MacOSUIArgs) -> dict:
    if args.operation == "frontmost_app":
        return {"application": _frontmost_app()}

    if args.operation == "inspect_front_window":
        return _inspect_front_window(args.max_results)

    if args.operation == "click_named":
        if not args.name:
            raise ValueError("name is required for click_named")
        return _click_or_focus_named(args.name, args.role, "click")

    if args.operation == "focus_named":
        if not args.name:
            raise ValueError("name is required for focus_named")
        return _click_or_focus_named(args.name, args.role, "focus")

    if args.operation == "menu_click":
        if not args.menu_path:
            raise ValueError("menu_path is required for menu_click")
        return _menu_click(args.menu_path)

    raise ValueError(f"Unsupported macOS UI operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="macos_ui_tool",
        description=(
            "Observe and control the frontmost macOS application through Accessibility UI elements. "
            "Use this for generic computer-use tasks: inspect the current window, click named buttons or controls, "
            "focus fields, and drive menus."
        ),
        args_model=MacOSUIArgs,
        handler=macos_ui_handler,
        supports_parallel=False,
        side_effecting=True,
    )
