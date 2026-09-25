// Contenido de las secciones de la landing. Al cambiar el estado de un módulo
// (cabeceras `## Estado de implementación`, T31.14) hay que actualizar STATUS y
// TICKETS: la página promete lo mismo que el código, ni más ni menos.

import type { ReactNode } from "react";

export const REPO_URL = "https://github.com/juandevelop85/antOS";
export const RELEASES_URL = `${REPO_URL}/releases`;
export const docUrl = (path: string) => `${REPO_URL}/blob/master/${path}`;

/** Tickets completados / totales en docs/tickets/README.md. */
export const TICKETS = { done: 152, total: 158 };

export const PROBLEMS: { title: string; body: ReactNode }[] = [
  {
    title: "Autoridad ambiental total",
    body: <>Un script o un agente de IA hereda todo lo que tú puedes hacer: leer tu <code>.env</code>, tus llaves SSH o tu carpeta personal.</>,
  },
  {
    title: "Solo entienden bytes",
    body: <>El sistema no sabe qué es un repositorio, una rama, un test que falla ni un puerto ocupado. Todo ese contexto lo reconstruyes tú a mano.</>,
  },
  {
    title: "Sin marcha atrás",
    body: <>Si un agente rompe algo, te toca arreglarlo con <code>git reflog</code> y paciencia. Nada está pensado para deshacerse.</>,
  },
];

export const AGENT_CARDS = [
  { icon: "📐", name: "Arquitecto", color: "var(--c-arch)", cmd: "antos agent do", body: "Lee el ticket, planifica contra el catálogo de capacidades y calcula el radio de impacto." },
  { icon: "💻", name: "Coder", color: "var(--c-code)", cmd: "antos swarm dispatch", body: "Implementa el plan en su propio worktree aislado, en paralelo con los demás." },
  { icon: "🧪", name: "QA", color: "var(--c-qa)", cmd: "antos ci run", body: "Ejecuta los tests del proyecto. Si no pasan, el cambio no avanza." },
  { icon: "🛡️", name: "Auditor", color: "var(--c-audit)", cmd: "antos diff", body: "Revisa el diff consolidado y te pide la aprobación antes de fusionarlo." },
];

export const FLOW: { title: string; body: ReactNode }[] = [
  { title: "Intención", body: "Pides algo en lenguaje natural." },
  { title: "Plan tipado", body: "Se traduce a capacidades con parámetros, efectos y nivel de riesgo declarados." },
  { title: "Blast radius", body: <>Ves qué ficheros, puertos y secretos se tocarán. Con <code>--dry-run</code> no se modifica nada.</> },
  { title: "Recinto", body: <>La ejecución queda confinada por el kernel, con acceso a secretos solo mediante <em>grants</em> temporales.</> },
  { title: "Instantánea", body: <>Antes de cada cambio se guarda una instantánea. <code>antos undo --ticket</code> lo revierte todo.</> },
];

export const COMPARISON: [string, string, string][] = [
  ["Acceso de un agente", "Todo lo que puede el usuario", "Capacidades declaradas"],
  ["Secretos", "Legibles siempre", "Grant con caducidad"],
  ["Lecturas en Linux", "Sin restricción", "Confinadas con Landlock"],
  ["Deshacer", "Manual", "Atómico, por ticket"],
  ["Contexto del repo", "Ninguno", "Ramas, diffs, tests y tickets"],
];

export const CRATES: [string, string][] = [
  ["kernel", "Núcleo bare-metal para x86_64 y AArch64 con compositor 2D, VirtIO y cargador ELF64"],
  ["antos-init", "PID 1 y shell soberano, sin glibc ni musl"],
  ["antosd", "Demonio, CLI, planificador, sandbox y antFlow"],
  ["barra", "Shell Wayland con GTK4 Layer Shell"],
  ["builder", "Imágenes BIOS, UEFI e ISO híbridas"],
  ["capabilities", "Manifiestos TOML con contratos y riesgos"],
];

export type StatusKind = "real" | "partial" | "sim" | "wip";

export const STATUS_LABEL: Record<StatusKind, string> = {
  real: "Real",
  partial: "Parcial",
  sim: "Simulado",
  wip: "En curso",
};

export const STATUS: { kind: StatusKind; title: string; body: ReactNode }[] = [
  { kind: "real", title: "Recinto Landlock / Seatbelt", body: "Syscalls de Landlock invocadas directamente, sin crate intermedio. Confina también las lecturas en Linux." },
  { kind: "real", title: "antFlow sobre worktrees", body: "Ejecución paralela, validación con tests y consolidación de diffs." },
  { kind: "real", title: "Undo por ticket e instantáneas", body: <><code>antos undo</code> y <code>antos snapshot restore</code>.</> },
  { kind: "real", title: "Kernel bare-metal", body: "Arranca hasta el shell interactivo en UTM y en VirtualBox ARM64." },
  { kind: "real", title: "CRDT de edición colaborativa", body: "RGA propio con desempate determinista." },
  { kind: "partial", title: "Autopilot", body: "El escaneo del workspace y los incidentes son reales, pero no cubre todo el ciclo autónomo." },
  { kind: "partial", title: "Profiler", body: <>Mide con <code>getrusage(2)</code> cuando puede. Parte de los datos todavía se fabrica y así se etiqueta.</> },
  { kind: "sim", title: "Supervisor eBPF", body: "Es un panel de auditoría en memoria. Todavía no carga ningún programa LSM." },
  { kind: "sim", title: "Depuración DAP", body: "Todavía no lanza ningún depurador." },
  { kind: "wip", title: "Shell soberano (Fase 29)", body: "Parser, ejecución de ELF externos, historial y autocompletado." },
];
