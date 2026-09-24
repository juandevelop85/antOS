// Guiones de la demo del escritorio. Son fijos: no hablan con ningún antosd real.

export type AgentKey = "a" | "c" | "q" | "u";
export type Lane = "plan" | "run" | "qa" | "done";
export type ScenarioKey = "fix" | "secret" | "db";

export const AGENTS: Record<AgentKey, { name: string; color: string }> = {
  a: { name: "📐 Arquitecto", color: "var(--c-arch)" },
  c: { name: "💻 Coder", color: "var(--c-code)" },
  q: { name: "🧪 QA", color: "var(--c-qa)" },
  u: { name: "🛡️ Auditor", color: "var(--c-audit)" },
};

export const LANES: { key: Lane; label: string }[] = [
  { key: "plan", label: "Plan" },
  { key: "run", label: "En curso" },
  { key: "qa", label: "QA" },
  { key: "done", label: "Hecho" },
];

/** Movimiento de una tarjeta: [id, carril, agente, título]. */
export type CardMove = [id: string, lane: Lane, who: AgentKey, title: string];

/** Un paso: retardo en ms, línea de log (HTML fijo de este fichero) y movimiento opcional. */
export type Step = [delay: number, line: string | null, move?: CardMove];

export interface Scenario {
  text: string;
  steps: Step[];
  toast: [title: string, detail: string];
}

export const SCENARIOS: Record<ScenarioKey, Scenario> = {
  fix: {
    text: "arregla el test que falla en api-service",
    steps: [
      [300, '<span class="a">📐 arquitecto</span> leyendo ticket y memoria semántica…', ["t1", "plan", "a", "Diagnóstico del test"]],
      [900, '<span class="a">📐 arquitecto</span> plan: 2 capacidades · blast radius: <b>3 ficheros</b>, 0 secretos'],
      [700, '<span class="c">💻 coder</span> worktree efímero <code>wt/fix-auth-7a1</code> creado', ["t1", "run", "c", "Parche en auth.rs"]],
      [600, '<span class="muted">  ↳ recinto Landlock activo · lecturas limitadas a workspace/api-service</span>'],
      [900, '<span class="c">💻 coder</span> 2 ficheros modificados (+18 −6)', ["t2", "plan", "a", "Test de regresión"]],
      [700, '<span class="q">🧪 qa</span> antos ci run → <span class="ok">48/48 ✓</span>', ["t1", "qa", "q", "Parche en auth.rs"]],
      [500, null, ["t2", "run", "c", "Test de regresión"]],
      [800, '<span class="q">🧪 qa</span> nuevo test de regresión <span class="ok">✓</span>', ["t2", "qa", "q", "Test de regresión"]],
      [700, '<span class="u">🛡️ auditor</span> diff consolidado listo · instantánea <code>snap-0192</code> guardada'],
    ],
    toast: ["Aprobación pendiente", "api-service · 3 ficheros · 49/49 tests en verde"],
  },
  secret: {
    text: "sincroniza releases con GITHUB_TOKEN",
    steps: [
      [300, '<span class="a">📐 arquitecto</span> la capacidad <code>forge.release_sync</code> necesita <code>secret.GITHUB_TOKEN</code>', ["t1", "plan", "a", "Sincronizar releases"]],
      [900, '<span class="warn">⚠ riesgo medio</span> · acceso a red y a un secreto de la bóveda'],
      [800, '<span class="u">🛡️ auditor</span> sin concesión activa: se pide un <em>grant</em> temporal'],
      [700, '<span class="muted">  $ antos grant secret.GITHUB_TOKEN --minutos 15 --para "sincronizar releases"</span>'],
      [900, '<span class="ok">✓</span> concesión <code>g-7f2a</code> válida 15 min · solo para esta tarea', ["t1", "run", "c", "Sincronizar releases"]],
      [900, '<span class="c">💻 coder</span> 3 releases sincronizadas', ["t1", "qa", "q", "Verificar SHA256SUMS"]],
      [700, '<span class="q">🧪 qa</span> firmas y SHA256SUMS <span class="ok">✓</span> · concesión revocada'],
    ],
    toast: ["Tarea completada", "El grant g-7f2a se ha revocado antes de caducar"],
  },
  db: {
    text: "levanta postgres para este proyecto",
    steps: [
      [300, '<span class="a">📐 arquitecto</span> perfil Nix detectado · servicio <code>postgres@16</code>', ["t1", "plan", "a", "Servicio postgres"]],
      [800, '<span class="muted">  ↳ diagnóstico de puertos: 5432 ocupado por un proceso huérfano</span>'],
      [800, '<span class="c">💻 coder</span> asignado el puerto libre 5433 · sin matar procesos ajenos', ["t1", "run", "c", "Servicio postgres"]],
      [700, '<span class="muted">  $ antos service up postgres</span>'],
      [900, '<span class="ok">✓</span> postgres efímero listo · DATABASE_URL inyectada solo en el entorno del proyecto', ["t1", "qa", "q", "Healthcheck"]],
      [700, '<span class="q">🧪 qa</span> healthcheck <span class="ok">✓</span> · migraciones aplicadas'],
    ],
    toast: ["Servicio en marcha", "postgres@16 en :5433 · se apaga con antos service down"],
  },
};

/** Una intención libre cae en el guion más cercano por palabras clave. */
export function pickScenario(text: string): ScenarioKey {
  const t = text.toLowerCase();
  if (/token|secret|release|grant|clave/.test(t)) return "secret";
  if (/postgres|redis|base de datos|\bdb\b|servicio|mariadb/.test(t)) return "db";
  return "fix";
}

export function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"']/g,
    (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch] as string,
  );
}
