from __future__ import annotations

from pathlib import Path
from typing import Sequence

from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow

from app.config import Settings


def load_google_credentials(settings: Settings, scopes: Sequence[str]) -> Credentials:
    if not settings.google_client_secret_path or not settings.google_token_path:
        raise RuntimeError("Google client secret path and token path must be configured.")

    creds = None
    token_path = Path(settings.google_token_path)
    client_secret_path = Path(settings.google_client_secret_path)

    if token_path.exists():
        creds = Credentials.from_authorized_user_file(str(token_path), scopes=scopes)

    if creds and creds.valid:
        return creds

    if creds and creds.expired and creds.refresh_token:
        creds.refresh(Request())
    else:
        flow = InstalledAppFlow.from_client_secrets_file(str(client_secret_path), scopes=scopes)
        creds = flow.run_local_server(port=0)

    token_path.write_text(creds.to_json(), encoding="utf-8")
    return creds
