export type EchoState = "idle" | "listening" | "thinking" | "executing" | "speaking";

export interface VoiceEvent {
  type: string;
  state?: EchoState;
  text?: string;
  response?: string;
  audio_base64?: string;
  tools?: string[];
  provider?: string;
  message?: string;
}
