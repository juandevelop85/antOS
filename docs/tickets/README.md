# antOS · Catálogo y Hoja de Ruta de Tickets

Este directorio contiene el desglose técnico y ordenado de tareas para transformar el sistema en **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa.

> **Dos vías a partir de la Fase 29:**
> - **antOS Linux (driver diario, Fase 30):** NixOS + sesión Wayland + `antos-barra` + userland de desarrollo (Neovim, Git, `antos dev`). Es el sistema pensado para usarse a diario; corre software existente porque es Linux.
> - **Kernel bare-metal (I+D de soberanía, Fase 29 y sucesivas):** el núcleo `no_std` propio (`kernel/` + `user/`), con su compositor 2D y su shell. Vía de investigación en paralelo; no ejecuta software POSIX.
> El CLI `antos` y `antos-barra` (`system/`) son compartidos por la vía Linux y por el host.

> **CI verde es requisito de cierre (T31.10).** La regla 2 de
> [`.agents/rules/antos-development.md`](../../.agents/rules/antos-development.md)
> ya exige verificar los criterios de aceptación con `cargo test --workspace`
> antes de marcar un ticket como completado; esto lo deja explícito para las
> puertas de calidad de [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)
> también: **ningún ticket se da por cerrado si deja algún trabajo de la CI en
> rojo**, ni siquiera uno que el propio ticket no tocó. Antes de T31.10 la CI
> llevaba tiempo en rojo de fondo (`check-format`/`clippy-host`/`clippy-kernel`
> nunca pasaban sobre el árbol real) precisamente porque nada distinguía "mi
> cambio rompió algo" de "ya estaba roto", así que los tickets se cerraban sin
> esa señal. Si un ticket necesita tocar código fuera de su alcance declarado
> para no dejar la CI en rojo, se documenta esa ampliación de alcance en el
> propio ticket (como ya viene siendo la práctica en la Fase 31), no se deja
> pasar en silencio.

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
| **Fase 24** | [T24.1](T24.1-integracion-de-bootloader-uefi-limine-en-builder-para-arranque-hibrido.md) | Integración de Bootloader UEFI Limine en `builder` para Arranque Híbrido | ✅ Completado |
| **Fase 24** | [T24.2](T24.2-empaquetador-de-ramdisk-initramfs-live-con-sistema-base-y-herramientas.md) | Empaquetador de Ramdisk (Initramfs) Live con Sistema Base y Herramientas | ✅ Completado |
| **Fase 24** | [T24.3](T24.3-drivers-de-almacenamiento-fisico-ahci-sata-y-nvme-para-deteccion-de-discos.md) | Drivers de Almacenamiento Físico (AHCI/SATA y NVMe) para Detección de Discos | ✅ Completado |
| **Fase 24** | [T24.4](T24.4-asistente-de-instalacion-guiado-cli-y-particionamiento-en-vivo-antos-install.md) | Asistente de Instalación Guiado CLI y Particionamiento en Vivo (`antos install`) | ✅ Completado |
| **Fase 24** | [T24.5](T24.5-generador-automatizado-de-live-usb-y-script-de-grabacion-antos-usb.md) | Generador Automatizado de Live USB y Script de Grabación (`antos usb flash`) | ✅ Completado |
| **Fase 25** | [T25.1](T25.1-extension-de-antpkg-para-aplicaciones-graficas-xdg-y-desktop-entries.md) | Extensión de `antpkg` para Aplicaciones Gráficas XDG y Desktop Entries | ✅ Completado |
| **Fase 25** | [T25.2](T25.2-gestor-y-puente-de-aplicaciones-flatpak-y-contenedores-graficos.md) | Gestor y Puente de Aplicaciones Flatpak y Contenedores Gráficos (`antos app`) | ✅ Completado |
| **Fase 25** | [T25.3](T25.3-catalogo-oficial-de-recetas-antpkg-para-navegadores-e-ides.md) | Catálogo Oficial de Recetas antpkg para Navegadores e IDEs | ✅ Completado |
| **Fase 25** | [T25.4](T25.4-lanzador-de-aplicaciones-graficas-y-contexto-de-workspace-en-barra-wayland.md) | Lanzador de Aplicaciones Gráficas y Contexto de Workspace en Barra Wayland | ✅ Completado |
| **Fase 26** | [T26.1](T26.1-controlador-de-framebuffer-grafico-aarch64-y-virtio-gpu.md) | Controlador de Framebuffer Gráfico AArch64 y VirtIO-GPU en Kernel Bare-Metal | ✅ Completado |
| **Fase 26** | [T26.2](T26.2-desktop-shell-nativo-en-rust-y-compositor-2d-framebuffer.md) | Desktop Shell Nativo en Rust y Compositor 2D sobre Framebuffer | ✅ Completado |
| **Fase 26** | [T26.3](T26.3-controlador-de-entrada-virtio-input-teclado-y-raton.md) | Controlador de Entrada Nativo: VirtIO-Input, Teclado y Ratón | ✅ Completado |
| **Fase 26** | [T26.4](T26.4-cargador-de-ejecutables-elf64-y-sistema-de-ficheros-initramfs-tarfs.md) | Cargador de Ejecutables ELF64 y Sistema de Ficheros Initramfs Tarfs | ✅ Completado |
| **Fase 26** | [T26.5](T26.5-runtime-soberano-libantos-y-shell-interactivo-en-espacio-de-usuario.md) | Runtime Soberano `libantos` y Shell Interactivo en Espacio de Usuario | ✅ Completado |
| **Fase 27** | [T27.1](T27.1-soporte-real-del-protocolo-de-arranque-limine-en-el-kernel.md) | Soporte Real del Protocolo de Arranque Limine (Mitad Alta y Boot Requests) en el Kernel | ✅ Completado |
| **Fase 27** | [T27.2](T27.2-controlador-pcie-ecam-y-host-usb-xhci.md) | Controlador PCIe ECAM y Host Controller USB 3.0 xHCI | ✅ Completado |
| **Fase 27** | [T27.3](T27.3-pila-usb-y-subclase-hid-para-teclado-y-raton.md) | Pila USB y Subclase HID para Teclado y Ratón en Bare-Metal | ✅ Completado |
| **Fase 27** | [T27.4](T27.4-terminal-grafico-activo-en-compositor-y-multiplexacion.md) | Terminal Gráfico Activo en Compositor 2D y Control Soberano de Escritorio | ✅ Completado |
| **Fase 28** | [T28.1](T28.1-verificacion-y-robustez-del-driver-virtio-input-mmio.md) | Verificación y Robustez del Driver VirtIO-Input MMIO (Teclado/Ratón/Tablet Nativos) | ✅ Completado |
| **Fase 28** | [T28.2](T28.2-mapeo-pcie-ecam-mmio-multiplataforma-y-escaneo-de-bus-con-puentes.md) | Mapeo PCIe ECAM/MMIO Multi-Plataforma y Escaneo de Bus con Puentes | ✅ Completado |
| **Fase 28** | [T28.3](T28.3-xhci-robusto-rings-por-endpoint-control-transfers-hotplug-y-hubs.md) | xHCI Robusto: Rings/Buffers por Endpoint, Control Transfers Extendidas, Hotplug y Hubs | ✅ Completado |
| **Fase 28** | [T28.4](T28.4-parser-de-hid-report-descriptor-y-decodificador-generico-por-usages.md) | Parser de HID Report Descriptor y Decodificador Genérico Dirigido por Usages | ✅ Completado |
| **Fase 28** | [T28.5](T28.5-ergonomia-de-entrada-leds-auto-repeat-layouts-y-aceleracion-de-puntero.md) | Ergonomía de Entrada: LEDs de Teclado, Auto-Repeat, Layouts y Aceleración de Puntero | ✅ Completado |
| **Fase 28** | [T28.6](T28.6-timer-aarch64-resiliente-y-gic-v2-v3-con-enrutado-de-irq-de-perifericos.md) | Timer AArch64 Resiliente (Fallback Físico EL1) y GIC v2/v3 con Enrutado de IRQ de Periféricos | ✅ Completado |
| **Fase 28** | [T28.7](T28.7-paridad-de-display-virtio-gpu-pci-ramfb-y-cadena-de-fallback-de-framebuffer.md) | Paridad de Display: virtio-gpu-pci, ramfb y Cadena de Fallback de Framebuffer | ✅ Completado |
| **Fase 28** | [T28.8](T28.8-descubrimiento-por-firmware-dtb-acpi-y-bring-up-sin-direcciones-hardcodeadas.md) | Descubrimiento por Firmware (DTB/ACPI) y Bring-Up sin Direcciones Hardcodeadas | ✅ Completado |
| **Fase 28** | [T28.9](T28.9-perifericos-x86-64-raton-ps2-y-pila-usb-xhci-en-x86-64.md) | Periféricos x86_64: Ratón PS/2 y Pila USB xHCI en x86_64 | ✅ Completado |
| **Fase 28** | [T28.10](T28.10-banco-de-pruebas-de-perifericos-matriz-de-emulacion-y-tests-de-integracion.md) | Banco de Pruebas de Periféricos: Matriz de Emulación y Tests de Integración de Entrada | ✅ Completado |
| **Fase 29** | [T29.1](T29.1-framework-de-despacho-de-comandos-y-parser-de-linea-en-el-shell-soberano.md) | Framework de Despacho de Comandos y Parser de Línea en el Shell Soberano | ⏳ Pendiente |
| **Fase 29** | [T29.2](T29.2-ejecucion-de-elf-y-coreutils-minimas-no-std.md) | Ejecución de ELF Externos y Coreutils Mínimas `no_std` | ⏳ Pendiente |
| **Fase 29** | [T29.3](T29.3-edicion-de-linea-historial-y-autocompletado-en-el-shell-soberano.md) | Edición de Línea, Historial y Autocompletado en el Shell Soberano | ⏳ Pendiente |
| **Fase 30** | [T30.1](T30.1-paquete-nix-de-antos-barra-y-modulo-de-sesion-wayland-declarativo.md) | Paquete Nix de `antos-barra` y Módulo de Sesión Wayland Declarativo | 🔄 En Progreso |
| **Fase 30** | [T30.2](T30.2-imagen-grafica-de-vm-e-iso-de-antos-linux.md) | Imagen Gráfica de VM e ISO de antOS Linux | 🔄 En Progreso |
| **Fase 30** | [T30.3](T30.3-userland-de-desarrollo-en-la-imagen-neovim-git-y-antos-dev.md) | Userland de Desarrollo en la Imagen: Neovim, Git, Terminal y `antos dev` | 🔄 En Progreso |
| **Fase 30** | [T30.4](T30.4-verificacion-end-to-end-del-escritorio-antos-linux-y-smoke-en-ci.md) | Verificación End-to-End del Escritorio antOS Linux y Smoke en CI | 🔄 En Progreso |
| **Fase 30** | [T30.5](T30.5-instalacion-de-antos-linux-en-hardware-real-y-dual-boot.md) | Instalación de antOS Linux en Hardware Real y Dual-Boot | 🔄 En Progreso |
| **Fase 30** | [T30.6](T30.6-escritorio-tradicional-opcional-sobre-labwc.md) | Escritorio Tradicional Opcional sobre Labwc (panel, lanzador, fondo, notificaciones) | ✅ Completado |
| **Fase 30** | [T30.7](T30.7-arranque-acelerado-de-la-vm-grafica-en-macos-hvf.md) | Arranque Acelerado de la VM Gráfica en macOS (HVF) | ✅ Completado |
| **Fase 30** | [T30.8](T30.8-barra-compacta-con-expansion-bajo-demanda.md) | Barra Compacta con Expansión Bajo Demanda | ✅ Completado |
| **Fase 30** | [T30.9](T30.9-escritorio-kde-plasma-6-wayland-como-sabor-alternativo.md) | Escritorio KDE Plasma 6 (Wayland) como Sabor Alternativo | ✅ Completado |
| **Fase 31** | [T31.1](T31.1-bypass-de-autenticacion-y-tokens-predecibles-en-la-consola-web.md) | Bypass de Autenticación y Tokens Predecibles en la Consola Web Remota | ✅ Completado |
| **Fase 31** | [T31.2](T31.2-ciclo-de-vida-y-limites-de-recursos-del-servidor-de-consola-web.md) | Ciclo de Vida y Límites de Recursos del Servidor de Consola Web | ✅ Completado |
| **Fase 31** | [T31.3](T31.3-desbordamientos-aritmeticos-en-cargador-elf-y-tarfs-del-kernel.md) | Desbordamientos Aritméticos en el Cargador ELF y en tarfs del Kernel | ✅ Completado |
| **Fase 31** | [T31.4](T31.4-aislamiento-real-de-microvm-y-eliminacion-de-inyeccion-de-shell.md) | Aislamiento Real de MicroVM y Eliminación de Inyección de Shell | ✅ Completado |
| **Fase 31** | [T31.5](T31.5-identidad-criptografica-real-en-antmesh.md) | Identidad Criptográfica Real y Tokens de Emparejamiento en antMesh | ✅ Completado |
| **Fase 31** | [T31.6](T31.6-cifrado-en-reposo-y-escritura-atomica-de-la-boveda-de-secretos.md) | Cifrado en Reposo y Escritura Atómica de la Bóveda de Secretos | ✅ Completado |
| **Fase 31** | [T31.7](T31.7-eliminacion-de-panicos-por-unwrap-en-rutas-del-demonio.md) | Eliminación de Pánicos por `unwrap` en Rutas del Demonio | ✅ Completado |
| **Fase 31** | [T31.8](T31.8-limites-y-tiempos-de-espera-en-el-socket-ipc-del-demonio.md) | Límites y Tiempos de Espera en el Socket IPC del Demonio | ✅ Completado |
| **Fase 31** | [T31.9](T31.9-comprobacion-de-limites-del-interprete-wasm.md) | Comprobación de Límites del Intérprete WebAssembly | ✅ Completado |
| **Fase 31** | [T31.10](T31.10-reparacion-de-la-ci-en-rojo-clippy-y-rustfmt.md) | Reparación de la CI en Rojo: Puertas de Clippy y rustfmt | ✅ Completado |
| **Fase 31** | [T31.11](T31.11-retirada-de-la-capa-de-compatibilidad-syso.md) | Retirada de la Capa de Compatibilidad `syso` y de los Símbolos Deprecados | ✅ Completado |
| **Fase 31** | [T31.12](T31.12-finalizacion-de-la-nomenclatura-en-ingles.md) | Finalización de la Estandarización de Nomenclatura en Inglés | ✅ Completado |
| **Fase 31** | [T31.13](T31.13-cobertura-de-tests-de-la-capa-cli-y-de-antos-barra.md) | Cobertura de Tests de la Capa CLI y de `antos-barra` | ✅ Completado |
| **Fase 31** | [T31.14](T31.14-alineacion-de-la-documentacion-de-modulos-con-la-implementacion-real.md) | Alineación de la Documentación de Módulos con la Implementación Real | ✅ Completado |
| **Fase 31** | [T31.15](T31.15-descomposicion-de-modulos-de-gran-tamano.md) | Descomposición de Módulos de Gran Tamaño | ✅ Completado |
| **Fase 31** | [T31.16](T31.16-los-tests-del-kernel-no-se-compilan-ni-se-ejecutan.md) | Los 86 Tests del Kernel No Se Compilan ni Se Ejecutan | ✅ Completado |
| **Fase 31** | [T31.17](T31.17-la-vm-de-antos-nixos-no-se-construye.md) | La VM de antOS NixOS No Se Construye: Dos Roturas Latentes en el Camino Nix | ✅ Completado |
| **Fase 31** | [T31.18](T31.18-el-modulo-nixos-de-antos-no-arranca-el-demonio.md) | El Módulo NixOS de antOS No Arranca el Demonio | ✅ Completado |
| **Fase 31** | [T31.19](T31.19-la-ci-no-construye-por-nix.md) | La CI No Construye por Nix | ✅ Completado |
| **Fase 32** | [T32.1](T32.1-coalescencia-y-ordenacion-en-el-asignador-de-memoria-del-kernel.md) | Coalescencia, Ordenación y Reutilización en el Asignador de Memoria del Kernel | ✅ Completado |
| **Fase 32** | [T32.2](T32.2-aceleracion-del-framebuffer-y-optimizacion-de-flush-en-virtio-gpu.md) | Aceleración del Framebuffer y Optimización de Flush en VirtIO GPU | ✅ Completado |
| **Fase 32** | [T32.3](T32.3-servidor-ipc-concurrente-y-desacoplamiento-de-mutacion-en-antosd.md) | Servidor IPC Concurrente y Desacoplamiento de Mutación en antosd | ✅ Completado |
| **Fase 32** | [T32.4](T32.4-optimizacion-de-telemetria-en-memoria-y-eliminacion-de-sondeo-activo-en-barra.md) | Optimización de Telemetría en Memoria y Eliminación de Sondeo Activo en Barra | ✅ Completado |
| **Fase 32** | [T32.5](T32.5-consulta-unificada-y-cache-consistente-en-el-analizador-git.md) | Consulta Unificada y Caché Consistente en el Analizador Git | ✅ Completado |
| **Fase 32** | [T32.6](T32.6-eliminacion-de-espera-activa-busy-waiting-en-nvme-y-ahci-sata.md) | Eliminación de Espera Activa (Busy-Waiting) en Controladores NVMe y AHCI SATA | ✅ Completado |
| **Fase 32** | [T32.7](T32.7-optimizacion-de-grafo-y-vectores-en-memoria-semantica.md) | Optimización de Grafo y Vectores en Memoria Semántica y Visor de Diffs | ✅ Completado |
| **Fase 32** | [T32.8](T32.8-zero-copy-en-vfs-y-clones-copy-on-write-para-linux.md) | Zero-Copy en VFS y Clones Copy-on-Write en Linux | ✅ Completado |
| **Fase 33** | [T33.1](T33.1-honestidad-de-antflow-y-autopilot-estado-de-implementacion-visible.md) | Honestidad de antFlow y Autopilot: Estado de Implementación Visible | ✅ Completado |
| **Fase 33** | [T33.2](T33.2-runtime-de-agente-bucle-de-herramientas-sobre-el-catalogo-de-capacidades.md) | Runtime de Agente: Bucle de Herramientas sobre el Catálogo de Capacidades | ✅ Completado |
| **Fase 33** | [T33.3](T33.3-roles-antflow-reales-sobre-el-runtime-de-agente.md) | Roles antFlow Reales sobre el Runtime de Agente | ✅ Completado |
| **Fase 33** | [T33.4](T33.4-la-barra-como-puesto-de-mando-pasos-en-vivo-aprobaciones-inline-y-tablero-real.md) | La Barra como Puesto de Mando: Pasos en Vivo, Aprobaciones Inline y Tablero Real | ✅ Completado |
| **Fase 33** | [T33.5](T33.5-evaluacion-reproducible-de-agentes-smoke-determinista-y-metricas.md) | Evaluación Reproducible de Agentes: Smoke Determinista y Métricas | ✅ Completado |
| **Fase 34** | [T34.1](T34.1-servicios-efimeros-reales-nix-sistema-y-adopcion-con-ollama-como-primer-caso.md) | Servicios Efímeros Reales: Backends Nix / Sistema / Adopción, con Ollama como Primer Caso | ✅ Completado |
| **Fase 34** | [T34.2](T34.2-ollama-como-motor-de-primera-clase-contexto-ciclo-de-vida-de-modelos-y-auto-local.md) | Ollama como Motor de Primera Clase: Contexto, Ciclo de Vida de Modelos y `auto` Local | ✅ Completado |
| **Fase 34** | [T34.3](T34.3-ollama-de-serie-en-la-imagen-nixos-services-ollama-solo-loopback.md) | Ollama de Serie en la Imagen NixOS: `services.antos.llm` sobre `services.ollama`, Solo Loopback | ⏳ Pendiente |
| **Fase 34** | [T34.4](T34.4-perfiles-local-hybrid-cloud-y-fiabilidad-de-los-roles-con-modelos-pequenos.md) | Perfiles `local` / `hybrid` / `cloud` y Fiabilidad de los Roles con Modelos Pequeños | ⏳ Pendiente |

