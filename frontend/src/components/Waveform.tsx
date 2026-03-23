import { EchoState } from "../types";

interface WaveformProps {
  level: number;
  state: EchoState;
}

const BAR_COUNT = 24;

export function Waveform({ level, state }: WaveformProps) {
  return (
    <div className="flex h-24 items-end justify-center gap-1">
      {Array.from({ length: BAR_COUNT }, (_, index) => {
        const phase = (index + 1) / BAR_COUNT;
        const activityBoost = state === "listening" ? 1.2 : state === "speaking" ? 1 : 0.35;
        const height = Math.max(8, (level * 92 + phase * 40) * activityBoost);
        return (
          <span
            key={index}
            className="wave-bar block w-1 rounded-full bg-pulse"
            style={{
              height,
              opacity: state === "idle" ? 0.18 : 0.35 + phase * 0.4,
              animationDelay: `${index * 60}ms`,
            }}
          />
        );
      })}
    </div>
  );
}
