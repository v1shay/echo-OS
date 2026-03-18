from __future__ import annotations

from .schemas import Observation


class Verifier:
    def verify(self, observation: Observation, rules: dict, action_details: dict | None = None) -> tuple[bool, dict]:
        details: dict[str, object] = {}

        if "frontmost_app" in rules:
            expected = str(rules["frontmost_app"]).lower()
            actual = (observation.frontmost_app or "").lower()
            matched = expected in actual or actual in expected
            details["frontmost_app"] = {
                "expected": rules["frontmost_app"],
                "actual": observation.frontmost_app,
                "matched": matched,
            }
            if not matched:
                return False, details

        if "screen_text_contains" in rules:
            expected = str(rules["screen_text_contains"]).lower()
            haystack = observation.screen_text.lower()
            matched = expected in haystack
            details["screen_text_contains"] = {
                "expected": rules["screen_text_contains"],
                "matched": matched,
            }
            if not matched:
                return False, details

        if "captured_text_contains" in rules:
            captured_text = str((action_details or {}).get("captured_text", ""))
            expected = str(rules["captured_text_contains"]).lower()
            matched = expected in captured_text.lower()
            details["captured_text_contains"] = {
                "expected": rules["captured_text_contains"],
                "captured_text": captured_text,
                "matched": matched,
            }
            if not matched:
                return False, details

        if "textedit_document_contains" in rules:
            captured_text = str((action_details or {}).get("textedit_document_text", ""))
            expected = str(rules["textedit_document_contains"]).lower()
            matched = expected in captured_text.lower()
            details["textedit_document_contains"] = {
                "expected": rules["textedit_document_contains"],
                "captured_text": captured_text,
                "matched": matched,
            }
            if not matched:
                return False, details

        return True, details

    def verify_browser(self, browser_state: dict, rules: dict) -> tuple[bool, dict]:
        url = str(browser_state.get("url", ""))
        title = str(browser_state.get("title", ""))
        text = str(browser_state.get("visibleText", ""))
        details = {"url": url, "title": title}
        if "browser_ready" in rules and not browser_state.get("ready"):
            return False, {**details, "ready": browser_state.get("ready")}
        if "url_contains" in rules and str(rules["url_contains"]).lower() not in url.lower():
            return False, details
        if "title_contains" in rules and str(rules["title_contains"]).lower() not in title.lower():
            return False, details
        if "text_contains" in rules:
            expected = str(rules["text_contains"]).lower()
            combined = " ".join([url.lower(), title.lower(), text.lower()])
            tokens = [token for token in expected.split() if token]
            matched = expected in combined or (
                tokens and sum(token in combined for token in tokens) >= max(1, len(tokens) - 1)
            )
            details["text_contains"] = {
                "expected": rules["text_contains"],
                "matched": matched,
            }
            if not matched:
                return False, details
        return True, details
