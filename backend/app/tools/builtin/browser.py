from __future__ import annotations

from pathlib import Path
from typing import Literal, Optional

from pydantic import Field
from playwright.async_api import Browser, BrowserContext, Page, Playwright, async_playwright

from app.tools.base import ToolArgs, ToolContext, ToolDefinition


class BrowserArgs(ToolArgs):
    operation: Literal["launch", "navigate", "click", "fill", "press", "extract_text", "screenshot", "close"]
    url: Optional[str] = None
    selector: Optional[str] = None
    text: Optional[str] = None
    key: Optional[str] = None
    timeout_ms: int = Field(default=10000, ge=100, le=60000)


class BrowserSession:
    def __init__(self, playwright: Playwright, browser: Browser, context: BrowserContext, page: Page):
        self.playwright = playwright
        self.browser = browser
        self.context = context
        self.page = page

    async def close(self) -> None:
        await self.context.close()
        await self.browser.close()
        await self.playwright.stop()


_BROWSER_SESSIONS: dict[str, BrowserSession] = {}


def _normalize_url(url: str) -> str:
    candidate = url.strip()
    if candidate.startswith(("http://", "https://")):
        return candidate
    return f"https://{candidate}"


async def _get_session(context: ToolContext) -> BrowserSession:
    existing = _BROWSER_SESSIONS.get(context.session_id)
    if existing:
        return existing

    playwright = await async_playwright().start()
    browser = await playwright.chromium.launch(headless=context.settings.playwright_headless)
    browser_context = await browser.new_context()
    page = await browser_context.new_page()
    session = BrowserSession(playwright=playwright, browser=browser, context=browser_context, page=page)
    _BROWSER_SESSIONS[context.session_id] = session
    return session


async def browser_handler(context: ToolContext, args: BrowserArgs) -> dict:
    if args.operation == "close":
        session = _BROWSER_SESSIONS.pop(context.session_id, None)
        if session:
            await session.close()
        return {"closed": True}

    session = await _get_session(context)
    page = session.page

    if args.operation == "launch":
        if args.url:
            await page.goto(_normalize_url(args.url), timeout=args.timeout_ms)
        return {"url": page.url, "title": await page.title()}

    if args.operation == "navigate":
        if not args.url:
            raise ValueError("url is required for navigate")
        await page.goto(_normalize_url(args.url), timeout=args.timeout_ms)
        return {"url": page.url, "title": await page.title()}

    if args.operation == "click":
        if not args.selector:
            raise ValueError("selector is required for click")
        await page.locator(args.selector).first.click(timeout=args.timeout_ms)
        return {"clicked": args.selector, "url": page.url}

    if args.operation == "fill":
        if not args.selector or args.text is None:
            raise ValueError("selector and text are required for fill")
        await page.locator(args.selector).first.fill(args.text, timeout=args.timeout_ms)
        return {"filled": args.selector, "value": args.text}

    if args.operation == "press":
        if not args.key:
            raise ValueError("key is required for press")
        await page.keyboard.press(args.key)
        return {"pressed": args.key}

    if args.operation == "extract_text":
        if args.selector:
            content = await page.locator(args.selector).first.inner_text(timeout=args.timeout_ms)
        else:
            content = await page.locator("body").inner_text(timeout=args.timeout_ms)
        return {"url": page.url, "title": await page.title(), "content": content[:12000]}

    if args.operation == "screenshot":
        screenshot_dir = Path(context.settings.workspace_root) / "tmp" / "screenshots"
        screenshot_dir.mkdir(parents=True, exist_ok=True)
        target = screenshot_dir / f"{context.session_id}.png"
        await page.screenshot(path=str(target), full_page=True)
        return {"path": str(target), "url": page.url}

    raise ValueError(f"Unsupported browser operation: {args.operation}")


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="browser_tool",
        description="Control a browser with Playwright for web navigation, extraction, and multi-step web actions.",
        args_model=BrowserArgs,
        handler=browser_handler,
        supports_parallel=False,
        side_effecting=True,
    )
