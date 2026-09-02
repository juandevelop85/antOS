# antOS · Catálogo y Hoja de Ruta de Tickets

Este directorio contiene el desglose técnico y ordenado de tareas para transformar el sistema en **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa.

---

## Estado Global del Backlog

| Fase | ID | Título | Estado |
| :--- | :--- | :--- | :--- |
| **Fase 0** | [T0.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T0.1-renombrado-de-syso-a-antos.md) | Renombrado integral del sistema de `syso` a `antOS` | ✅ Completado |
| **Fase 1** | [T1.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T1.1-protocolo-introspeccion-git.md) | Extensión de `antos-protocolo` para introspección Git | ✅ Completado |
| **Fase 1** | [T1.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T1.2-analizador-git-en-demonio.md) | Analizador de repositorios Git en segundo plano en `antosd` | ✅ Completado |
| **Fase 1** | [T1.3](file:///Users/juandevelop/Develop/antOS/docs/tickets/T1.3-spec-engine-tickets-parser.md) | Indexador y parser nativo de especificaciones y tickets Markdown | ✅ Completado |
| **Fase 2** | [T2.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.1-capacidades-git-semantico.md) | Capacidades tipadas para Git (`status`, `commit_semantic`, `branch`) | ✅ Completado |
| **Fase 2** | [T2.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.2-capacidades-worktrees-aislados.md) | Capacidad de gestión de *Git Worktrees* efímeros para agentes | ✅ Completado |
| **Fase 2** | [T2.3](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.3-diagnostico-de-puertos-y-procesos.md) | Capacidad de diagnóstico y liberación de puertos en colisión | ✅ Completado |
| **Fase 3** | [T3.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.1-roles-y-maquina-de-estados-de-agentes.md) | Orquestador de roles multi-agente (antFlow Core en Rust) | ✅ Completado |
| **Fase 3** | [T3.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.2-flujo-ejecucion-paralela-en-worktrees.md) | Ejecución paralela de tareas en worktrees con validación de tests | ✅ Completado |
| **Fase 3** | [T3.3](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.3-consolidador-de-diffs-y-rollback-por-ticket.md) | Consolidador de diffs y reversión granular por ticket (`undo --ticket`) | ✅ Completado |
| **Fase 4** | [T4.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T4.1-barra-intencion-actualizada.md) | Adaptación y enriquecimiento de la barra de intenciones Wayland/GTK4 | ✅ Completado |
| **Fase 4** | [T4.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T4.2-panel-centro-de-agentes-y-tickets.md) | Centro de control de agentes y tablero de tickets (`Super + A`) | ✅ Completado |
| **Fase 5** | [T5.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T5.1-servicios-locales-efimeros-nix.md) | Aprovisionamiento declarativo de servicios efímeros (Bases de datos/Nix) | ✅ Completado |
| **Fase 5** | [T5.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T5.2-boveda-segura-de-secretos-y-grants.md) | Bóveda de secretos y blindaje de `.env`/claves SSH con concesiones | ✅ Completado |
| **Fase 6** | [T6.1](T6.1-motor-de-inferencia-llm-local-con-ollama-y-fallback-offline.md) | Motor de Inferencia LLM Local con Ollama y Fallback Offline | ✅ Completado |
| **Fase 6** | [T6.2](T6.2-memoria-semántica-y-grafo-de-contexto-del-proyecto-con-sqlite-vectorial.md) | Memoria Semántica y Grafo de Contexto del Proyecto con SQLite Vectorial | ✅ Completado |
| **Fase 7** | [T7.1](T7.1-gestor-de-perfiles-de-entorno-declarativo-nix-y-devbox-por-proyecto.md) | Gestor de Perfiles de Entorno Declarativo Nix y Devbox por Proyecto | ✅ Completado |
| **Fase 7** | [T7.2](T7.2-control-de-cuotas-de-cpu-y-memoria-para-sandboxes-de-agentes.md) | Control de Cuotas de CPU y Memoria para Sandboxes de Agentes | ✅ Completado |
| **Fase 8** | [T8.1](T8.1-visor-de-diffs-interactivo-y-terminal-embebido-en-wayland.md) | Visor de Diffs Interactivo y Terminal Embebido en Wayland | ⏳ Pendiente |
| **Fase 8** | [T8.2](T8.2-bandeja-de-notificaciones-y-aprobaciones-asíncronas-para-agentes.md) | Bandeja de Notificaciones y Aprobaciones Asíncronas para Agentes | ⏳ Pendiente |