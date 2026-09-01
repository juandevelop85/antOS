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
| **Fase 2** | [T2.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.1-capacidades-git-semantico.md) | Capacidades tipadas para Git (`status`, `commit_semantic`, `branch`) | ⏳ Pendiente |
| **Fase 2** | [T2.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.2-capacidades-worktrees-aislados.md) | Capacidad de gestión de *Git Worktrees* efímeros para agentes | ⏳ Pendiente |
| **Fase 2** | [T2.3](file:///Users/juandevelop/Develop/antOS/docs/tickets/T2.3-diagnostico-de-puertos-y-procesos.md) | Capacidad de diagnóstico y liberación de puertos en colisión | ⏳ Pendiente |
| **Fase 3** | [T3.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.1-roles-y-maquina-de-estados-de-agentes.md) | Orquestador de roles multi-agente (antFlow Core en Rust) | ⏳ Pendiente |
| **Fase 3** | [T3.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.2-flujo-ejecucion-paralela-en-worktrees.md) | Ejecución paralela de tareas en worktrees con validación de tests | ⏳ Pendiente |
| **Fase 3** | [T3.3](file:///Users/juandevelop/Develop/antOS/docs/tickets/T3.3-consolidador-de-diffs-y-rollback-por-ticket.md) | Consolidador de diffs y reversión granular por ticket (`undo --ticket`) | ⏳ Pendiente |
| **Fase 4** | [T4.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T4.1-barra-intencion-actualizada.md) | Adaptación y enriquecimiento de la barra de intenciones Wayland/GTK4 | ⏳ Pendiente |
| **Fase 4** | [T4.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T4.2-panel-centro-de-agentes-y-tickets.md) | Centro de control de agentes y tablero de tickets (`Super + A`) | ⏳ Pendiente |
| **Fase 5** | [T5.1](file:///Users/juandevelop/Develop/antOS/docs/tickets/T5.1-servicios-locales-efimeros-nix.md) | Aprovisionamiento declarativo de servicios efímeros (Bases de datos/Nix) | ⏳ Pendiente |
| **Fase 5** | [T5.2](file:///Users/juandevelop/Develop/antOS/docs/tickets/T5.2-boveda-segura-de-secretos-y-grants.md) | Bóveda de secretos y blindaje de `.env`/claves SSH con concesiones | ⏳ Pendiente |