---

## Fase 31 · Auditoría de Código (September 2026)

Revisión completa del árbol (`system/`, `kernel/`, `builder/`, `user/`) sin
modificar código en el momento de la auditoría. Los dieciséis tickets de
arriba recogen lo encontrado, agrupados en tres bloques:

- **Defectos de seguridad y corrección (T31.1 – T31.9).** Bypass de
  autenticación en la consola web (**✅ T31.1 resuelto**: token obligatorio
  con 401 real, entropía de 256 bits, solo el hash en disco, bind loopback
  por defecto) y su ciclo de vida (**✅ T31.2 resuelto**: `stop()` cierra el
  puerto de verdad, tope de 64 conexiones concurrentes, cabeceras leídas por
  completo, `decode_ws_frame` con aritmética comprobada), desbordamientos
  aritméticos en el cargador ELF del kernel y en tarfs (**✅ T31.3 resuelto**:
  aritmética de límites comprobada en `elf.rs`/`tarfs.rs`, revalidación local
  dentro de las funciones `unsafe`, mantenimiento de caché AArch64 línea a
  línea, `normalize_path` resuelve `..` sin escapar de la raíz, y
  `overflow-checks = true` activado en el perfil `release` del kernel),
  ejecución sin aislamiento en `vm exec` e inyección de shell en la detección
  de herramientas (**✅ T31.4 resuelto**: `vm exec` pasa por `sandbox::run`
  con la política más restrictiva, `env.rs` resuelve binarios sobre `PATH`
  sin intérprete alguno, T16.1 corregido con el estado real del
  aislamiento) e identidad de antMesh derivada de un hash no criptográfico
  (**✅ T31.5 resuelto**: identidad Ed25519 real vía nuevo `crate::crypto`
  sobre `ed25519-dalek`, clave privada `0600` separada de la identidad
  pública, tokens de emparejamiento con 128 bits de entropía y un solo uso,
  `md5_hash` retirada por completo, T9.1 corregido con el estado real del
  transporte) y secretos en claro en la bóveda (**✅ T31.6 resuelto**:
  `vault.json` cifrado con ChaCha20-Poly1305, clave AEAD separada en
  `vault.key`, escritura atómica unificada en `crypto::write_secret_file` y
  aplicada también a las sesiones web y la identidad de antMesh, `SecretValue`
  con `Debug` redactado y borrado de memoria al soltarse, migración
  transparente de bóvedas en claro previas) y pánicos por `unwrap`/`expect`
  en rutas del demonio (**✅ T31.7 resuelto**: nuevo `crate::util` con
  `lock_or_recover` aplicado a los ocho estados globales con cerrojo,
  `partial_cmp` sustituido por `total_cmp` en el profiler, direcciones de
  `ctx.rs` construidas sin *parsing*, `#![deny(clippy::unwrap_used,
  clippy::expect_used)]` a nivel de crate con paso dedicado en CI) y límites
  ausentes en el socket IPC (**✅ T31.8 resuelto**: lectura acotada a 4 MiB
  con `Event::Error` al superarla, `set_read_timeout`/`set_write_timeout`
  por conexión —liberado el de lectura tras la petición inicial, para no
  cortar una aprobación humana interactiva—, `bind` bajo `umask` restrictiva
  sin ventana de permisos, serialización de conexiones documentada como
  deliberada en la cabecera del módulo) y en el intérprete WASM (**✅ T31.9
  resuelto**: `checked_add` en `read_memory`/`write_memory` y en el cómputo
  de dirección de `i32.load`/`i32.store*`, `saturating_add` en
  `consume_fuel`, nuevo `Trap::MalformedModule` para bytecode truncado e
  índices de función/tipo fuera de rango en `call_function`, corpus de seis
  módulos malformados verificado con `catch_unwind`).
