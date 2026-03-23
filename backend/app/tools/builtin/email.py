from __future__ import annotations

import base64
from email.message import EmailMessage
from typing import Literal, Optional

from googleapiclient.discovery import build
from pydantic import Field

from app.integrations.google_workspace import load_google_credentials
from app.tools.base import ToolArgs, ToolContext, ToolDefinition

GMAIL_SCOPES = ["https://www.googleapis.com/auth/gmail.send"]


class EmailArgs(ToolArgs):
    action: Literal["send", "draft"]
    to: str = Field(description="Recipient email address.")
    subject: str = Field(description="Email subject line.")
    body: str = Field(description="Plain text email body.")
    cc: Optional[str] = None


async def email_handler(context: ToolContext, args: EmailArgs) -> dict:
    creds = load_google_credentials(context.settings, GMAIL_SCOPES)
    service = build("gmail", "v1", credentials=creds, cache_discovery=False)

    sender = context.settings.gmail_sender
    if not sender:
        raise RuntimeError("GMAIL_SENDER is not configured.")

    message = EmailMessage()
    message["To"] = args.to
    message["From"] = sender
    message["Subject"] = args.subject
    if args.cc:
        message["Cc"] = args.cc
    message.set_content(args.body)

    encoded = base64.urlsafe_b64encode(message.as_bytes()).decode("utf-8")
    body = {"raw": encoded}

    if args.action == "draft":
        draft = service.users().drafts().create(userId="me", body={"message": body}).execute()
        return {"action": "draft", "id": draft["id"]}

    sent = service.users().messages().send(userId="me", body=body).execute()
    return {"action": "send", "id": sent["id"]}


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="email_tool",
        description="Create or send email through Gmail using the authenticated Google account.",
        args_model=EmailArgs,
        handler=email_handler,
        supports_parallel=False,
        side_effecting=True,
        requires_confirmation=True,
    )
