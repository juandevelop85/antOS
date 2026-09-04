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
| **Fase 17** | [T17.2](T17.2-soporte-multi-proyecto-en-visor-de-diffs-y-estado-de-workspace.md) | Soporte Multi-Proyecto en Visor de Diffs y Estado de Workspace | ✅ Completado |
| **Fase 17** | [T17.3](T17.3-inicializacion-y-gestion-declarativa-de-proyectos-git-en-workspace.md) | Inicialización y Gestión Declarativa de Proyectos Git en Workspace | ✅ Completado |
| **Fase 17** | [T17.4](T17.4-catalogo-y-gestion-de-tickets-desacoplados-por-proyecto-en-workspace.md) | Catálogo y Gestión de Tickets Desacoplados por Proyecto en Workspace | ✅ Completado |
| **Fase 18** | [T18.1](T18.1-abstraccion-de-capa-de-hardware-hal-y-desacoplamiento-de-arquitectura-en-kernel.md) | Abstracción de Capa de Hardware (HAL) y Desacoplamiento de Arquitectura en Kernel | ✅ Completado |
| **Fase 18** | [T18.2](T18.2-arranque-aarch64-consola-serie-pl011-y-vectores-de-excepcion-vbar-el1.md) | Arranque AArch64, Consola Serie PL011 y Vectores de Excepción VBAR_EL1 | ✅ Completado |
| **Fase 18** | [T18.3](T18.3-paginacion-aarch64-ttbr0-ttbr1-y-controlador-de-interrupciones-gic.md) | Paginación AArch64 (TTBR0/TTBR1) y Controlador de Interrupciones GIC | ✅ Completado |
| **Fase 18** | [T18.4](T18.4-llamadas-al-sistema-svc-en-aarch64-y-generacion-de-imagenes-uefi-bootaa64-efi.md) | Llamadas al Sistema (SVC) en AArch64 y Generación de Imágenes UEFI (BOOTAA64.EFI) | ✅ Completado |
| **Fase 19** | [T19.1](T19.1-motor-de-inferencia-openai-compatible-y-hub-de-proveedores-gratuitos.md) | Motor de Inferencia OpenAI-Compatible y Hub de Proveedores Gratuitos | ✅ Completado |
| **Fase 19** | [T19.2](T19.2-gestion-de-configuracion-persistente-seleccion-dinamica-y-catalogo-de-llms-gratuitos.md) | Gestión de Configuración Persistente, Selección Dinámica y Catálogo de LLMs Gratuitos | ✅ Completado |
| **Fase 19** | [T19.3](T19.3-deteccion-e-instalacion-de-ollama-y-opencode-por-defecto-con-receta-antpkg.md) | Detección e Instalación de Ollama y OpenCode por Defecto con Receta antpkg | ✅ Completado |
| **Fase 19** | [T19.4](T19.4-enriquecimiento-del-equipo-multi-agente-antflow-y-asignacion-de-modelos-por-rol.md) | Enriquecimiento del Equipo Multi-Agente antFlow y Asignación de Modelos por Rol | ✅ Completado |
| **Fase 20** | [T20.1](T20.1-espacio-de-trabajo-integrado-dev-tui-con-neovim-monitor-de-agentes-y-visor-de-diffs.md) | Espacio de Trabajo Integrado Dev TUI con Neovim, Monitor de Agentes y Visor de Diffs | ✅ Completado |
| **Fase 20** | [T20.2](T20.2-reproductor-autonomo-de-bugs-y-generador-de-tests-de-regresion-tdd.md) | Reproductor Autónomo de Bugs y Generador de Tests de Regresión TDD | ✅ Completado |
| **Fase 20** | [T20.3](T20.3-matriz-de-ci-cd-local-paralela-en-sandboxes-y-pre-commit-hooks-del-auditor.md) | Matriz de CI/CD Local Paralela en Sandboxes y Pre-Commit Hooks del Auditor | ✅ Completado |
| **Fase 20** | [T20.4](T20.4-snapshots-atomicos-de-entorno-de-desarrollo-y-time-machine-de-estado.md) | Snapshots Atómicos de Entorno de Desarrollo y Time Machine de Estado | ✅ Completado |
| **Fase 21** | [T21.1](T21.1-benchmarking-continuo-y-deteccion-de-regresiones-de-rendimiento-en-worktrees.md) | Benchmarking Continuo y Detección de Regresiones de Rendimiento en Worktrees | ✅ Completado |
| **Fase 21** | [T21.2](T21.2-sincronizacion-bidireccional-con-forjas-git-issues-a-tickets-y-pull-requests.md) | Sincronización Bidireccional con Forjas Git: Issues a Tickets y Pull Requests | ✅ Completado |
| **Fase 21** | [T21.3](T21.3-generador-y-sincronizador-de-documentacion-viva-de-arquitectura-y-diagramas-mermaid.md) | Generador y Sincronizador de Documentación Viva de Arquitectura y Diagramas Mermaid | ✅ Completado |
| **Fase 22** | [T22.1](T22.1-modularizacion-y-desacoplamiento-de-antosd-en-cli-y-subcomandos.md) | Modularización y Desacoplamiento de `antosd` en CLI y Subcomandos | ✅ Completado |
| **Fase 22** | [T22.2](T22.2-descomposicion-modular-de-antos-protocolo-en-submodulos-tematicos.md) | Descomposición Modular de `antos-protocolo` en Submódulos Temáticos | ✅ Completado |
| **Fase 22** | [T22.3](T22.3-estandarizacion-de-nomenclatura-en-ingles-y-limpieza-de-deuda-tecnica.md) | Estandarización de Nomenclatura en Inglés y Limpieza de Deuda Técnica | ✅ Completado |
| **Fase 22** | [T22.4](T22.4-capa-de-abstraccion-de-runtime-de-plataforma-platform-runtime.md) | Capa de Abstracción de Runtime de Plataforma (`PlatformRuntime`) | ✅ Completado |
| **Fase 22** | [T22.5](T22.5-interfaz-abi-inicial-kernel-userspace-y-proceso-init-bare-metal.md) | Interfaz ABI Inicial Kernel-Userspace y Proceso Init Bare-Metal | ✅ Completado |
| **Fase 22** | [T22.6](T22.6-matriz-de-ci-cd-automatizada-y-verificacion-multiplataforma-en-github-actions.md) | Matriz de CI/CD Automatizada y Verificación Multiplataforma en GitHub Actions | ✅ Completado |
| **Fase 23** | [T23.1](T23.1-controlador-de-interrupciones-apic-lapic-ioapic-y-reemplazo-de-pic8259.md) | Controlador de Interrupciones APIC (LAPIC/IOAPIC) y Reemplazo del PIC 8259 | ✅ Completado |
| **Fase 23** | [T23.2](T23.2-planificador-preemptivo-bloques-pcb-tcb-y-conmutacion-de-contexto.md) | Planificador Preemptivo, Bloques PCB/TCB y Conmutación de Contexto | ✅ Completado |
| **Fase 23** | [T23.3](T23.3-consola-grafica-framebuffer-con-fuente-bitmap-y-secuencias-ansi.md) | Consola Gráfica Framebuffer en Pantalla con Fuente Bitmap y Secuencias ANSI | ✅ Completado |
| **Fase 23** | [T23.4](T23.4-driver-de-bloque-virtio-blk-y-sistema-de-ficheros-initrd-tarfs.md) | Driver de Bloque VirtIO (`virtio-blk`) y Sistema de Ficheros Inicial Initrd/tarfs | ✅ Completado |
| **Fase 23** | [T23.5](T23.5-ampliacion-de-llamadas-al-sistema-posix-e-ipc-por-canales-microkernel.md) | Ampliación de Llamadas al Sistema POSIX e IPC por Canales Microkernel | ✅ Completado |
| **Fase 24** | [T24.1](T24.1-integracion-de-bootloader-uefi-limine-en-builder-para-arranque-hibrido.md) | Integración de Bootloader UEFI Limine en `builder` para Arranque Híbrido | ⏳ Pendiente |
| **Fase 24** | [T24.2](T24.2-empaquetador-de-ramdisk-initramfs-live-con-sistema-base-y-herramientas.md) | Empaquetador de Ramdisk (Initramfs) Live con Sistema Base y Herramientas | ⏳ Pendiente |
| **Fase 24** | [T24.3](T24.3-drivers-de-almacenamiento-fisico-ahci-sata-y-nvme-para-deteccion-de-discos.md) | Drivers de Almacenamiento Físico (AHCI/SATA y NVMe) para Detección de Discos | ⏳ Pendiente |
| **Fase 24** | [T24.4](T24.4-asistente-de-instalacion-guiado-cli-y-particionamiento-en-vivo-antos-install.md) | Asistente de Instalación Guiado CLI y Particionamiento en Vivo (`antos install`) | ⏳ Pendiente |
| **Fase 24** | [T24.5](T24.5-generador-automatizado-de-live-usb-y-script-de-grabacion-antos-usb.md) | Generador Automatizado de Live USB y Script de Grabación (`antos usb flash`) | ⏳ Pendiente |