- **Integración continua (T31.10, T31.16).** Las puertas de `clippy` y
  `rustfmt` de T22.6 fallaban sobre el árbol actual, así que la CI llevaba
  tiempo en rojo (**✅ T31.10 resuelto**: 170 ficheros reformateados en un
  commit aislado; 97 avisos de clippy resueltos en el host —dos con
  `unsafe`/`Result<_, ()>` reales corregidos con intención, el resto
  mecánico— y 58 en el kernel para ambos objetivos —15 secciones `# Safety`
  escritas de verdad y 3 `Result<_, ()>` con error nombrado o eliminados
  por no tener ningún camino de fallo—; cerrados los huecos de cobertura de
  `system/barra` y `user/`, con un job de CI dedicado para cada uno; nota
  anti-regresión añadida al encabezado del propio catálogo). Los tests del
  workspace del anfitrión (383 a la fecha de T31.10) y las compilaciones del
  kernel para ambas arquitecturas sí pasan. Aparte, los 86 tests escritos en
  el kernel no compilan: falta el arnés `no_std` y no hay trabajo de CI que
  los ejecute.
- **Mejoras estructurales (T31.11 – T31.15).** Retirada de la capa `syso` y de
  los 45 símbolos deprecados (**✅ T31.11 resuelto**: transición suave para
  variables de entorno vía nuevo `env_with_legacy_fallback` —`SYSO_*` con
  aviso de obsolescencia, no en silencio—, migración real de una sola vez
  para rutas en disco vía `migrate_legacy_path` —`.syso/` → `.antos/`,
  claves en `~/.config/syso/` → `~/.config/antos/`, ficheros de proyecto
  `syso.packages.toml`/`syso-paquetes.nix` → `antos.packages.toml`/
  `antos-paquetes.nix`, este último renombrado también en disco—,
  `POLICY_ENV` a `ANTOS_SANDBOX_POLICY` sin alias, los 45 símbolos
  deprecados eliminados y sus 8 usos internos en tests de `flow.rs`
  reescritos contra la API real; de paso, corregido un manifiesto de
  capacidad —`pkg.declare.toml`/`system.declare.toml`— que declaraba
  `writes` contra los nombres de fichero viejos y habría hecho que el
  recinto denegara la escritura real), finalización de la nomenclatura en
  inglés (**✅ T31.12 resuelto**: patrón sistémico de `pub const
  NombreEspañol: Self = Self::VarianteInglesa` encontrado y eliminado por
  completo en `TicketStatus`/`FlowState`/`AgentRole` —invisible para
  T31.11 al no llevar `#[deprecated]`—, `Pendiente`→`PendingChanges` y
  `Propuesta`→`Proposal` renombrados con sus ~165 sitios de uso,
  `spec.rs` reescrito íntegro como el ejemplo que el propio ticket citaba,
  cuatro `.nix` de `system/nixos/` renombrados, nueva regla en
  `antos-development.md` fijando que comentarios y texto de cara al
  usuario siguen en español, y nuevo guion
  `.agents/scripts/check-spanish-identifiers.py` con su propio job de CI,
  verificado en vivo para que falle ante un identificador nuevo en
  español) y cobertura de tests de la capa CLI y de la barra (**✅ T31.13
  resuelto parcialmente**: analizador de argumentos extraído a funciones
  puras y probado en los seis puntos de entrada con mayor riesgo —indexado
  manual `args[i]`/`i + 1 < args.len()` citado por el propio ticket, los 15
  sitios de `system.rs` (`cmd_bootloader`, `cmd_vm`, `cmd_autopilot`,
  `cmd_web`) más un ejemplo de `tools.rs` (`cmd_quota`)—, 22 tests nuevos;
  `is_granted` de `grants.rs` separado en un núcleo con el reloj inyectado
  y 10 tests de sus rutas de decisión; `sandbox/mod.rs` con 10 tests de
  `Policy::from_blast`/`with_grants`; 4 tests de la lógica pura de
  `antos-barra` (`truncate_str`, `level_css_class`) añadidos sin poder
  compilarse en esta máquina —sin GTK4 en macOS—, con un paso de `cargo
  test` añadido al job de CI de Linux para verificarlos ahí; quedan
  diferidos y documentados en el propio ticket: el resto de los 43
  subcomandos del CLI, la extracción de más lógica no-GTK de la barra, y
  el trabajo de medición de cobertura, que el propio ticket ya marcaba
  como opcional) y alineación de las cabeceras de módulo con lo realmente
  implementado (**✅ T31.14 resuelto**: `ebpf.rs` reescrito para admitir
  que su panel de telemetría es un `VecDeque` en memoria sin ningún
  programa eBPF cargado —cero dependencia de `libbpf`/`aya`, el único
  productor de eventos es `simulate_violation`—, nuevo
  `EbpfStatus::backend: Simulated | LinuxBpf` que el CLI muestra sin
  ambigüedad en vez del anterior «KERNEL LSM ACTIVO (BPF Enforcing)»
  incondicional; `collab.rs` separado en su CRDT real —un RGA propio con
  desempate determinista, no una fachada— y su `DapServer`, que resultó
  ser una maqueta completa: no lanza el comando que recibe ni ningún
  depurador, la pila de llamadas y las variables son una lista fija de
  ejemplo, `add_breakpoint` marca `verified: true` siempre porque no hay
  nada que pudiera rechazarlo — nuevo `DapSessionStatus::simulated: bool`
  y aviso explícito en `antos debug`; `profiler.rs` documentado con la
  misma distinción: `duration_ms`/`cpu_*_ms`/`peak_memory_bytes`/
  `page_faults` son reales vía `getrusage(2)` cuando funciona —nuevo
  `ProfileReport::metrics_are_real`, en vez de la estimación de respaldo
  silenciosa que antes se servía sin distinguir—, mientras que
  `hotspots` resultó ser una tabla fija elegida por subcadena del
  comando (`"test"`/`"build"`/nada), nunca muestreo real, documentado
  así en `ProfileHotspot`; nueva regla en `antos-development.md` fijando
  que una cabecera describe el comportamiento actual, nunca lo
  aspiracional, con un campo explícito de backend/simulación cuando
  ambos coexisten en el mismo módulo. `mesh.rs`/`vm.rs` ya habían
  recibido esta misma corrección en T31.5/T31.4 respectivamente;
  `sandbox/seatbelt.rs` y `sandbox/landlock.rs` sirvieron de referencia,
  tal como los cita el propio ticket, de cómo se ve una cabecera
  honesta desde el principio. El resto de las ~90 cabeceras de módulo del
  árbol no se re-auditó una por una en esta pasada más allá de un barrido
  heurístico por vocabulario de riesgo —«cifrado», «aislado», «kernel»,
  «tiempo real»— que no encontró más casos tan flagrantes como los de
  arriba), y descomposición de los ficheros que han vuelto a superar las
  1500 líneas (**✅ T31.15 resuelto parcialmente**: siete commits de
  movimiento puro de código —sin tocar ningún `match` de despacho de una
  sola función gigante, así que ninguno cambia comportamiento observable,
  verificado con el mismo número de tests, 434, antes y después de cada
  uno— resuelven `cli/commands/tools.rs` (3592 líneas → 11 ficheros de
  <800), `cli/commands/system.rs` (2065 → 7 ficheros), `pkg.rs` (1963 →
  `pkg/{recipes,desktop,lifecycle,tests}.rs`), `protocolo/src/tests.rs`
  (2211 → 4 bloques, no estaba en la tabla del ticket pero creció por
  encima del umbral igual) y la extracción de tests de `planner/local.rs`
  (2858 → `local/mod.rs` 1837 + `local/tests.rs`); `exec/mod.rs` (3699 →
  3047) y `ipc.rs` (2607 → `ipc/mod.rs` 2417 + `ipc/transport.rs`, este
  último separando transporte de despacho como pedía el propio ticket)
  quedan con sus dos funciones únicas y enormes de despacho
  (`changes_for`/`apply`, `handle_connection`/`remote_intent`)
  deliberadamente sin partir —reescribir cada brazo de esos `match` es un
  trabajo de riesgo comparable a un ticket propio, no movimiento
  mecánico—, y `planner/local/mod.rs` (1837) y `barra/main.rs` (1528, sin
  poder compilarse en esta máquina) quedan igual de intactos; umbral de
  800/1500 líneas documentado en `antos-development.md`) y los 86 tests
  del kernel que nunca se habían ejecutado (**✅ T31.16 resuelto**: el
  hallazgo del propio arranque —`#[test]` sigue exigiendo el crate `test`
  incluso con `custom_test_frameworks` activo; hacen falta
  `#[test_case]`— explica por qué los tres atributos por sí solos no
  bastaban; los 98 tests que había para entonces (86 a la fecha del
  ticket) se renombraron mecánicamente y corren de verdad en QEMU vía
  `kernel/run-tests-{x86_64,aarch64}.sh`, nuevo job `kernel-tests` en CI
  para ambas arquitecturas, `peripheral-smoke` intacto; de los tres
  fallos reales que aparecieron al ejecutarlos por primera vez, dos eran
  aserciones desincronizadas de una corrección de escalado de puntero ya
  hecha y documentada en el propio código, y uno era un bug genuino de
  producción — el heap de AArch64 (`static mut [u8; N]`) no garantizaba
  la alineación de 8 bytes que el asignador de lista enlazada exige,
  corregido con `#[repr(align(16))]`; ningún test quedó en `#[ignore]`).

