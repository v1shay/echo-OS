# Jarvis Voice OS

Voice-first desktop agent for macOS with:

- local Whisper STT via `whisper-cli`
- browser control through a Playwright sidecar
- screen observation through screenshot + Vision OCR
- grounded desktop actions via `pyautogui`
- planner/executor/verifier loop with deterministic demo-safe planning and optional local Qwen intent help

## Quickstart

Use the real repo root:

```bash
cd /Users/agarwal/coding/jarvis/jarvis
./scripts/bootstrap_macos.sh
./scripts/run_jarvis.sh run --voice
```

For a text-triggered command:

```bash
./scripts/run_jarvis.sh run --text "Search YouTube for NBA highlights"
```

For continuous voice capture:

```bash
./scripts/run_jarvis.sh voice-loop
```

To run the local required test suite:

```bash
./scripts/doctor.sh
```

## Architecture

```text
Microphone / text input
  -> Whisper STT
  -> Intent resolver (heuristics first, local LLM fallback)
  -> Planner
  -> Observe screen/browser
  -> Execute tool
  -> Re-observe
  -> Verify
  -> TTS response

Tools:
  open_app
  browser_attach_or_launch
  browser_open
  browser_assert
  press_hotkey
  type_text
  read_screen
```

## Notes

- `bootstrap_macos.sh` creates `.venv`, installs Python and browser sidecar dependencies, compiles the OCR helper, and downloads local models if needed.
- Grant Accessibility, Screen Recording, Automation, and Microphone permissions when macOS prompts.
- Google Chrome is the default browser target. Set `JARVIS_CHROME_EXECUTABLE` if Chrome lives somewhere else.
- If `ELEVENLABS_API_KEY` and `ELEVENLABS_VOICE_ID` are set, Jarvis will use ElevenLabs TTS; otherwise it falls back to macOS `say`.
