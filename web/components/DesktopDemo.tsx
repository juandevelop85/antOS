"use client";

import Image from "next/image";
import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import {
  AGENTS,
  LANES,
  SCENARIOS,
  escapeHtml,
  pickScenario,
  type AgentKey,
  type CardMove,
  type Lane,
  type ScenarioKey,
} from "@/lib/scenarios";

interface Card {
  id: string;
  lane: Lane;
  who: AgentKey;
  title: string;
}

const INITIAL_LOG = [
  '<span class="muted"># Super + A · tablero de agentes</span>',
  '<span class="muted"># escribe una intención o elige una sugerencia ↑</span>',
];

const CHIPS: { key: ScenarioKey; label: string }[] = [
  { key: "fix", label: SCENARIOS.fix.text },
  { key: "secret", label: SCENARIOS.secret.text },
  { key: "db", label: SCENARIOS.db.text },
];

export default function DesktopDemo() {
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [cards, setCards] = useState<Card[]>([]);
  const [log, setLog] = useState<string[]>(INITIAL_LOG);
  const [toast, setToast] = useState<[string, string] | null>(null);
  const [clock, setClock] = useState("--:--");
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);
  const logRef = useRef<HTMLPreElement>(null);

  const later = (ms: number) =>
    new Promise<void>((resolve) => {
      timers.current.push(setTimeout(resolve, ms));
    });

  const clearTimers = () => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
  };

  const placeCard = ([id, lane, who, title]: CardMove) =>
    setCards((prev) => [...prev.filter((c) => c.id !== id), { id, lane, who, title }]);

  const run = useCallback(async (key: ScenarioKey, typed?: string) => {
    const sc = SCENARIOS[key];
    clearTimers();
    setCards([]);
    setToast(null);
    setRunning(true);
    const text = typed ?? sc.text;
    setInput(text);
    setLog([`<span class="p">❯</span> ${escapeHtml(text)}`]);
    for (const [ms, line, move] of sc.steps) {
      await later(ms);
      if (line) setLog((prev) => [...prev, line]);
      if (move) placeCard(move);
    }
    await later(500);
    setToast(sc.toast);
    setRunning(false);
  }, []);

  // Reloj de la barra y arranque suave para que el escritorio no esté vacío.
  useEffect(() => {
    const tick = () =>
      setClock(new Date().toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" }));
    tick();
    const interval = setInterval(tick, 30_000);
    const autostart = matchMedia("(prefers-reduced-motion: reduce)").matches
      ? undefined
      : setTimeout(() => void run("fix"), 1200);
    return () => {
      clearInterval(interval);
      if (autostart) clearTimeout(autostart);
      clearTimers();
    };
  }, [run]);

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log]);

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const text = input.trim();
    if (text && !running) void run(pickScenario(text), text);
  };

  const approve = () => {
    setToast(null);
    setCards((prev) => prev.map((c) => ({ ...c, lane: "done" })));
    setLog((prev) => [...prev, '<span class="ok">✓ aprobado</span> · fusionado en main · worktrees limpiados']);
  };

  const undo = () => {
    setToast(null);
    setCards([]);
    setLog((prev) => [
      ...prev,
      '<span class="warn">↺ antos undo</span> · restaurada la instantánea previa · 0 cambios en disco',
    ]);
  };

  return (
    <div className="desk" aria-label="Demostración interactiva del escritorio de antOS">
      <div className="desk-top">
        <Image src="/assets/icon.webp" alt="" width={20} height={20} />
        <span className="desk-badge">⎇ main · 2 worktrees</span>
        <span className="desk-badge">⬡ 313 tests</span>
        <span className="desk-clock">{clock}</span>
      </div>

      <form className="intent" autoComplete="off" onSubmit={onSubmit}>
        <span className="intent-glyph" aria-hidden="true">❯</span>
        <label className="sr-only" htmlFor="intent-input">Intención</label>
        <input
          id="intent-input"
          placeholder="¿Qué quieres hacer?"
          value={input}
          disabled={running}
          onChange={(e) => setInput(e.target.value)}
        />
        <kbd>↵</kbd>
      </form>
      <div className="chips">
        {CHIPS.map((chip) => (
          <button key={chip.key} type="button" disabled={running} onClick={() => void run(chip.key)}>
            {chip.label}
          </button>
        ))}
      </div>

      <div className="desk-body">
        <div className="board">
          {LANES.map((lane) => (
            <div className="col" key={lane.key}>
              <h4>{lane.label}</h4>
              <div className="lane">
                {cards
                  .filter((c) => c.lane === lane.key)
                  .map((c) => (
                    <div
                      className="card"
                      key={`${c.id}-${c.lane}`}
                      style={{ "--c": AGENTS[c.who].color } as React.CSSProperties}
                    >
                      <b>{AGENTS[c.who].name}</b>
                      {c.title}
                    </div>
                  ))}
              </div>
            </div>
          ))}
        </div>

        <pre className="log" ref={logRef} aria-live="polite">
          {log.map((line, i) => (
            <span key={i}>
              {i > 0 && "\n"}
              <span dangerouslySetInnerHTML={{ __html: line }} />
            </span>
          ))}
        </pre>
      </div>

      {toast && (
        <div className="toast">
          <div className="toast-body">
            <strong>{toast[0]}</strong>
            <p>{toast[1]}</p>
          </div>
          <div className="toast-actions">
            <button type="button" className="btn btn-primary btn-sm" onClick={approve}>Aprobar</button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={undo}>antos undo</button>
          </div>
        </div>
      )}
    </div>
  );
}