Con T31.16 se completan los dieciséis tickets originales de la Fase 31.

- **La VM de antOS NixOS no se construía (T31.17).** Verificar la VM gráfica de
  antOS Linux (T30.2) destapó **dos roturas latentes**, ambas en el camino de
  compilación de Nix, que **ninguna prueba de CI recorre** — la CI solo
  compila el workspace del anfitrión con `cargo` sobre el árbol entero
  (**✅ T31.17 resuelto**):
  1. **`antos-barra` no enlazaba:** cinco funciones movían widgets de `gtk4`
     (`Label`, `Box`, `ApplicationWindow`) y punteros `Rc<RefCell<…>>` dentro
     de closures pasados a `std::thread::spawn`, que exige `Send`. Helper
     `run_offthread(work, apply)` —el hilo solo hace I/O de socket y devuelve
     datos `Send`; un `timeout_add_local` en el hilo principal consume el
     `Receiver` y toca la interfaz, el mismo patrón que
     `dispatch_intent`/`listen_events`— más el arreglo de un `E0593`. 9
     errores procedentes de T4.1, T13.1 y T25.4. Los trabajos de CI
     `antos-linux-desktop` y `clippy-barra` llevaban en rojo desde entonces
     sin bloquear cierres; los tests de barra de T31.13 se ejecutan por
     primera vez.
  2. **El paquete `antosd` no compilaba:** `system/nixos/package.nix` pasó en
     T31.12 a un `fileset` explícito de `src` que dejó fuera `recipes/` y
     `system/desktop/rc.xml`, justo lo que `antosd` embebe con `include_str!`
     (12 `error: couldn't read …`). Añadidos al conjunto. Vivo desde T31.12;
     invisible porque no hay job que construya por Nix — queda anotado como
     ticket propio.

