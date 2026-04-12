# FormIQ — AI Workout Coach

## What we're building
A web app that uses the phone/laptop camera to coach users through a full workout session:
- Warmup phase with quality scoring and a gate before the main workout
- Live form analysis via MediaPipe pose estimation + Claude Vision (frame sampling every 2–3s)
- Real-time injury risk score (0–100) displayed as a live meter
- Rep counter via pose landmark velocity tracking
- Post-workout cooldown with timed stretches personalized to the session
- Daily recovery logger (sleep, soreness, energy, RPE)
- Friend connections and streaks (social layer)

## Stack
- React + TypeScript + Vite + Tailwind CSS
- Firebase Auth (email/password + Google) + Firestore
- MediaPipe Pose (browser, via CDN or npm)
- Anthropic Claude Vision API (proxied via Vercel serverless function)
- Vercel deployment (auto-deploy on git push)

## Key constraints
- HTTPS required for camera access — local dev uses Vite's HTTPS plugin or localhost exception
- Anthropic API key lives ONLY in Vercel server env (never VITE_ prefixed) — frontend calls /api/analyze
- MediaPipe runs continuously at 30fps; we sample 3 best frames every 2.5s for Claude
- Prefer working code over perfect code — this is a hackathon
- Optimistic UI: update state immediately, sync Firestore in background

## Folder structure (enforce this)
src/
  components/       # Reusable UI components
  pages/            # Route-level page components
  hooks/            # Custom React hooks
  lib/              # Firebase, API clients, utilities
  types/            # TypeScript interfaces (single source of truth)
  stores/           # Zustand global state
  assets/           # Static assets

api/                # Vercel serverless functions (NOT inside src/)

## People/roles (do not cross-pollinate files without asking)
- Person A owns: hooks/usePoseDetection, hooks/useRepCounter, lib/formAnalysis, api/analyze
- Person B owns: pages/, components/, stores/, lib/firebase