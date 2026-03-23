import { useEffect, useMemo, useState } from "react";
import { demoScenarios } from "../demoData";

const cycleMs = 7000;

export function DemoShowcase() {
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setActiveIndex((current) => (current + 1) % demoScenarios.length);
    }, cycleMs);
    return () => window.clearInterval(timer);
  }, []);

  const activeScenario = demoScenarios[activeIndex];
  const progress = useMemo(() => ((activeIndex + 1) / demoScenarios.length) * 100, [activeIndex]);

  return (
    <main className="relative min-h-screen overflow-hidden bg-canvas text-ink">
      <div
        className="absolute inset-0 opacity-90 transition-all duration-700"
        style={{
          background: `radial-gradient(circle at 15% 20%, ${activeScenario.palette.glow}, transparent 28%),
            radial-gradient(circle at 85% 18%, ${activeScenario.palette.wash}, transparent 24%),
            linear-gradient(135deg, rgba(4,4,6,0.98), rgba(10,10,14,0.94))`,
        }}
      />
      <div className="absolute inset-0 bg-[linear-gradient(rgba(255,255,255,0.03)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.03)_1px,transparent_1px)] bg-[size:44px_44px] opacity-20" />

      <section className="relative z-10 mx-auto flex min-h-screen max-w-7xl flex-col px-6 py-8 md:px-10">
        <header className="flex flex-col gap-5 border-b border-white/10 pb-6 md:flex-row md:items-end md:justify-between">
          <div className="space-y-3">
            <p className="font-display text-4xl tracking-[0.42em] md:text-6xl">echo_os</p>
            <div className="space-y-2">
              <p className="font-body text-[10px] uppercase tracking-[0.6em] text-muted">Autonomous Demo Suite</p>
              <p className="max-w-2xl font-body text-sm text-ink/70 md:text-base">
                Four cinematic walkthroughs showing how Echo listens, reasons, acts, and confirms completion
                across apps, communication, and file management.
              </p>
            </div>
          </div>
          <div className="w-full max-w-sm space-y-3">
            <div className="flex items-center justify-between text-[10px] uppercase tracking-[0.35em] text-muted">
              <span>Scenario Progress</span>
              <span>{String(activeIndex + 1).padStart(2, "0")}</span>
            </div>
            <div className="h-1.5 rounded-full bg-white/10">
              <div
                className="h-full rounded-full transition-all duration-700"
                style={{ width: `${progress}%`, backgroundColor: activeScenario.palette.accent }}
              />
            </div>
          </div>
        </header>

        <div className="grid flex-1 gap-8 py-8 lg:grid-cols-[1.2fr_0.8fr]">
          <div className="space-y-6">
            <div className="grid gap-3 md:grid-cols-4">
              {demoScenarios.map((scenario, index) => {
                const isActive = index === activeIndex;
                return (
                  <button
                    key={scenario.id}
                    type="button"
                    onClick={() => setActiveIndex(index)}
                    className={`rounded-[1.5rem] border px-4 py-4 text-left transition-all duration-300 ${
                      isActive
                        ? "border-white/25 bg-white/10 shadow-ambient"
                        : "border-white/8 bg-white/[0.03] hover:border-white/18 hover:bg-white/[0.05]"
                    }`}
                  >
                    <p className="font-body text-[10px] uppercase tracking-[0.34em] text-muted">{scenario.eyebrow}</p>
                    <p className="mt-3 font-body text-sm leading-snug text-ink">{scenario.title}</p>
                  </button>
                );
              })}
            </div>

            <article className="relative overflow-hidden rounded-[2rem] border border-white/10 bg-white/[0.045] p-6 shadow-ambient backdrop-blur md:p-8">
              <div
                className="absolute inset-x-0 top-0 h-1"
                style={{
                  background: `linear-gradient(90deg, transparent, ${activeScenario.palette.accent}, transparent)`,
                }}
              />
              <div className="grid gap-8 xl:grid-cols-[0.9fr_1.1fr]">
                <div className="space-y-6">
                  <div className="space-y-3">
                    <p className="font-body text-[11px] uppercase tracking-[0.44em] text-muted">
                      {activeScenario.eyebrow}
                    </p>
                    <h1 className="max-w-xl font-body text-3xl font-medium leading-tight md:text-5xl">
                      {activeScenario.title}
                    </h1>
                    <p className="max-w-xl font-body text-base leading-relaxed text-ink/72">
                      {activeScenario.summary}
                    </p>
                  </div>

                  <div className="rounded-[1.5rem] border border-white/10 bg-black/25 p-5">
                    <p className="font-body text-[10px] uppercase tracking-[0.34em] text-muted">Spoken Prompt</p>
                    <p className="mt-3 font-body text-lg leading-relaxed text-ink">{activeScenario.prompt}</p>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    {activeScenario.tools.map((tool) => (
                      <span
                        key={tool}
                        className="rounded-full border border-white/10 bg-white/[0.05] px-3 py-1.5 font-body text-[11px] uppercase tracking-[0.26em] text-ink/80"
                      >
                        {tool}
                      </span>
                    ))}
                  </div>

                  <div className="rounded-[1.5rem] border border-white/10 bg-white/[0.03] p-5">
                    <p className="font-body text-[10px] uppercase tracking-[0.34em] text-muted">Echo Response</p>
                    <p className="mt-3 font-body text-base leading-relaxed text-ink/88">{activeScenario.response}</p>
                  </div>
                </div>

                <div className="grid gap-4 md:grid-cols-3 xl:grid-cols-1">
                  {activeScenario.panels.map((panel) => (
                    <section
                      key={panel.title}
                      className="rounded-[1.6rem] border border-white/10 bg-black/30 p-5 transition-all duration-500"
                      style={{ boxShadow: `0 0 40px ${activeScenario.palette.glow}` }}
                    >
                      <div className="flex items-center justify-between">
                        <div>
                          <p className="font-body text-sm text-ink">{panel.title}</p>
                          <p className="mt-1 font-body text-[10px] uppercase tracking-[0.3em] text-muted">
                            {panel.subtitle}
                          </p>
                        </div>
                        <span
                          className="h-2.5 w-2.5 rounded-full"
                          style={{ backgroundColor: activeScenario.palette.accent }}
                        />
                      </div>
                      <div className="mt-5 space-y-3">
                        {panel.lines.map((line) => (
                          <div
                            key={line}
                            className="rounded-2xl border border-white/6 bg-white/[0.04] px-4 py-3 font-body text-sm text-ink/82"
                          >
                            {line}
                          </div>
                        ))}
                      </div>
                    </section>
                  ))}
                </div>
              </div>
            </article>
          </div>

          <aside className="space-y-5">
            <section className="rounded-[2rem] border border-white/10 bg-white/[0.045] p-6 backdrop-blur">
              <p className="font-body text-[10px] uppercase tracking-[0.34em] text-muted">Execution Arc</p>
              <div className="mt-5 space-y-4">
                {activeScenario.timeline.map((entry, index) => (
                  <div key={`${entry.phase}-${index}`} className="flex gap-4">
                    <div className="flex flex-col items-center">
                      <span
                        className="flex h-10 w-10 items-center justify-center rounded-full border border-white/10 font-body text-xs"
                        style={{ color: activeScenario.palette.accent }}
                      >
                        {String(index + 1).padStart(2, "0")}
                      </span>
                      {index < activeScenario.timeline.length - 1 ? (
                        <span className="mt-2 h-full w-px bg-white/10" />
                      ) : null}
                    </div>
                    <div className="pb-5">
                      <p className="font-body text-sm uppercase tracking-[0.24em] text-ink">{entry.phase}</p>
                      <p className="mt-2 font-body text-sm leading-relaxed text-ink/68">{entry.detail}</p>
                    </div>
                  </div>
                ))}
              </div>
            </section>

            <section className="overflow-hidden rounded-[2rem] border border-white/10 bg-white/[0.045] p-6 backdrop-blur">
              <div className="flex items-center justify-between">
                <div>
                  <p className="font-body text-[10px] uppercase tracking-[0.34em] text-muted">Live State</p>
                  <p className="mt-2 font-body text-xl text-ink">{activeScenario.status}</p>
                </div>
                <div
                  className="h-3 w-3 rounded-full"
                  style={{
                    backgroundColor: activeScenario.palette.accent,
                    boxShadow: `0 0 24px ${activeScenario.palette.accent}`,
                  }}
                />
              </div>
              <div className="mt-6 rounded-[1.6rem] border border-white/8 bg-black/30 p-4">
                <div className="flex gap-2">
                  {Array.from({ length: 24 }).map((_, index) => (
                    <span
                      key={index}
                      className="wave-bar h-16 flex-1 rounded-full"
                      style={{
                        background: `linear-gradient(180deg, ${activeScenario.palette.accent}, rgba(255,255,255,0.08))`,
                        animationDelay: `${index * 0.06}s`,
                      }}
                    />
                  ))}
                </div>
              </div>
              <p className="mt-5 font-body text-sm leading-relaxed text-ink/68">
                Demo mode is meant to show the product story: natural prompt, model planning, tool execution,
                and a visible completed outcome.
              </p>
            </section>
          </aside>
        </div>
      </section>
    </main>
  );
}