- **El módulo NixOS no arrancaba el demonio (T31.18).** Con la VM ya
  construyéndose y arrancando al escritorio (T31.17), `antos-barra` mostraba
  `no hay demonio antOS en /var/lib/antos/estado/antos.sock: Permission
  denied`. `system/nixos/module.nix` solo definía el `oneshot` `antos-doctor`;
  **no había ningún `systemd.services.antos`** que ejecutara `antos demonio`
  (el que hace `ipc::serve`), y el directorio de estado era `0700 root` frente
  a un `antos-barra` que corre como el usuario de autologin (**✅ T31.18
  resuelto**: nuevo servicio `antos` con opción `services.antos.user` —el
  socket es `0600` por T31.8, así que el demonio corre como el usuario de la
  sesión Wayland, que `desktop.nix` fija solo; `workspace`/`state` pasan a ser
  de ese usuario. Headless con `user = "root"`: sin cambios. Verificado en la
  VM: `antos.service active (running)`, socket `srw------- antos`).

- **La CI no construía por Nix (T31.19).** El hueco que dejó pasar T31.17 y
  T31.18: `ci.yml` compilaba el workspace del anfitrión con `cargo` y el árbol
  entero, pero ningún job hacía `nix build` del paquete `antosd`, de
  `antos-barra` ni del sistema NixOS (**✅ T31.19 resuelto**: nuevo job
  `nix-build` que hace `nix flake check` y `nix build` de `.#antosd`,
  `.#antos-barra` y el `toplevel` de `antos-vm` y `antos-desktop-vm`;
  `cache.nixos.org` sirve prehecho casi todo, así que descarga mucho y compila
  poco. Habría fallado ante T31.17 y ante un `fileset` de `package.nix`
  incompleto. `.#iso` queda para un job *nightly*).

