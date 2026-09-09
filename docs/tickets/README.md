# antOS · Catálogo y Hoja de Ruta de Tickets

Este directorio contiene el desglose técnico y ordenado de tareas para transformar el sistema en **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa.

> **Dos vías a partir de la Fase 29:**
> - **antOS Linux (driver diario, Fase 30):** NixOS + sesión Wayland + `antos-barra` + userland de desarrollo (Neovim, Git, `antos dev`). Es el sistema pensado para usarse a diario; corre software existente porque es Linux.
> - **Kernel bare-metal (I+D de soberanía, Fase 29 y sucesivas):** el núcleo `no_std` propio (`kernel/` + `user/`), con su compositor 2D y su shell. Vía de investigación en paralelo; no ejecuta software POSIX.
> El CLI `antos` y `antos-barra` (`system/`) son compartidos por la vía Linux y por el host.

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
| **Fase 31** | [T31.1](T31.1-bypass-de-autenticacion-y-tokens-predecibles-en-la-consola-web.md) | Bypass de Autenticación y Tokens Predecibles en la Consola Web Remota | ✅ Completado |
| **Fase 31** | [T31.2](T31.2-ciclo-de-vida-y-limites-de-recursos-del-servidor-de-consola-web.md) | Ciclo de Vida y Límites de Recursos del Servidor de Consola Web | ✅ Completado |
| **Fase 31** | [T31.3](T31.3-desbordamientos-aritmeticos-en-cargador-elf-y-tarfs-del-kernel.md) | Desbordamientos Aritméticos en el Cargador ELF y en tarfs del Kernel | ✅ Completado |
| **Fase 31** | [T31.4](T31.4-aislamiento-real-de-microvm-y-eliminacion-de-inyeccion-de-shell.md) | Aislamiento Real de MicroVM y Eliminación de Inyección de Shell | ✅ Completado |
| **Fase 31** | [T31.5](T31.5-identidad-criptografica-real-en-antmesh.md) | Identidad Criptográfica Real y Tokens de Emparejamiento en antMesh | ✅ Completado |
| **Fase 31** | [T31.6](T31.6-cifrado-en-reposo-y-escritura-atomica-de-la-boveda-de-secretos.md) | Cifrado en Reposo y Escritura Atómica de la Bóveda de Secretos | ✅ Completado |
| **Fase 31** | [T31.7](T31.7-eliminacion-de-panicos-por-unwrap-en-rutas-del-demonio.md) | Eliminación de Pánicos por `unwrap` en Rutas del Demonio | ✅ Completado |
| **Fase 31** | [T31.8](T31.8-limites-y-tiempos-de-espera-en-el-socket-ipc-del-demonio.md) | Límites y Tiempos de Espera en el Socket IPC del Demonio | ✅ Completado |
| **Fase 31** | [T31.9](T31.9-comprobacion-de-limites-del-interprete-wasm.md) | Comprobación de Límites del Intérprete WebAssembly | ✅ Completado |
| **Fase 31** | [T31.10](T31.10-reparacion-de-la-ci-en-rojo-clippy-y-rustfmt.md) | Reparación de la CI en Rojo: Puertas de Clippy y rustfmt | ⏳ Pendiente |
| **Fase 31** | [T31.11](T31.11-retirada-de-la-capa-de-compatibilidad-syso.md) | Retirada de la Capa de Compatibilidad `syso` y de los Símbolos Deprecados | ⏳ Pendiente |
| **Fase 31** | [T31.12](T31.12-finalizacion-de-la-nomenclatura-en-ingles.md) | Finalización de la Estandarización de Nomenclatura en Inglés | ⏳ Pendiente |
| **Fase 31** | [T31.13](T31.13-cobertura-de-tests-de-la-capa-cli-y-de-antos-barra.md) | Cobertura de Tests de la Capa CLI y de `antos-barra` | ⏳ Pendiente |
| **Fase 31** | [T31.14](T31.14-alineacion-de-la-documentacion-de-modulos-con-la-implementacion-real.md) | Alineación de la Documentación de Módulos con la Implementación Real | ⏳ Pendiente |
| **Fase 31** | [T31.15](T31.15-descomposicion-de-modulos-de-gran-tamano.md) | Descomposición de Módulos de Gran Tamaño | ⏳ Pendiente |
| **Fase 31** | [T31.16](T31.16-los-tests-del-kernel-no-se-compilan-ni-se-ejecutan.md) | Los 86 Tests del Kernel No Se Compilan ni Se Ejecutan | ⏳ Pendiente |

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
  `rustfmt` de T22.6 fallan sobre el árbol actual, así que la CI lleva tiempo
  en rojo. Los 318 tests del workspace del anfitrión y las compilaciones del
  kernel para ambas arquitecturas sí pasan. Aparte, los 86 tests escritos en
  el kernel no compilan: falta el arnés `no_std` y no hay trabajo de CI que
  los ejecute.
- **Mejoras estructurales (T31.11 – T31.15).** Retirada de la capa `syso` y de
  los 45 símbolos deprecados, finalización de la nomenclatura en inglés,
  cobertura de tests de la capa CLI y de la barra, alineación de las cabeceras
  de módulo con lo realmente implementado, y descomposición de los ficheros que
  han vuelto a superar las 1500 líneas.

Orden sugerido de ataque: ~~T31.1~~ → ~~T31.2~~ → ~~T31.3~~ → ~~T31.4~~ →
~~T31.5~~ → ~~T31.6~~ → ~~T31.7~~ → ~~T31.8~~ → ~~T31.9~~ **→ T31.10 →
T31.16**, y el resto según convenga. T31.10 y T31.16 conviene abordarlos
pronto: sin CI verde y sin tests de kernel ejecutables, las correcciones de
T31.3 no tienen forma de verificarse.
