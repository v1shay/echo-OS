<div align="center">

<img width="700" alt="echo-OS Demo" src="https://github.com/user-attachments/assets/4e9659b0-cb91-451d-9181-0962e7dbf343" />


---

**Echo-OS is a voice-first autonomous OS** that executes real-world tasks through continuous autonomous agents.

A real-time execution layer that runs on natural language input, built as the **winner of an ElevenLabs, FeatherlessAI, and AoPS-backed hackathon**.

</div>

---

## Features

- Voice → execution pipeline (not just transcription)
- Continuous agent loop (not request/response)
- Real-time reasoning + execution streaming
- Native OS control via Electron layer
- Tool use across local system and web
- Persistent memory across sessions
- End-to-end speech interface (STT + TTS)

---

## Architecture

| Layer | Purpose | Stack |
|---|---|---|
| Interface | Real-time transcripts + execution state | React + WebSockets |
| Agent | Planning, reasoning, orchestration | Python |
| Execution | Local + web task execution | Custom tool layer |
| Voice | Input + output speech pipeline | Whisper + ElevenLabs |
| Desktop | System-level control | Electron |

---

## Anatomy

```txt
echo-os/
├── frontend/        # realtime UI + streaming state
├── backend/         # agent loop + orchestration
├── desktop/         # electron system layer
├── agents/          # planning + execution logic
├── tools/           # local + web integrations
└── memory/          # session + persistent state
```

## Install

```bash
git clone https://github.com/your-username/echo-os.git
cd echo-os
```

## Backend

```bash
cd backend
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --reload

```
## Frontend

```bash
cd ../frontend
npm install
npm run dev
```

## Desktop

```bash
cd ../desktop
npm install
npm run electron
```
