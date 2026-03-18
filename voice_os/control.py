from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path
from urllib.parse import quote

import pyautogui

from .config import JarvisConfig

pyautogui.FAILSAFE = True
pyautogui.PAUSE = 0.1


class GuiController:
    def __init__(self, config: JarvisConfig) -> None:
        self.config = config

    def _run_osascript(self, script: str, check: bool = True) -> str:
        completed = subprocess.run(
            ["osascript", "-e", script],
            check=check,
            capture_output=True,
            text=True,
        )
        return completed.stdout.strip()

    def open_app(self, app_name: str) -> None:
        for _ in range(4):
            try:
                self.activate_app(app_name)
            except Exception:
                subprocess.run(["open", "-a", app_name], check=True)
            if self.frontmost_app() and app_name.lower() in self.frontmost_app().lower():
                time.sleep(self.config.settle_delay_seconds)
                return
            time.sleep(0.4)
        subprocess.run(["open", "-a", app_name], check=True)
        self.activate_app(app_name)
        time.sleep(self.config.settle_delay_seconds)

    def activate_app(self, app_name: str) -> None:
        script = f'tell application "{app_name}" to activate'
        self._run_osascript(script)
        time.sleep(self.config.action_delay_seconds)

    def hide_app(self, app_name: str) -> None:
        script = f'tell application "{app_name}" to set visible to false'
        self._run_osascript(script, check=False)
        time.sleep(self.config.action_delay_seconds)

    def frontmost_app(self) -> str | None:
        script = (
            'tell application "System Events" to get name of first application process '
            "whose frontmost is true"
        )
        try:
            return self._run_osascript(script) or None
        except Exception:
            return None

    def chrome_open_url(self, url: str) -> dict[str, str]:
        escaped_url = url.replace("\\", "\\\\").replace('"', '\\"')
        script = f'''
        tell application "Google Chrome"
            activate
            if (count of windows) = 0 then
                make new window
            end if
            set URL of active tab of front window to "{escaped_url}"
            delay 2
            return "{{\\"title\\":\\"" & (title of active tab of front window) & "\\",\\"url\\":\\"" & (URL of active tab of front window) & "\\"}}"
        end tell
        '''
        raw = self._run_osascript(script)
        time.sleep(self.config.settle_delay_seconds)
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"title": "", "url": url}

    def chrome_active_tab(self) -> dict[str, str]:
        script = '''
        tell application "Google Chrome"
            if (count of windows) = 0 then
                return "{\\"title\\":\\"\\",\\"url\\":\\"\\"}"
            end if
            return "{\\"title\\":\\"" & (title of active tab of front window) & "\\",\\"url\\":\\"" & (URL of active tab of front window) & "\\"}"
        end tell
        '''
        raw = self._run_osascript(script, check=False)
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"title": "", "url": ""}

    def click_point(self, x: int, y: int) -> None:
        pyautogui.moveTo(x, y, duration=0.15)
        pyautogui.click()
        time.sleep(self.config.action_delay_seconds)

    def hotkey(self, *keys: str) -> None:
        try:
            pyautogui.hotkey(*keys)
        except Exception:
            modifiers = []
            mapping = {"command": "command down", "shift": "shift down", "option": "option down"}
            for key in keys[:-1]:
                if key in mapping:
                    modifiers.append(mapping[key])
            final_key = keys[-1]
            script = f'tell application "System Events" to keystroke "{final_key}" using {{{", ".join(modifiers)}}}'
            subprocess.run(["osascript", "-e", script], check=True)
        time.sleep(self.config.action_delay_seconds)

    def type_text(self, text: str) -> None:
        try:
            pyautogui.write(text, interval=0.02)
        except Exception:
            subprocess.run(["pbcopy"], input=text.encode("utf-8"), check=True)
            script = (
                'tell application "System Events" to keystroke "v" using {command down}'
            )
            subprocess.run(["osascript", "-e", script], check=True)
        time.sleep(self.config.action_delay_seconds)

    def new_textedit_document(self) -> None:
        script = """
        tell application "TextEdit"
            activate
            make new document
        end tell
        """
        subprocess.run(
            ["osascript", "-e", script],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(self.config.settle_delay_seconds)

    def set_textedit_text(self, text: str) -> str:
        escaped = text.replace("\\", "\\\\").replace('"', '\\"')
        script = f'''
        tell application "TextEdit"
            activate
            if not (exists document 1) then
                make new document
            end if
            set text of front document to "{escaped}"
            return text of front document
        end tell
        '''
        output = subprocess.check_output(
            ["osascript", "-e", script],
            text=True,
        ).strip()
        time.sleep(self.config.settle_delay_seconds)
        return output

    def copy_frontmost_text(self) -> str | None:
        original_clipboard = None
        try:
            original_clipboard = subprocess.check_output(["pbpaste"])
        except Exception:
            original_clipboard = None

        self.hotkey("command", "a")
        self.hotkey("command", "c")
        time.sleep(0.2)
        try:
            copied = subprocess.check_output(["pbpaste"], text=True)
        except Exception:
            copied = None

        if original_clipboard is not None:
            try:
                subprocess.run(["pbcopy"], input=original_clipboard, check=True)
            except Exception:
                pass
        return copied

    def resolve_contact(self, query: str) -> dict[str, str] | None:
        escaped_query = query.replace("\\", "\\\\").replace('"', '\\"')
        script = f'''
        tell application "Contacts"
            set matches to every person whose name contains "{escaped_query}"
            if (count of matches) = 0 then
                return ""
            end if
            set targetPerson to item 1 of matches
            set emailValue to ""
            try
                set emailValue to value of first email of targetPerson
            end try
            set phoneValue to ""
            try
                set phoneValue to value of first phone of targetPerson
            end try
            return "{{\\"name\\":\\"" & (name of targetPerson) & "\\",\\"email\\":\\"" & emailValue & "\\",\\"phone\\":\\"" & phoneValue & "\\"}}"
        end tell
        '''
        raw = self._run_osascript(script, check=False)
        if not raw:
            return None
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            return None
        if not any(data.values()):
            return None
        return data

    def gmail_compose_url(
        self,
        to_email: str,
        subject: str,
        body: str,
        account_hint: str | None = None,
    ) -> str:
        account_index = "0" if (account_hint or "").lower() != "schoology" else "1"
        return (
            f"https://mail.google.com/mail/u/{account_index}/?view=cm&fs=1"
            f"&to={quote(to_email)}&su={quote(subject)}&body={quote(body)}"
        )

    def send_imessage(self, handle: str, text: str) -> dict[str, str]:
        escaped_handle = handle.replace("\\", "\\\\").replace('"', '\\"')
        escaped_text = text.replace("\\", "\\\\").replace('"', '\\"')
        script = f'''
        tell application "Messages"
            set targetService to 1st service whose service type = iMessage
            set targetBuddy to buddy "{escaped_handle}" of targetService
            send "{escaped_text}" to targetBuddy
            return "{{\\"handle\\":\\"" & (handle of targetBuddy) & "\\",\\"name\\":\\"" & (full name of targetBuddy) & "\\"}}"
        end tell
        '''
        raw = self._run_osascript(script)
        time.sleep(self.config.settle_delay_seconds)
        return json.loads(raw)

    def spotify_open_search(self, query: str) -> None:
        subprocess.run([ "open", f"spotify:search:{quote(query)}"], check=True)
        self.activate_app("Spotify")
        time.sleep(self.config.settle_delay_seconds)

    def spotify_player_state(self) -> dict[str, str]:
        script = '''
        tell application "Spotify"
            set currentState to player state as text
            set currentName to ""
            set currentArtist to ""
            try
                set currentName to name of current track
                set currentArtist to artist of current track
            end try
            return "{\\"state\\":\\"" & currentState & "\\",\\"track\\":\\"" & currentName & "\\",\\"artist\\":\\"" & currentArtist & "\\"}"
        end tell
        '''
        raw = self._run_osascript(script, check=False)
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"state": "", "track": "", "artist": ""}

    def spotify_play(self) -> dict[str, str]:
        script = '''
        tell application "Spotify"
            play
            delay 2
            set currentState to player state as text
            set currentName to ""
            set currentArtist to ""
            try
                set currentName to name of current track
                set currentArtist to artist of current track
            end try
            return "{\\"state\\":\\"" & currentState & "\\",\\"track\\":\\"" & currentName & "\\",\\"artist\\":\\"" & currentArtist & "\\"}"
        end tell
        '''
        raw = self._run_osascript(script, check=False)
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"state": "", "track": "", "artist": ""}
