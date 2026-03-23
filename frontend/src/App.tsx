import { useEchoSocket } from "./hooks/useEchoSocket";
import { FormEvent, useState } from "react";

const stateCopy = {
  idle: "Speak to begin",
  listening: "Listening",
  thinking: "Thinking",
  executing: "Executing",
  speaking: "Speaking",
} as const;

const brailleLogo = [
  "⠑⠉⠓⠕",
  "⠕⠎",
];

export default function App() {
  const session = useEchoSocket();
  const [command, setCommand] = useState("");

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (session.sendText(command)) {
      setCommand("");
    }
  };

  return (
    <main className="min-h-screen bg-canvas text-ink">
      <div className="relative flex min-h-screen flex-col items-center justify-center overflow-hidden px-6 py-10">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_top,rgba(232,245,233,0.08),transparent_28%),radial-gradient(circle_at_bottom,rgba(102,151,137,0.15),transparent_34%),linear-gradient(180deg,#010202,#020606_45%,#08110f)]" />
        <div className="absolute inset-0 bg-[linear-gradient(rgba(255,255,255,0.025)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.025)_1px,transparent_1px)] bg-[size:28px_28px] opacity-20" />

        <section className="relative z-10 flex w-full max-w-6xl flex-col items-center justify-center gap-8 text-center">
          <div className="space-y-4">
            <p className="font-body text-[11px] uppercase tracking-[0.72em] text-muted">Echo OS Demo Mode</p>
            <div className="rounded-[2.5rem] border border-white/10 bg-white/[0.03] px-6 py-8 shadow-ambient backdrop-blur-sm md:px-12 md:py-10">
              <div className="space-y-2">
                {brailleLogo.map((line) => (
                  <p key={line} className="font-display text-[4.25rem] leading-none tracking-[0.35em] text-ink md:text-[7.5rem]">
                    {line}
                  </p>
                ))}
              </div>
              <p className="mt-6 font-display text-2xl tracking-[0.65em] text-pulse md:text-4xl">echo_os</p>
            </div>
          </div>

          <div className="w-full max-w-3xl rounded-[1.75rem] border border-white/10 bg-black/20 px-6 py-5 backdrop-blur-sm">
            <div className="flex flex-wrap items-center justify-center gap-3 text-[11px] uppercase tracking-[0.34em] text-muted">
              <span>{stateCopy[session.state]}</span>
              <span>{session.connected ? "Linked" : "Offline"}</span>
              <span>{session.sttProvider}</span>
              {session.tools.length > 0 ? <span>{session.tools.join(" · ")}</span> : null}
            </div>
            <p className="mt-4 min-h-14 font-body text-lg leading-relaxed text-ink/92 md:text-xl">
              {session.transcript || session.assistantText || "Echo is standing by."}
            </p>
            <form onSubmit={handleSubmit} className="mt-5 flex flex-col gap-3 md:flex-row">
              <input
                value={command}
                onChange={(event) => setCommand(event.target.value)}
                placeholder="Type a demo command like: echo hello"
                className="flex-1 rounded-2xl border border-white/10 bg-white/[0.06] px-4 py-3 font-body text-base text-ink outline-none placeholder:text-muted"
              />
              <button
                type="submit"
                className="rounded-2xl border border-pulse/30 bg-pulse/15 px-5 py-3 font-body text-sm uppercase tracking-[0.28em] text-pulse"
              >
                Send
              </button>
            </form>
          </div>
        </section>
      </div>
    </main>
  );
}
