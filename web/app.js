// antOS · web de presentación (prototipo)
// La demo del hero es un guion fijo: no habla con ningún antosd real.

(() => {
  // Tema ----------------------------------------------------------------
  const root = document.documentElement;
  const store = {
    get() { try { return localStorage.getItem("antos-theme"); } catch { return null; } },
    set(v) { try { localStorage.setItem("antos-theme", v); } catch { /* sin almacenamiento */ } },
  };
  const saved = store.get();
  if (saved) root.dataset.theme = saved;
  document.getElementById("theme-toggle").addEventListener("click", () => {
    const current = root.dataset.theme
      || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
    const next = current === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    store.set(next);
  });

  // Reloj ---------------------------------------------------------------
  const clock = document.getElementById("clock");
  const tick = () => {
    clock.textContent = new Date().toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" });
  };
  tick();
  setInterval(tick, 30_000);

  // Guiones de la demo --------------------------------------------------
  const AGENT = {
    a: { name: "📐 Arquitecto", color: "var(--c-arch)" },
    c: { name: "💻 Coder", color: "var(--c-code)" },
    q: { name: "🧪 QA", color: "var(--c-qa)" },
    u: { name: "🛡️ Auditor", color: "var(--c-audit)" },
  };

  // Cada paso: [retardo ms, línea de log (HTML), movimiento de tarjeta opcional]
  const SCENARIOS = {
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

  // Estado y DOM --------------------------------------------------------
  const form = document.getElementById("intent-form");
  const input = document.getElementById("intent-input");
  const chips = document.querySelectorAll("#chips button");
  const log = document.getElementById("log");
  const toast = document.getElementById("toast");
  const toastBody = document.getElementById("toast-body");
  const lanes = Object.fromEntries(
    [...document.querySelectorAll(".lane")].map((el) => [el.dataset.lane, el]),
  );
  let running = false;
  let cards = {};
  let timers = [];

  const later = (ms) => new Promise((r) => timers.push(setTimeout(r, ms)));
  const setBusy = (busy) => {
    running = busy;
    chips.forEach((c) => (c.disabled = busy));
    input.disabled = busy;
  };
  const write = (html) => {
    log.insertAdjacentHTML("beforeend", "\n" + html);
    log.scrollTop = log.scrollHeight;
  };
  const placeCard = ([id, lane, who, title]) => {
    const old = cards[id];
    if (old) old.remove();
    const el = document.createElement("div");
    el.className = "card";
    el.style.setProperty("--c", AGENT[who].color);
    el.innerHTML = `<b>${AGENT[who].name}</b>${title}`;
    lanes[lane].appendChild(el);
    cards[id] = el;
  };
  const reset = () => {
    timers.forEach(clearTimeout);
    timers = [];
    Object.values(lanes).forEach((l) => (l.innerHTML = ""));
    cards = {};
    toast.hidden = true;
  };

  // Una intención libre cae en el guion más cercano por palabras clave.
  const pick = (text) => {
    const t = text.toLowerCase();
    if (/token|secret|release|grant|clave/.test(t)) return "secret";
    if (/postgres|redis|base de datos|db|servicio|mariadb/.test(t)) return "db";
    return "fix";
  };

  async function run(key, typed) {
    if (running) return;
    const sc = SCENARIOS[key];
    reset();
    setBusy(true);
    input.value = typed ?? sc.text;
    log.innerHTML = `<span class="p">❯</span> ${escapeHtml(input.value)}`;
    for (const [ms, line, move] of sc.steps) {
      await later(ms);
      if (line) write(line);
      if (move) placeCard(move);
    }
    await later(500);
    toastBody.innerHTML = `<strong>${sc.toast[0]}</strong><p>${sc.toast[1]}</p>`;
    toast.hidden = false;
    setBusy(false);
  }

  function escapeHtml(s) {
    return s.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);
  }

  chips.forEach((c) => c.addEventListener("click", () => run(c.dataset.intent)));
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const text = input.value.trim();
    if (text) run(pick(text), text);
  });

  document.getElementById("toast-ok").addEventListener("click", () => {
    toast.hidden = true;
    Object.entries(cards).forEach(([id, el]) => {
      const who = el.style.getPropertyValue("--c") === AGENT.q.color ? "q" : "u";
      placeCard([id, "done", who, el.lastChild.textContent]);
    });
    write('<span class="ok">✓ aprobado</span> · fusionado en main · worktrees limpiados');
  });
  document.getElementById("toast-undo").addEventListener("click", () => {
    toast.hidden = true;
    write('<span class="warn">↺ antos undo</span> · restaurada la instantánea previa · 0 cambios en disco');
    Object.values(lanes).forEach((l) => (l.innerHTML = ""));
    cards = {};
  });

  // Arranque automático suave para que el hero no esté vacío.
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
    timers.push(setTimeout(() => run("fix"), 1200));
  }
})();