Además, en Fase 30: **escritorio tradicional sobre Labwc (T30.6)** —opción
`services.antos.desktop.panel` con `waybar`/`fuzzel`/`swaybg`/`mako` y menú de
clic derecho, sin dejar de ser una sesión `wlroots` ligera; el propio arranque
destapó que labwc no se sostiene bajo TCG (`libseat`/`logind` timeout) y que un
reinicio de `greetd` apilaba clientes, corregido con un `autostart`
idempotente— y **arranque acelerado en macOS (T30.7)**:
`system/arrancar-vm-macos.sh` construye la ISO en vivo en el contenedor y la
arranca con `qemu -accel hvf` **en el host**, donde el invitado AArch64 va casi
a velocidad nativa y la sesión Wayland arranca en segundos. Verificado: la ISO
arranca por HVF hasta consola con `antos-doctor` pasando y el demonio corriendo.

Orden sugerido de ataque: ~~T31.1~~ → ~~T31.2~~ → ~~T31.3~~ → ~~T31.4~~ →
~~T31.5~~ → ~~T31.6~~ → ~~T31.7~~ → ~~T31.8~~ → ~~T31.9~~ → ~~T31.10~~ →
~~T31.11~~ → ~~T31.12~~ → ~~T31.13~~ → ~~T31.14~~ → ~~T31.15~~ →
~~T31.16~~ → ~~T31.17~~ → ~~T31.18~~ → ~~T31.19~~. Fase 31 completa.

