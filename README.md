<p align="center">
  <img width="700" alt="echo-OS Demo" src="https://github.com/user-attachments/assets/4e9659b0-cb91-451d-9181-0962e7dbf343" />
</p>

<p align="center"><strong>A Voice-First Autonomous Operating System</strong></p>

<p align="center">
echo-OS is a voice-native system that executes real-world tasks through continuous autonomous agents.
</p>

---

## Overview

echo-OS replaces manual interaction with a voice-driven execution layer.

Users state intent in natural language.  
The system interprets, plans, executes, verifies, and responds continuously.


---

## Tech Stack

| Layer         | Technology          |
|--------------|-------------------|
| Frontend     | React + Tailwind  |
| Realtime     | WebSockets        |
| Backend      | Python            |
| Agent Model  | LLM APIs          |
| STT          | Whisper / APIs    |
| TTS          | ElevenLabs / APIs |
| Desktop      | Electron          |
| Orchestration| Custom Agent Loop |

---

## Core Flow

* ↓ Microphone Input

* ↓ Speech-to-Text (STT)

* ↓ Agent Reasoning Engine

* ↓ Execution Layer (Local + Web Tools)

* ↓ Verification Layer
  
* ↓ Memory System
  
* ↓ Text-to-Speech (TTS)

---

## Architecture

### Frontend (React + WebSockets)
- Real-time interface for transcripts, reasoning, and execution state  
- Streams updates via WebSockets  
- Designed to remain ambient and low-friction  

### Backend (Python Agent Engine)
- Handles intent parsing, planning, tool selection, and orchestration  
- Maintains persistent session memory  
- Runs a continuous agent loop (not request-response)

### Desktop Layer (Electron)
- Enables native system control (files, apps, OS automation)  
- Converts system into a true operating layer  

---

## Installation

### Clone

```bash
git clone https://github.com/v1shay/echo-os.git
cd echo-os

---

## Setup & Run

# clone
git clone https://github.com/your-username/echo-os.git
cd echo-os

# backend setup
cd backend
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
cd ..

# frontend setup
cd frontend
npm install
cd ..

# desktop setup
cd desktop
npm install
cd ..

# environment variables
export OPENAI_API_KEY=your_key_here
export ELEVENLABS_API_KEY=your_key_here

# run system (3 terminals)

# terminal 1 - backend
cd backend
uvicorn main:app --reload

# terminal 2 - frontend
cd frontend
npm run dev

# terminal 3 - desktop
cd desktop
npm run electron
