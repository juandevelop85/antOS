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
| **Fase 8** | [T8.1](T8.1-visor-de-diffs-interactivo-y-terminal-embebido-en-wayland.md) | Visor de Diffs Interactivo y Terminal Embebido en Wayland | ✅ Completado |
| **Fase 8** | [T8.2](T8.2-bandeja-de-notificaciones-y-aprobaciones-asíncronas-para-agentes.md) | Bandeja de Notificaciones y Aprobaciones Asíncronas para Agentes | ✅ Completado |
| **Fase 9** | [T9.1](T9.1-protocolo-de-red-p2p-cifrado-antmesh-con-descubrimiento-mdns-y-quic.md) | Protocolo de Red P2P Cifrado (antMesh) con Descubrimiento mDNS y QUIC | ✅ Completado |
| **Fase 9** | [T9.2](T9.2-despacho-distribuido-de-roles-antflow-a-nodos-de-gpu-y-sincronización-de-worktrees.md) | Despacho Distribuido de Roles antFlow a Nodos de GPU y Sincronización de Worktrees | ✅ Completado |
| **Fase 10** | [T10.1](T10.1-sistema-de-ficheros-virtual-fuse-para-inspección-de-ast-símbolos-y-diffs-antfs.md) | Sistema de Ficheros Virtual FUSE para Inspección de AST, Símbolos y Diffs (/antfs) | ✅ Completado |
| **Fase 10** | [T10.2](T10.2-interceptores-de-escritura-semántica-con-validación-tipada-previa-a-disco.md) | Interceptores de Escritura Semántica con Validación Tipada Previa a Disco | ✅ Completado |
| **Fase 11** | [T11.1](T11.1-supervisor-kernel-ebpf-lsm-para-detección-de-fugas-de-sandbox-y-syscalls-anómalas.md) | Supervisor Kernel eBPF (LSM) para Detección de Fugas de Sandbox y Syscalls Anómalas | ✅ Completado |
| **Fase 11** | [T11.2](T11.2-profiler-continuo-de-cpu-y-memoria-en-runtime-con-sugerencias-de-optimización-para-agentes.md) | Profiler Continuo de CPU y Memoria en Runtime con Sugerencias de Optimización para Agentes | ✅ Completado |
| **Fase 12** | [T12.1](T12.1-servidor-language-server-protocol-lsp-unificado-alimentado-por-la-memoria-semántica.md) | Servidor Language Server Protocol (LSP) Unificado Alimentado por la Memoria Semántica | ✅ Completado |
| **Fase 12** | [T12.2](T12.2-edición-colaborativa-en-vivo-humano-agente-y-protocolo-dap-de-depuración-aislada.md) | Edición Colaborativa en Vivo Humano-Agente y Protocolo DAP de Depuración Aislada | ✅ Completado |
| **Fase 13** | [T13.0](T13.0-compositor-wayland-ultraligero-y-configuración-declarativa-de-sesión-de-escritorio.md) | Compositor Wayland Ultraligero y Configuración Declarativa de Sesión de Escritorio | ✅ Completado |
| **Fase 13** | [T13.1](T13.1-integracion-de-telemetria-ebpf-profiler-y-pair-programming-en-la-barra-wayland-gtk4.md) | Integración de Telemetría eBPF, Profiler y Pair Programming en la Barra Wayland GTK4 | ✅ Completado |
| **Fase 13** | [T13.2](T13.2-pipeline-de-arranque-bare-metal-compilacion-cruzada-de-kernel-y-disco-bios-uefi-en-qemu.md) | Pipeline de Arranque Bare Metal, Compilación Cruzada de Kernel y Disco BIOS/UEFI en QEMU | ✅ Completado |
| **Fase 14** | [T14.1](T14.1-motor-de-capacidades-y-plugins-en-webassembly-wasi-con-aislamiento-de-memoria.md) | Motor de Capacidades y Plugins en WebAssembly (WASI) con Aislamiento de Memoria | ✅ Completado |
| **Fase 14** | [T14.2](T14.2-agente-multimodal-con-captura-de-pantalla-wayland-para-inspeccion-y-qa-visual.md) | Agente Multimodal con Captura de Pantalla Wayland para Inspección y QA Visual | ✅ Completado |
| **Fase 14** | [T14.3](T14.3-generador-de-live-iso-autonoma-empaquetado-release-y-distribucion-v0.1.0.md) | Generador de Live ISO Autónoma, Empaquetado Release y Distribución v0.1.0 | ✅ Completado |
| **Fase 15** | [T15.1](T15.1-motor-de-inspeccion-de-almacenamiento-y-particionador-gpt.md) | Motor de Inspección de Almacenamiento y Particionador GPT | ✅ Completado |
| **Fase 15** | [T15.2](T15.2-instalador-guiado-de-sistema-base-disco-completo-y-dual-boot.md) | Instalador Guiado de Sistema Base (Disco Completo y Dual Boot) | ✅ Completado |
| **Fase 15** | [T15.3](T15.3-gestor-de-arranque-uefi-y-dual-boot-automatizado.md) | Gestor de Arranque UEFI y Dual Boot Automatizado | ✅ Completado |
| **Fase 16** | [T16.1](T16.1-microvms-efimeras-y-aislamiento-por-hipervisor-kvm.md) | MicroVMs Efímeras y Aislamiento por Hipervisor (KVM / Cloud-Hypervisor) | ✅ Completado |
| **Fase 16** | [T16.2](T16.2-gestor-de-paquetes-y-recetas-inmutables-antpkg.md) | Gestor de Paquetes y Recetas Inmutables (`antpkg`) | ✅ Completado |
| **Fase 16** | [T16.3](T16.3-modo-agente-autonomo-continuo-autopilot-daemon.md) | Modo Agente Autónomo Continuo (*Autopilot Daemon*) | ✅ Completado |
| **Fase 16** | [T16.4](T16.4-consola-web-remota-en-tiempo-real-y-bridge-websocket.md) | Consola Web Remota en Tiempo Real y Bridge WebSocket | ✅ Completado |
| **Fase 17** | [T17.1](T17.1-aislamiento-de-frontera-git-y-descubrimiento-contextual-de-workspace.md) | Aislamiento de Frontera Git y Descubrimiento Contextual de Workspace | ✅ Completado |
| **Fase 17** | [T17.2](T17.2-soporte-multi-proyecto-en-visor-de-diffs-y-estado-de-workspace.md) | Soporte Multi-Proyecto en Visor de Diffs y Estado de Workspace | ⏳ Pendiente |
| **Fase 17** | [T17.3](T17.3-inicializacion-y-gestion-declarativa-de-proyectos-git-en-workspace.md) | Inicialización y Gestión Declarativa de Proyectos Git en Workspace | ⏳ Pendiente |