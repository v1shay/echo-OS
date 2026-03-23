from app.tools.registry import ToolRegistry
from app.tools.builtin import (
    app_control,
    browser,
    calendar,
    code_analysis,
    communications,
    macos_automation,
    macos_ui,
    desktop,
    email,
    filesystem,
    terminal,
)


def build_registry(settings) -> ToolRegistry:
    registry = ToolRegistry(settings)
    for builder in [
        filesystem.build_tool,
        terminal.build_tool,
        code_analysis.build_tool,
        app_control.build_tool,
        browser.build_tool,
        communications.build_tool,
        email.build_tool,
        calendar.build_tool,
        desktop.build_tool,
        macos_automation.build_tool,
        macos_ui.build_tool,
    ]:
        registry.register(builder())
    return registry
