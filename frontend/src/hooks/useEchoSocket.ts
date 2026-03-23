import { useEffect, useMemo, useRef, useState } from "react";
import type { EchoState, VoiceEvent } from "../types";

type BrowserSpeechRecognitionCtor = BrowserSpeechRecognitionConstructor;

interface EchoSessionState {
  state: EchoState;
  transcript: string;
  assistantText: string;
  level: number;
  tools: string[];
  connected: boolean;
  sttProvider: string;
}

const initialState: EchoSessionState = {
  state: "idle",
  transcript: "",
  assistantText: "",
  level: 0,
  tools: [],
  connected: false,
  sttProvider: "connecting",
};

function floatTo16BitPCM(samples: Float32Array): Int16Array {
  const converted = new Int16Array(samples.length);
  for (let index = 0; index < samples.length; index += 1) {
    const sample = Math.max(-1, Math.min(1, samples[index]));
    converted[index] = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
  }
  return converted;
}

function toBase64(buffer: ArrayBufferLike): string {
  let binary = "";
  const bytes = new Uint8Array(buffer);
  for (let index = 0; index < bytes.byteLength; index += 1) {
    binary += String.fromCharCode(bytes[index]);
  }
  return window.btoa(binary);
}

export function useEchoSocket() {
  const [session, setSession] = useState<EchoSessionState>(initialState);
  const websocketRef = useRef<WebSocket | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const playbackAudioRef = useRef<HTMLAudioElement | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const sourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const monitorGainRef = useRef<GainNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const reconnectTimerRef = useRef<number | null>(null);
  const speechRecognitionRef = useRef<BrowserSpeechRecognition | null>(null);
  const pendingSpeechTextRef = useRef("");
  const speechDetectedRef = useRef(false);
  const lastSpeechAtRef = useRef(0);

  const isElectron = useMemo(() => navigator.userAgent.toLowerCase().includes("electron"), []);
  const sessionId = useMemo(() => {
    const existing = window.sessionStorage.getItem("echo_session_id");
    if (existing) {
      return existing;
    }
    const generated = window.crypto?.randomUUID?.() ?? `echo-${Date.now()}`;
    window.sessionStorage.setItem("echo_session_id", generated);
    return generated;
  }, []);

  const socketUrl = useMemo(() => {
    const configured = import.meta.env.VITE_BACKEND_WS_URL as string | undefined;
    return configured ?? `ws://127.0.0.1:8000/ws/voice/${sessionId}`;
  }, [sessionId]);

  useEffect(() => {
    let cancelled = false;

    const speakLocally = (text: string) => {
      const normalized = text.trim();
      if (!normalized || typeof window === "undefined" || !("speechSynthesis" in window)) {
        return;
      }

      window.speechSynthesis.cancel();
      const utterance = new SpeechSynthesisUtterance(normalized);
      utterance.rate = 1;
      utterance.pitch = 1;
      utterance.volume = 1;
      utterance.onend = () => {
        setSession((current) => ({ ...current, state: "idle" }));
      };
      utterance.onerror = () => {
        setSession((current) => ({ ...current, state: "idle" }));
      };
      window.speechSynthesis.speak(utterance);
      setSession((current) => ({ ...current, state: "speaking" }));
    };

    const playSynthesizedAudio = async (audioBase64: string) => {
      const binary = window.atob(audioBase64);
      const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
      const blob = new Blob([bytes], { type: "audio/mpeg" });
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);
      playbackAudioRef.current = audio;

      audio.onended = () => {
        URL.revokeObjectURL(url);
        playbackAudioRef.current = null;
        setSession((current) => ({ ...current, state: "idle" }));
      };

      audio.onerror = () => {
        URL.revokeObjectURL(url);
        playbackAudioRef.current = null;
        speakLocally(pendingSpeechTextRef.current);
      };

      try {
        await audio.play();
      } catch {
        URL.revokeObjectURL(url);
        playbackAudioRef.current = null;
        speakLocally(pendingSpeechTextRef.current);
      }
    };

    const connect = () => {
      if (cancelled) {
        return;
      }

      const websocket = new WebSocket(socketUrl);
      websocketRef.current = websocket;

      websocket.onopen = () => {
        setSession((current) => ({ ...current, connected: true }));
      };

      websocket.onmessage = async (event) => {
        const payload: VoiceEvent = JSON.parse(event.data);
        if (payload.type === "status" && payload.state) {
          setSession((current) => ({
            ...current,
            state: payload.state!,
            tools: payload.tools ?? current.tools,
          }));
        }
        if (payload.type === "stt_provider" && payload.provider) {
          setSession((current) => ({ ...current, sttProvider: payload.provider! }));
        }
        if (payload.type === "voice_error" && payload.message) {
          setSession((current) => ({
            ...current,
            assistantText: payload.message ?? current.assistantText,
            sttProvider: payload.provider ?? current.sttProvider,
          }));
        }
        if (payload.type === "transcript_partial" || payload.type === "transcript_committed") {
          setSession((current) => ({ ...current, transcript: payload.text ?? current.transcript }));
        }
        if (payload.type === "assistant_text") {
          setSession((current) => ({ ...current, assistantText: payload.text ?? current.assistantText }));
        }
        if (payload.type === "task_result") {
          pendingSpeechTextRef.current = payload.response ?? "";
          setSession((current) => ({ ...current, assistantText: payload.response ?? current.assistantText }));
        }
        if (payload.type === "tts_error") {
          speakLocally(pendingSpeechTextRef.current);
        }
        if (payload.type === "tts_complete" && payload.audio_base64) {
          await playSynthesizedAudio(payload.audio_base64);
        }
      };

      websocket.onclose = () => {
        setSession((current) => ({ ...current, connected: false }));
        if (!cancelled) {
          reconnectTimerRef.current = window.setTimeout(connect, 1500);
        }
      };
    };

    connect();

    return () => {
      cancelled = true;
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
      }
      playbackAudioRef.current?.pause();
      playbackAudioRef.current = null;
      if ("speechSynthesis" in window) {
        window.speechSynthesis.cancel();
      }
      websocketRef.current?.close();
    };
  }, [socketUrl]);

  const sendText = (text: string) => {
    const normalized = text.trim();
    if (!normalized) {
      return false;
    }
    const socket = websocketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      setSession((current) => ({
        ...current,
        assistantText: "Backend connection is offline. Start the backend and try again.",
      }));
      return false;
    }
    socket.send(JSON.stringify({ type: "text_input", text: normalized }));
    setSession((current) => ({
      ...current,
      transcript: normalized,
      state: "thinking",
    }));
    return true;
  };

  useEffect(() => {
    if (isElectron) {
      return;
    }

    const SpeechRecognitionCtor = (
      window.SpeechRecognition ??
      window.webkitSpeechRecognition
    ) as BrowserSpeechRecognitionCtor | undefined;

    if (!SpeechRecognitionCtor) {
      return;
    }

    let cancelled = false;
    const recognition = new SpeechRecognitionCtor();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = "en-US";
    speechRecognitionRef.current = recognition;

    recognition.onstart = () => {
      setSession((current) => ({ ...current, sttProvider: "browser_speech" }));
    };

    recognition.onresult = (event: any) => {
      const result = event.results[event.results.length - 1];
      if (!result) {
        return;
      }

      const transcript = result[0]?.transcript?.trim() ?? "";
      if (!transcript) {
        return;
      }

      setSession((current) => ({ ...current, transcript }));

      if (!result.isFinal) {
        return;
      }

      const lowered = transcript.toLowerCase();
      if (!lowered.startsWith("echo")) {
        return;
      }

      const text = transcript.slice(4).replace(/^[,\s:.-]+/, "").trim();
      if (!text) {
        setSession((current) => ({ ...current, state: "listening" }));
        return;
      }

      const socket = websocketRef.current;
      if (socket?.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type: "text_input", text }));
        setSession((current) => ({ ...current, state: "thinking" }));
      }
    };

    recognition.onerror = () => {
      setSession((current) => ({ ...current, sttProvider: "browser_speech_error" }));
    };

    recognition.onend = () => {
      if (!cancelled) {
        window.setTimeout(() => recognition.start(), 500);
      }
    };

    recognition.start();

    return () => {
      cancelled = true;
      recognition.stop();
    };
  }, [isElectron]);

  useEffect(() => {
    let cancelled = false;
    const hasBrowserSpeech = !isElectron && Boolean(
      window.SpeechRecognition ||
        window.webkitSpeechRecognition,
    );
    const speechThreshold = 0.015;
    const silenceCommitMs = 700;

    async function startCapture() {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      if (cancelled) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }

      const audioContext = new AudioContext();
      streamRef.current = stream;
      const source = audioContext.createMediaStreamSource(stream);
      const analyser = audioContext.createAnalyser();
      analyser.fftSize = 1024;
      const processor = audioContext.createScriptProcessor(4096, 1, 1);
      const monitorGain = audioContext.createGain();
      monitorGain.gain.value = 0;

      audioContextRef.current = audioContext;
      sourceRef.current = source;
      analyserRef.current = analyser;
      processorRef.current = processor;
      monitorGainRef.current = monitorGain;

      source.connect(analyser);
      analyser.connect(processor);
      processor.connect(monitorGain);
      monitorGain.connect(audioContext.destination);

      processor.onaudioprocess = (captureEvent) => {
        const channel = captureEvent.inputBuffer.getChannelData(0);
        const pcm = floatTo16BitPCM(channel);
        const rms = Math.sqrt(channel.reduce((sum, sample) => sum + sample * sample, 0) / channel.length);
        setSession((current) => ({ ...current, level: rms }));
        const socket = websocketRef.current;
        if (!socket || socket.readyState !== WebSocket.OPEN) {
          return;
        }
        if (!hasBrowserSpeech) {
          const now = performance.now();
          socket.send(
            JSON.stringify({
              type: "audio_chunk",
              sample_rate: audioContext.sampleRate,
              audio_base64: toBase64(pcm.buffer),
            }),
          );

          if (rms >= speechThreshold) {
            speechDetectedRef.current = true;
            lastSpeechAtRef.current = now;
          } else if (
            speechDetectedRef.current &&
            now - lastSpeechAtRef.current >= silenceCommitMs
          ) {
            speechDetectedRef.current = false;
            socket.send(JSON.stringify({ type: "audio_commit" }));
          }
        }
      };
    }

    startCapture().catch((error) => {
      const message = error instanceof Error ? error.message : "Microphone access failed.";
      setSession((current) => ({
        ...current,
        assistantText: `Microphone error: ${message}`,
        sttProvider: "microphone_error",
      }));
    });

    return () => {
      cancelled = true;
      processorRef.current?.disconnect();
      sourceRef.current?.disconnect();
      analyserRef.current?.disconnect();
      monitorGainRef.current?.disconnect();
      speechDetectedRef.current = false;
      streamRef.current?.getTracks().forEach((track) => track.stop());
      audioContextRef.current?.close().catch(() => undefined);
    };
  }, [isElectron]);

  return {
    ...session,
    sendText,
  };
}
