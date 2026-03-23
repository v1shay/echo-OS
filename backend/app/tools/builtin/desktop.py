from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Literal, Optional

import mss
import pyautogui
import pyperclip
from pydantic import Field

from app.tools.base import ToolArgs, ToolContext, ToolDefinition


class DesktopArgs(ToolArgs):
    operation: Literal[
        "clipboard_read",
        "clipboard_write",
        "screenshot",
        "keypress",
        "hotkey",
        "type_text",
        "click",
        "double_click",
        "right_click",
        "move_mouse",
        "drag_mouse",
        "scroll",
        "mouse_position",
        "wait",
    ]
    text: Optional[str] = Field(default=None, description="Clipboard or keyboard text.")
    key: Optional[str] = Field(default=None, description="Key name for keypress.")
    keys: Optional[str] = Field(default=None, description="Comma-separated key names for hotkey.")
    x: Optional[int] = None
    y: Optional[int] = None
    clicks: int = Field(default=1, ge=1, le=3)
    duration_seconds: float = Field(default=0.0, ge=0.0, le=30.0)
    amount: Optional[int] = Field(default=None, description="Scroll amount in wheel units.")


async def desktop_handler(context: ToolContext, args: DesktopArgs) -> dict:
    if args.operation == "clipboard_read":
        return {"clipboard": pyperclip.paste()}

    if args.operation == "clipboard_write":
        if args.text is None:
            raise ValueError("text is required for clipboard_write")
        pyperclip.copy(args.text)
        return {"clipboard": args.text}

    if args.operation == "screenshot":
        screenshot_dir = Path(context.settings.workspace_root) / "tmp" / "desktop"
        screenshot_dir.mkdir(parents=True, exist_ok=True)
        target = screenshot_dir / f"{context.session_id}.png"
        with mss.mss() as sct:
            sct.shot(output=str(target))
        return {"path": str(target)}

    if args.operation == "keypress":
        if not args.key:
            raise ValueError("key is required for keypress")
        await asyncio.to_thread(pyautogui.press, args.key)
        return {"key": args.key}

    if args.operation == "hotkey":
        if not args.keys:
            raise ValueError("keys is required for hotkey")
        keys = [key.strip() for key in args.keys.split(",") if key.strip()]
        if not keys:
            raise ValueError("keys is required for hotkey")
        await asyncio.to_thread(pyautogui.hotkey, *keys)
        return {"keys": keys}

    if args.operation == "type_text":
        if args.text is None:
            raise ValueError("text is required for type_text")
        await asyncio.to_thread(pyautogui.write, args.text, interval=0.01)
        return {"typed": args.text}

    if args.operation == "click":
        if args.x is None or args.y is None:
            raise ValueError("x and y are required for click")
        await asyncio.to_thread(pyautogui.click, x=args.x, y=args.y, clicks=args.clicks)
        return {"clicked": {"x": args.x, "y": args.y, "clicks": args.clicks}}

    if args.operation == "double_click":
        if args.x is None or args.y is None:
            raise ValueError("x and y are required for double_click")
        await asyncio.to_thread(pyautogui.doubleClick, x=args.x, y=args.y)
        return {"double_clicked": {"x": args.x, "y": args.y}}

    if args.operation == "right_click":
        if args.x is None or args.y is None:
            raise ValueError("x and y are required for right_click")
        await asyncio.to_thread(pyautogui.rightClick, x=args.x, y=args.y)
        return {"right_clicked": {"x": args.x, "y": args.y}}

    if args.operation == "move_mouse":
        if args.x is None or args.y is None:
            raise ValueError("x and y are required for move_mouse")
        await asyncio.to_thread(pyautogui.moveTo, args.x, args.y, duration=args.duration_seconds)
        return {"moved_to": {"x": args.x, "y": args.y, "duration_seconds": args.duration_seconds}}

    if args.operation == "drag_mouse":
        if args.x is None or args.y is None:
            raise ValueError("x and y are required for drag_mouse")
        await asyncio.to_thread(pyautogui.dragTo, args.x, args.y, duration=args.duration_seconds, button="left")
        return {"dragged_to": {"x": args.x, "y": args.y, "duration_seconds": args.duration_seconds}}

    if args.operation == "scroll":
        if args.amount is None:
            raise ValueError("amount is required for scroll")
        await asyncio.to_thread(pyautogui.scroll, args.amount)
        return {"scrolled": args.amount}

    if args.operation == "mouse_position":
        x, y = await asyncio.to_thread(pyautogui.position)
        return {"x": x, "y": y}

    if args.operation == "wait":
        await asyncio.sleep(args.duration_seconds)
        return {"waited_seconds": args.duration_seconds}

    raise ValueError(f"Unsupported desktop operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="desktop_tool",
        description="Interact with the clipboard, keyboard, mouse, and screenshots on the desktop.",
        args_model=DesktopArgs,
        handler=desktop_handler,
        supports_parallel=False,
        side_effecting=True,
        requires_confirmation=True,
    )
