from __future__ import annotations

from typing import Literal, Optional

from googleapiclient.discovery import build
from pydantic import Field

from app.integrations.google_workspace import load_google_credentials
from app.tools.base import ToolArgs, ToolContext, ToolDefinition

CALENDAR_SCOPES = ["https://www.googleapis.com/auth/calendar"]


class CalendarArgs(ToolArgs):
    action: Literal["list_events", "create_event"]
    start_iso: Optional[str] = Field(default=None, description="Start time in ISO-8601 format.")
    end_iso: Optional[str] = Field(default=None, description="End time in ISO-8601 format.")
    title: Optional[str] = None
    description: Optional[str] = None
    location: Optional[str] = None
    max_results: int = Field(default=10, ge=1, le=50)


async def calendar_handler(context: ToolContext, args: CalendarArgs) -> dict:
    creds = load_google_credentials(context.settings, CALENDAR_SCOPES)
    service = build("calendar", "v3", credentials=creds, cache_discovery=False)
    calendar_id = context.settings.google_calendar_id

    if args.action == "list_events":
        if not args.start_iso:
            raise ValueError("start_iso is required for list_events")
        events = (
            service.events()
            .list(
                calendarId=calendar_id,
                timeMin=args.start_iso,
                timeMax=args.end_iso,
                maxResults=args.max_results,
                singleEvents=True,
                orderBy="startTime",
            )
            .execute()
        )
        return {"events": events.get("items", [])}

    if not all([args.start_iso, args.end_iso, args.title]):
        raise ValueError("title, start_iso, and end_iso are required for create_event")

    event = {
        "summary": args.title,
        "description": args.description,
        "location": args.location,
        "start": {"dateTime": args.start_iso},
        "end": {"dateTime": args.end_iso},
    }
    created = service.events().insert(calendarId=calendar_id, body=event).execute()
    return {"id": created["id"], "htmlLink": created.get("htmlLink")}


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="calendar_tool",
        description="Read from or write to the user's calendar using Google Calendar.",
        args_model=CalendarArgs,
        handler=calendar_handler,
        supports_parallel=False,
        side_effecting=True,
        requires_confirmation=True,
    )