---

## Fase 32 · Auditoría y Optimización de Rendimiento del Sistema (September 2026)

Revisión técnica de cuellos de botella de latencia, contención de cerrojos, fragmentación de memoria, sobrecarga de E/S y operaciones cuadráticas en el kernel bare-metal y el espacio de usuario (`system/antosd`, `system/barra`).

- **Memoria y renderizado en Kernel (T32.1 – T32.2):**
  - **✅ T32.1 resuelto:** Coalescencia contigua (izquierda, derecha y sándwich) y ordenación física en el asignador de lista enlazada; métricas $O(1)$ (`allocated_bytes`, `used()`, `free()`) y reciclaje diferido de pilas de kernel (16 KiB) en el planificador preemptivo tras el cambio de contexto para frenar fragmentación y OOM.
  - **✅ T32.2 resuelto:** Aceleración de trazado en VRAM de 128 a 16 operaciones base por glifo en `draw_char`; llenado contiguo por fila en `draw_rect`; seguimiento de rectángulos dañados (*dirty rectangles*) y eliminación de flushes de pantalla completa VirtIO en cada `newline` en AArch64; borrado y llenado rápido con `slice::fill` en `Surface`.
- **Concurrencia y E/S en Demonio e Interfaz (T32.3 – T32.5):**
  - **✅ T32.3 resuelto:** Servidor IPC concurrente despachado en hilos dedicados y desacoplamiento de mutaciones con `WORKSPACE_MUTATION_LOCK`; consultas de solo lectura en <50 ms sin congelamiento por sesiones interactivas; eliminación de la doble serialización en `SocketHandler::on_result`.
  - **✅ T32.4 resuelto:** Telemetría en RAM sin lecturas de disco síncronas en `antosd` (<1 ms); `VecDeque` para inserción/desalojo $O(1)$ de alertas; eliminación del bucle de sondeo activo a 80 ms en `system/barra` reemplazado por un canal reactivo (`async_channel` consumido con `glib::spawn_future_local`; `glib::MainContext::channel` ya no existe en glib-rs ≥ 0.19).
  - **✅ T32.5 resuelto:** Unificación de inspección Git con `status --porcelain=v2 --branch` (reducción de subprocesos de 4 a 1 en caso general); caché consistente con `WorktreeSignature` que detecta inmediatamente modificaciones en el working tree sin `git add`; memoización $O(1)$ de `detect_antos_root()`.
- **Almacenamiento, Estructuras de Datos y Zero-Copy (T32.6 – T32.8):**
  - **✅ T32.6 resuelto:** Eliminación de busy-waiting de millones de iteraciones de CPU en controladores NVMe y AHCI SATA; cesión de CPU (`io_wait`), soporte para suspensión de hilos (`ThreadState::Blocked`) en el planificador preemptivo y timeouts calibrados por ticks de temporizador.
  - **✅ T32.7 resuelto:** Reducción de la inserción en el grafo de contexto de $O(E^2)$ a $O(1)$ amortizado y consultas $O(\text{grado})$; diccionario léxico global compacto (`TermDictionary`) para vectores semánticos dispersos `Vec<(u32, f32)>` con producto escalar lineal sin asignación en heap; eliminación de asignaciones intermedias y búsqueda binaria en arrays estáticos para el visor de diffs sintáctico.
  - **✅ T32.8 resuelto:** Lectura zero-copy con `Cow<'static, [u8]>` para ficheros de memoria en VFS sin asignación en heap; soporte de instantáneas CoW (`FICLONE` / reflink) en Linux para ficheros y directorios; paralelización de stages concurrentes en CI local con `std::thread::scope`.

Orden sugerido de ataque: ~~T32.1~~ → ~~T32.2~~ → ~~T32.3~~ → ~~T32.4~~ → ~~T32.5~~ → ~~T32.6~~ → ~~T32.7~~ → ~~T32.8~~. Fase 32 completa.

