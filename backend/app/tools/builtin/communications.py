from __future__ import annotations

import subprocess
from typing import Literal, Optional

from pydantic import Field
from twilio.rest import Client

from app.tools.base import ToolArgs, ToolContext, ToolDefinition


class CommunicationsArgs(ToolArgs):
    channel: Literal["sms", "whatsapp", "imessage"]
    recipient: str = Field(description="Phone number or iMessage handle.")
    message: str = Field(description="Message body to send.")
    action: Literal["send", "preview"] = "send"


AMBIGUOUS_RECIPIENTS = {
    "friend",
    "my friend",
    "professor",
    "my professor",
    "mom",
    "my mom",
    "dad",
    "my dad",
    "brother",
    "my brother",
    "sister",
    "my sister",
}


async def communications_handler(context: ToolContext, args: CommunicationsArgs) -> dict:
    if args.recipient.strip().lower() in AMBIGUOUS_RECIPIENTS:
        raise ValueError("Recipient is ambiguous. Ask the user for an exact phone number or iMessage handle.")

    if args.action == "preview":
        return {
            "channel": args.channel,
            "recipient": args.recipient,
            "message": args.message,
            "preview": True,
        }

    if args.channel in {"sms", "whatsapp"}:
        if not context.settings.twilio_account_sid or not context.settings.twilio_auth_token:
            raise RuntimeError("Twilio credentials are not configured.")
        client = Client(
            context.settings.twilio_account_sid,
            context.settings.twilio_auth_token.get_secret_value(),
        )
        from_number = context.settings.twilio_sms_from
        to_number = args.recipient
        if args.channel == "whatsapp":
            if not context.settings.twilio_whatsapp_from:
                raise RuntimeError("TWILIO_WHATSAPP_FROM is not configured.")
            from_number = f"whatsapp:{context.settings.twilio_whatsapp_from}"
            to_number = f"whatsapp:{args.recipient}"
        if not from_number:
            raise RuntimeError("Twilio sender number is not configured.")
        message = client.messages.create(body=args.message, from_=from_number, to=to_number)
        return {"channel": args.channel, "sid": message.sid, "status": message.status}

    applescript = f"""
    tell application "Messages"
      set targetService to 1st service whose service type = iMessage
      set targetBuddy to buddy "{args.recipient}" of targetService
      send "{args.message.replace('"', '\\"')}" to targetBuddy
    end tell
    """
    result = subprocess.run(["osascript", "-e", applescript], capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "Failed to send iMessage.")
    return {"channel": "imessage", "recipient": args.recipient, "sent": True}


def build_tool() -> ToolDefinition:
    return ToolDefinition(
        name="communications_tool",
        description="Send SMS, WhatsApp, or iMessage messages through the configured provider.",
        args_model=CommunicationsArgs,
        handler=communications_handler,
        supports_parallel=False,
        side_effecting=True,
        requires_confirmation=True,
    )
