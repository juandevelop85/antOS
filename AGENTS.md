# antOS · Guía para Agentes de Desarrollo

Bienvenido al repositorio de **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa.

## 🛠️ Herramientas y Skills Disponibles

- **Skill `antos-ticket-runner`** ([`.agents/skills/antos-ticket-runner/SKILL.md`](file:///Users/juandevelop/Develop/antOS/.agents/skills/antos-ticket-runner/SKILL.md)):
  Utilizar esta skill para planificar, ejecutar y cerrar de forma sistemática los tickets de [`docs/tickets/`](file:///Users/juandevelop/Develop/antOS/docs/tickets/).

## 📋 Catálogo de Tickets
El backlog maestro se encuentra en [`docs/tickets/README.md`](file:///Users/juandevelop/Develop/antOS/docs/tickets/README.md).

## 🦀 Estructura de Crates
- `system/protocolo`: Mensajes IPC y tipos de intercambio (`antos-protocolo`).
- `system/antosd`: Demonio del sistema operativo (`antosd` / binario `antos`).
- `system/capabilities`: Capacidades tipadas (Git, Sandboxes, Worktrees, Puertos, Servicios, Secretos).
- `system/barra`: Shell de escritorio / Interfaz Wayland GTK4.
- `kernel`: Núcleo `no_std` en Rust.
- `builder`: Generador de imágenes de arranque.