---

## Fase 33 · Agentes que Construyen Software (September 2026)

Revisión del estado del proyecto frente a su propósito —«sistema operativo
declarativo para construir software con agentes desde la barra»— hecha el
2026-09-16 contrastando el backlog (130 tickets ✅) con el código.

**Lo que funciona de verdad:** el ciclo *intención → planificador (Claude /
Ollama / OpenAI-compat / reglas locales) → catálogo de 122 capacidades
tipadas (`system/capabilities/`) revalidado → radio de impacto y `tier` →
sandbox Landlock/Seatbelt → snapshot → ejecución → journal → `undo`*, con la
barra y el CLI como interfaces (`session::intent_session`). Demonio, IPC,
memoria semántica, parser de tickets, LSP, forja, escritorio (Plasma/Labwc),
ISO y userland están sólidos.

**El hueco:** antFlow es una máquina de estados sin cerebro. En
`FlowEngine::run_worktree_pipeline` el «Coder» escribe un *scaffold* fijo,
el «Arquitecto» solo cuenta criterios y los modelos por rol son etiquetas;
`Autopilot::generate_fix` equilibra llaves. Los únicos puntos donde participa
un modelo son los planificadores, de un solo turno. No existe un **bucle de
agente** (leer → razonar → editar → probar → corregir). La arquitectura ya
tiene la forma correcta —las capacidades son las herramientas del agente,
con sus tiers y su sandbox— y solo falta el runtime que las pone en manos
de un modelo con presupuesto y supervisión.

- **T33.1** — honestidad primero (regla T31.14): cabeceras, `FlowBackend::
  Simulated | Agent` por IPC, tablero y CLI que no confunden simulación con
  agente.
- **T33.2** — runtime `agent::AgentRun`: conversación multi-turno con
  `tool_use` (Claude / Ollama / OpenAI-compat) cuyo *toolset* es un
  subconjunto del catálogo (`fs.read`, `fs.list`, `fs.patch` nuevo,
  `fs.write`, `test.run` nuevo, `git.status`, `memory.search`), cada paso
  por validación → blast → aprobación → snapshot → `exec` → journal;
  presupuesto; proveedor `fake` determinista para CI. Sin `sh -c` (T31.4).
- **T33.3** — Arquitecto (plan tipado), Coder (bucle en el worktree), QA
  (tests reales realimentando fallos), Auditor (veredicto tipado) sobre el
  runtime, con modelos por rol de `LlmConfig`; Autopilot pasa a usar el
  runtime o solo notifica.
- **T33.4** — la barra como puesto de mando: pasos en vivo, aprobaciones
  inline con diff, tablero con progreso y coste reales, Super+Space en
  Plasma.
- **T33.5** — evaluación reproducible: smoke determinista en CI con `fake`
  y `antos eval agent --live` con métricas y diff de regresiones.

Orden sugerido de ataque: ~~T33.1~~ → ~~T33.2~~ → ~~T33.3~~ → ~~T33.4~~ → ~~T33.5~~. Fase 33 completa.

---

## Fase 34 · Modelos Locales de Serie (September 2026)

Revisión hecha el 2026-09-17 a partir de una pregunta simple: ¿puede un
antOS recién instalado, sin ninguna clave de API, hacer todo lo que la
Fase 33 promete usando solo modelos gratuitos en la propia máquina?

**Lo que ya hay:** proveedores Ollama (`/api/chat`) y OpenAI-compatible
(llama.cpp / OpenCode) para planificador y runtime de agentes, `LlmConfig`
con modelo por rol, detección de Ollama al arrancar (`ctx.local_llm`), y la
primera sesión real con `qwen2.5-coder:7b` (T33.5 §3): pipeline completo
en 33 s, fiabilidad 2/5.

**Los huecos, contrastados con el código:**

- `antos service up postgres|redis` **no arranca nada**: `service::
  start_service` escribe un `service.json` con `status: running` y
  `pid: None` e inyecta en el `.env` una URL a un puerto vacío. Es una
  maqueta sin marcar (T31.14), y es justo el mecanismo por el que Ollama
  debería ser «un servicio más».
- Ollama **no está en la imagen** NixOS pese al título de T19.3; la receta
  antpkg es solo `arm64`, 0.5.7, con firma de relleno.
- `OllamaAgentProvider` no envía `num_ctx`: Ollama corta la conversación a
  4096 tokens **por el principio** (prompt de sistema y herramientas), lo
  que explica parte del 2/5.
- Los modelos por rol por defecto apuntan a OpenRouter y Groq; `auto` hace
  `bail!` para agentes. Sin claves, no hay pipeline.

- **T34.1** — servicios efímeros reales: `ServiceKind` con backends
  `External` (adopta lo que ya escucha), `System` (binario en `PATH`) y
  `Nix` (`nix run nixpkgs#…`, argumentos como vector), PID, log, sonda de
  salud y `backend`/`healthy` explícitos. Ollama, PostgreSQL y Redis
  verificados; el `.env` solo se toca cuando la sonda pasa.
- **T34.2** — Ollama de primera clase: `num_ctx`/`temperature`/`keep_alive`
  y tokens reales en el proveedor de agente; `antos llm pull|rm|doctor`
  por la API HTTP; `setup` que arranca el servicio y descarga el modelo
  recomendado por RAM; `auto` → Ollama cuando está sano.
- **T34.3** — `services.antos.llm` en la imagen NixOS sobre
  `services.ollama` de nixpkgs (cero dependencias nuevas), solo loopback,
  sin descargas en la activación; la receta antpkg deja de fingir.
- **T34.4** — perfiles `local` (por defecto en instalación limpia) /
  `hybrid` / `cloud`; `format` con JSON Schema para plan y veredicto,
  toolsets compactos para modelos pequeños, reintento dirigido ante
  argumentos malformados; `eval --live --repeat N` y job nocturno para
  medir con n>1 antes de tocar prompts.

Orden sugerido de ataque: ~~T34.1~~ → ~~T34.2~~ → T34.4 → T34.3 (la imagen se
verifica solo en CI/VM; lo demás se prueba en el Mac con Ollama.app).
