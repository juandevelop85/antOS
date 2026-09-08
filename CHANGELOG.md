# Registro de Cambios de antOS (CHANGELOG)

Todos los cambios notables de este proyecto están documentados en este archivo.
El formato se basa en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/) y este proyecto se adhiere a [Semantic Versioning](https://semver.org/lang/es/).

---

## [No Publicado] · Fases 15-28

Desarrollo posterior al lanzamiento v0.1.0: instalación en disco físico y dual-boot, virtualización con microVMs, gestor de paquetes inmutable `antpkg`, modo agente autónomo, aislamiento multi-proyecto en el workspace, portado completo del kernel a AArch64 (ARM 64-bit), ecosistema multi-LLM, entorno de desarrollo TUI integrado, benchmarking e integración con forjas Git, modularización interna y CI/CD multiplataforma, un kernel `x86_64` con multitarea preemptiva real, un **escritorio nativo bare-metal** con compositor 2D, gestión de aplicaciones gráficas (Flatpak/XDG) y un **runtime y shell de espacio de usuario soberanos** (`libantos` / `antos-init`) que arrancan como PID 1 sin `glibc`/`musl`; y, en las Fases 27-28, **arranque Limine real por UEFI**, **pila PCIe/USB xHCI y HID bare-metal**, **periféricos nativos a paridad entre x86_64 y AArch64** (VirtIO-Input, GIC v2/v3, timer resiliente, `virtio-gpu-pci`/`ramfb`, descubrimiento por DTB/ACPI) más una iteración de **endurecimiento runtime** que hace arrancar a antOS en **VirtualBox ARM64** y **UTM** hasta el shell interactivo. 65 tickets técnicos adicionales, todos completados al 100% (ver el backlog íntegro en [`docs/tickets/README.md`](docs/tickets/README.md)).

#### 🔹 Fase 15: Instalación en Disco Completo y Dual Boot UEFI
- **[T15.1] Motor de Inspección de Almacenamiento y Particionador GPT:** Descubrimiento y diagnóstico tipado de discos físicos y virtuales (NVMe, SATA/SSD, virtio) y particionamiento GPT seguro.
- **[T15.2] Instalador Guiado de Sistema Base (Disco Completo y Dual Boot):** Motor de instalación que transfiere antOS desde un medio Live hacia el disco de destino, con modos Disco Completo y Dual-Boot.
- **[T15.3] Gestor de Arranque UEFI y Dual Boot Automatizado:** Configuración del gestor de arranque UEFI nativo con detección automática de otros sistemas operativos instalados en el equipo.

#### 🔹 Fase 16: MicroVMs, Gestor de Paquetes Inmutable y Modo Autónomo
- **[T16.1] MicroVMs Efímeras y Aislamiento por Hipervisor (KVM/Cloud-Hypervisor):** Aislamiento por hipervisor para tareas riesgosas o pruebas de agentes con permisos elevados.
- **[T16.2] Gestor de Paquetes y Recetas Inmutables (`antpkg`):** Instalación, desinstalación y verificación de binarios mediante un almacén direccionado por contenido (CAS) con rollback por generaciones.
- **[T16.3] Modo Agente Autónomo Continuo (*Autopilot Daemon*):** El equipo `antFlow` opera en segundo plano de forma continua y desatendida, vigilando el repositorio y monitorizando fallos.
- **[T16.4] Consola Web Remota en Tiempo Real y Bridge WebSocket:** Panel web ultraligero embebido en `antosd` para supervisar y controlar antOS desde cualquier navegador.

#### 🔹 Fase 17: Aislamiento y Gestión Declarativa Multi-Proyecto en el Workspace
- **[T17.1] Aislamiento de Frontera Git y Descubrimiento Contextual de Workspace:** Aislamiento estricto entre el repositorio de antOS y los proyectos del usuario (`$WORKSPACE`) vía `GIT_CEILING_DIRECTORIES`.
- **[T17.2] Soporte Multi-Proyecto en Visor de Diffs y Estado de Workspace:** `antos diff` y la capacidad `ui.diff_viewer` operan por proyecto en lugar de asumir un único repositorio global.
- **[T17.3] Inicialización y Gestión Declarativa de Proyectos Git en Workspace:** Comandos para inicializar y gestionar repositorios Git dentro de `workspace/<proyecto>` con rama principal y `.gitignore` por stack.
- **[T17.4] Catálogo y Gestión de Tickets Desacoplados por Proyecto en Workspace:** Cada proyecto del workspace obtiene su propio catálogo de especificaciones y tickets, aislado del backlog del sistema.

#### 🔹 Fase 18: Portado del Kernel a AArch64 (ARM 64-bit) Bare Metal
- **[T18.1] Abstracción de Capa de Hardware (HAL) y Desacoplamiento de Arquitectura en Kernel:** Separación del código dependiente de x86_64 respecto de los componentes agnósticos de arquitectura.
- **[T18.2] Arranque AArch64, Consola Serie PL011 y Vectores de Excepción VBAR_EL1:** Soporte de arranque para `aarch64-unknown-none` con driver de consola serie PL011.
- **[T18.3] Paginación AArch64 (TTBR0/TTBR1) y Controlador de Interrupciones GIC:** MMU y paginación multinivel (`TCR_EL1`) más el controlador de interrupciones GIC.
- **[T18.4] Llamadas al Sistema (SVC) en AArch64 y Generación de Imágenes UEFI (BOOTAA64.EFI):** Transición a EL0 y despacho de syscalls vía `svc #0`; imágenes de arranque UEFI para ARM64.

#### 🔹 Fase 19: Ecosistema Multi-LLM y Catálogo de Modelos Gratuitos
- **[T19.1] Motor de Inferencia OpenAI-Compatible y Hub de Proveedores Gratuitos:** Planificador universal capaz de hablar con cualquier servidor `/v1/chat/completions` (Groq, OpenRouter, Gemini, OpenCode…).
- **[T19.2] Gestión de Configuración Persistente, Selección Dinámica y Catálogo de LLMs Gratuitos:** El usuario consulta, alterna y gestiona dinámicamente el motor de inferencia activo del sistema.
- **[T19.3] Detección e Instalación de Ollama y OpenCode por Defecto con Receta antpkg:** Detección proactiva de motores locales en arranque e instalación asistida vía recetas `antpkg`.
- **[T19.4] Enriquecimiento del Equipo Multi-Agente antFlow y Asignación de Modelos por Rol:** Cada rol de `antFlow` (Arquitecto, Coder, QA, Auditor) puede usar un motor y modelo LLM distinto.

#### 🔹 Fase 20: Entorno de Desarrollo TUI, TDD Autónomo, CI Local y Snapshots
- **[T20.1] Espacio de Trabajo Integrado Dev TUI con Neovim, Monitor de Agentes y Visor de Diffs (`antos dev`):** Multiplexor de terminal interactivo (`Super + W`) que combina edición de código, monitor de agentes y diffs.
- **[T20.2] Reproductor Autónomo de Bugs y Generador de Tests de Regresión TDD:** Dado un stack trace o mensaje de panic, reproduce el bug y genera pruebas de regresión (`antos reproduce` / `antos testgen`).
- **[T20.3] Matriz de CI/CD Local Paralela en Sandboxes y Pre-Commit Hooks del Auditor:** Integración continua local ultrarrápida y paralela (`antos ci`) con Git Hooks auditados por agentes (`antos hook`).
- **[T20.4] Snapshots Atómicos de Entorno de Desarrollo y Time Machine de Estado:** Congela y restaura en menos de 200 ms el estado completo del entorno de trabajo (`antos snapshot`).

#### 🔹 Fase 21: Benchmarking Continuo, Forjas Remotas y Documentación Viva
- **[T21.1] Benchmarking Continuo y Detección de Regresiones de Rendimiento en Worktrees:** Mide el impacto de cambios de código antes de fusionarlos a las ramas principales (`antos bench`).
- **[T21.2] Sincronización Bidireccional con Forjas Git: Issues a Tickets y Pull Requests:** Integración con GitHub y GitLab: importa Issues como tickets y sincroniza Pull Requests (`antos issue` / `antos pr`).
- **[T21.3] Generador y Sincronizador de Documentación Viva de Arquitectura y Diagramas Mermaid:** Documentación de arquitectura permanentemente sincronizada con el código fuente (`antos doc arch`).

#### 🔹 Fase 22: Modularización Interna, ABI Kernel-Userspace y CI/CD Multiplataforma
- **[T22.1] Modularización y Desacoplamiento de `antosd` en CLI y Subcomandos:** Descompone `main.rs`/`exec.rs` (más de 6.400 líneas) en `cli/commands/*` por subsistema.
- **[T22.2] Descomposición Modular de `antos-protocolo` en Submódulos Temáticos:** Divide el `lib.rs` de más de 3.500 líneas del protocolo IPC en submódulos temáticos.
- **[T22.3] Estandarización de Nomenclatura en Inglés y Limpieza de Deuda Técnica:** Alinea el código fuente con la regla de nomenclatura 100% en inglés de `.agents/rules/antos-development.md`.
- **[T22.4] Capa de Abstracción de Runtime de Plataforma (`PlatformRuntime`, `antos runtime`):** Unifica sandboxing, cuotas y telemetría entre Linux, macOS y el kernel bare-metal, reportando su fidelidad real por subsistema.
- **[T22.5] Interfaz ABI Inicial Kernel-Userspace y Proceso Init Bare-Metal:** El kernel `no_std` arranca y ejecuta su primer proceso de userspace nativo (`antos-init`), sin sistema operativo debajo.
- **[T22.6] Matriz de CI/CD Automatizada y Verificación Multiplataforma en GitHub Actions:** Pipeline `.github/workflows/ci.yml` que compila y prueba tanto los crates de host como el kernel `no_std` (x86_64 y AArch64).

#### 🔹 Fase 23: Kernel x86_64 con Multitarea Preemptiva Real
- **[T23.1] Controlador de Interrupciones APIC (LAPIC/IOAPIC) y Reemplazo del PIC 8259:** Sustituye el PIC 8259 (de 1984) por el Local APIC/IOAPIC moderno.
- **[T23.2] Planificador Preemptivo, Bloques PCB/TCB y Conmutación de Contexto:** El kernel deja de ejecutar programas de usuario de forma síncrona y bloqueante; nace el planificador Round-Robin preemptivo.
- **[T23.3] Consola Gráfica Framebuffer en Pantalla con Fuente Bitmap y Secuencias ANSI:** Consola en modo texto renderizada directamente sobre el framebuffer gráfico de UEFI/BIOS.
- **[T23.4] Driver de Bloque VirtIO (`virtio-blk`) y Sistema de Ficheros Inicial Initrd/tarfs:** Almacenamiento secundario real y carga de ejecutables de usuario desacoplada del binario del kernel.
- **[T23.5] Ampliación de Llamadas al Sistema POSIX e IPC por Canales Microkernel:** Formaliza la ABI de syscalls más allá de `SYS_WRITE`/`SYS_EXIT`, con IPC por canales entre procesos.

#### 🔹 Fase 24: Live USB, Instalador Guiado y Arranque en Metal
- **[T24.1] Integración de Bootloader UEFI Limine en `builder` para Arranque Híbrido:**
  - Binarios oficiales PE32+ de Limine (`BOOTX64.EFI` y `BOOTAA64.EFI`) embebidos en el crate `builder`.
  - Generador declarativo de archivos `/limine.conf`, `/EFI/BOOT/limine.conf` y `/startup.nsh`.
  - Código de arranque MBR para compatibilidad BIOS legacy y partición ESP FAT32 para UEFI nativo.
  - Creación de imágenes híbridas `.img` y `.iso` arrancables directamente en hardware y máquinas virtuales.
- **[T24.2] Empaquetador de Ramdisk (Initramfs) Live con Sistema Base y Herramientas:**
  - Módulo `builder::ramdisk` para empaquetado autónomo en formato micro-tar (`ustar`) de 512 bytes.
  - Estructura jerárquica de directorios del sistema de ficheros raíz (`/bin`, `/sbin`, `/etc`, `/dev`, `/proc`, `/sys`, `/mnt`, `/tmp`).
  - Inclusión del script maestro `/init` para montaje de sistemas virtuales (`devtmpfs`, `proc`, `sysfs`, `tmpfs`) y lanzamiento de `antosd` en modo Live.
  - Despliegue automático de utilidades del sistema (`antos`, `antosd`, `sh`, `parted`, `mkfs.ext4`) y manifiestos de configuración (`/etc/os-release`, `/etc/fstab`, `/etc/hostname`).
- **[T24.3] Drivers de Almacenamiento Físico (AHCI/SATA y NVMe) para Detección de Discos:**
  - Enumeración de controladores de almacenamiento masivo en bus PCI (`Class 0x01`: AHCI `0x06`, NVMe `0x08`, IDE `0x01`).
  - Driver nativo SATA / AHCI 1.0+ en `kernel/src/drivers/storage/ahci.rs` con soporte de comandos ATA `IDENTIFY DEVICE` (0xEC), `READ DMA EXT` (0x25) y `WRITE DMA EXT` (0x35) mediante listas de comando DMA y tablas PRDT.
  - Driver nativo PCIe NVMe 1.0+ en `kernel/src/drivers/storage/nvme.rs` con colas Admin (ASQ/ACQ) e I/O (IOSQ/IOCQ), doorbells MMIO y comandos NVM Read/Write con soporte de sectores de 512 y 4096 bytes.
  - Capa unificada de dispositivos de bloque `/dev/sda` y `/dev/nvme0n1` y contratos de datos IPC en `system/protocolo`.
- **[T24.4] Asistente de Instalación Guiado CLI y Particionamiento en Vivo (`antos install`):**
  - Módulo `system/antosd/src/installer/cli.rs` con asistente paso a paso para la sesión Live:
    * Paso 1: Detección y selección de discos de almacenamiento físicos con modelo, capacidad y SO existentes.
    * Paso 2: Modos de instalación: Disco Completo (con advertencia en rojo y confirmación textual estricta `SI`) o Dual-Boot seguro preservando la partición ESP y SO vecino.
    * Paso 3: Configuración de parámetros de sistema (hostname, usuario, timezone, keymap).
    * Paso 4: Despliegue guiado con barras de progreso animadas en terminal (particionamiento GPT, formateo, montaje en `/mnt/target`, copia de sistema base y capacidades antOS).
    * Paso 5: Generación persistente de `/etc/fstab`, registro UEFI NVRAM con `efibootmgr` (etiqueta `"antOS Linux"`) y desmontaje seguro.
  - Soporte de instalación desatendida no interactiva mediante `--config <archivo.toml>` y despliegue directo por flags.
- **[T24.5] Generador Automatizado de Live USB y Script de Grabación (`antos usb flash`):**
  - Módulo `system/antosd/src/installer/usb.rs` (`UsbManager`) con subcomandos `list`, `build`, `flash`, `verify`.
  - Detección segura de medios extraíbles USB en Linux (`lsblk`) y macOS (`diskutil`).
  - Salvaguarda crítica `is_safe_target` con bloqueo estricto de discos internos (NVMe, SATA del host, particiones `/` o `/boot` y discos superiores a 512 GB).
  - Grabación bit a bit en bloques de 4 MiB con barra de progreso dinámica en tiempo real (porcentaje, MB/s y tiempo transcurrido).
  - Verificación criptográfica obligatoria SHA-256 de los sectores grabados para certificar cero corrupción.
  - Compatibilidad directa con Ventoy (arranque directo de la ISO), Rufus (modo ISO y DD) y BalenaEtcher.
  - Guía oficial de instalación paso a paso en `docs/guia-live-usb-e-instalacion-fisica.md`.

#### 🔹 Fase 25: Aplicaciones Gráficas — `antpkg` GUI, Flatpak y Lanzador en la Barra
- **[T25.1] Extensión de `antpkg` para Aplicaciones Gráficas XDG y Desktop Entries:** `antpkg` genera y registra entradas `.desktop` conformes a la especificación Freedesktop, más allá de binarios CLI (`antos pkg apps`, `antos pkg validate`).
- **[T25.2] Gestor y Puente de Aplicaciones Flatpak y Contenedores Gráficos (`antos app`):** Gestión unificada de aplicaciones Flatpak junto a las nativas de `antpkg`, con lanzamiento inyectando el workspace activo.
- **[T25.3] Catálogo Oficial de Recetas antpkg para Navegadores e IDEs:** Recetas curadas para las principales herramientas de desarrollo: navegadores web (Firefox, Chrome) e IDEs (VS Code, Zed).
- **[T25.4] Lanzador de Aplicaciones Gráficas y Contexto de Workspace en Barra Wayland:** `Super + Space` funciona como lanzador estilo Spotlight/Raycast, inyectando `$ANTOS_WORKSPACE` al abrir IDEs.

#### 🔹 Fase 26: Escritorio Nativo Bare-Metal y Runtime/Shell de Espacio de Usuario Soberanos
- **[T26.1] Controlador de Framebuffer Gráfico AArch64 y VirtIO-GPU en Kernel Bare-Metal:** Detección de `simple-framebuffer` (DTB) o `virtio-gpu` MMIO con doble buffer, sin depender de un SO debajo.
- **[T26.2] Desktop Shell Nativo en Rust y Compositor 2D sobre Framebuffer:** Compositor 2D (cursor, HUD de intenciones, barra de estado, ventana de terminal) renderizado directamente sobre el framebuffer, sin GTK/Wayland.
- **[T26.3] Controlador de Entrada Nativo: VirtIO-Input, Teclado y Ratón:** Cola de eventos de entrada tipados compartida entre el driver PS/2 (x86_64) y VirtIO-Input (AArch64).
- **[T26.4] Cargador de Ejecutables ELF64 y Sistema de Ficheros Initramfs Tarfs:** El kernel carga ejecutables ELF64 de usuario desde un `tarfs` montado sobre el `initrd.tar` embebido.
- **[T26.5] Runtime Soberano `libantos` y Shell Interactivo en Espacio de Usuario:** Biblioteca de runtime sin `glibc`/`musl` (asignador sobre `SYS_MMAP`, IPC por canales, `print!`/`println!`/`read_line`) y el shell interactivo `antos-init` (PID 1, comandos `help`/`info`/`ls`/`cat`/`desktop`/`agent`).

#### 🔹 Fase 27: Arranque Limine Real, PCIe/USB Bare-Metal y Terminal Gráfico Activo
- **[T27.1] Soporte Real del Protocolo de Arranque Limine (Mitad Alta y Boot Requests) en el Kernel:** El kernel habla el protocolo Limine (enlace en mitad alta, `HHDM`, `memmap`, `framebuffer`, `module`), de modo que `antos-uefi-*.img` arranca por UEFI real (VirtualBox, EDK2, hardware) sin `PANIC: elf: Lower half PHDRs are not allowed`.
- **[T27.2] Controlador PCIe ECAM y Host Controller USB 3.0 xHCI:** Descubrimiento de la ventana ECAM (DTB/ACPI MCFG), escaneo de bus con puentes y bring-up de un controlador xHCI (anillos de comandos/eventos, DCBAA, *slots*).
- **[T27.3] Pila USB y Subclase HID para Teclado y Ratón en Bare-Metal:** Enumeración de dispositivos USB, `GET_DESCRIPTOR`/`SET_CONFIGURATION`, *Address Device*/*Configure Endpoint* y decodificadores HID Boot Protocol para teclado y ratón.
- **[T27.4] Terminal Gráfico Activo en Compositor 2D y Control Soberano de Escritorio:** La ventana de terminal del compositor pasa a ser interactiva y multiplexa el shell soberano con el escritorio nativo.

#### 🔹 Fase 28: Periféricos Nativos a Paridad entre Arquitecturas y Descubrimiento por Firmware
- **[T28.1] Verificación y Robustez del Driver VirtIO-Input MMIO:** Teclado, ratón y tablet nativos sobre el bus `virtio-mmio` de `-M virt`, con reintentos y validación de colas.
- **[T28.2] Mapeo PCIe ECAM/MMIO Multi-Plataforma y Escaneo de Bus con Puentes:** ECAM configurable por firmware (QEMU `virt`, VirtualBox) y recorrido de puentes PCI-a-PCI con protección contra bucles.
- **[T28.3] xHCI Robusto: Rings/Buffers por Endpoint, Control Transfers Extendidas, Hotplug y Hubs:** Un anillo y buffer DMA por endpoint, transferencias de control largas, *hotplug* por eventos + barrido PORTSC de reserva y soporte de hubs.
- **[T28.4] Parser de HID Report Descriptor y Decodificador Genérico Dirigido por Usages:** Análisis del Report Descriptor y decodificación por *usages*; el rol (teclado/ratón) se deriva del *usage* de la colección `Application`.
- **[T28.5] Ergonomía de Entrada: LEDs de Teclado, Auto-Repeat, Layouts y Aceleración de Puntero:** `SET_REPORT` para LEDs, auto-repetición, layouts US/ES y curva de aceleración del puntero (`INPUT_SETTINGS`).
- **[T28.6] Timer AArch64 Resiliente (Fallback Físico EL1) y GIC v2/v3 con Enrutado de IRQ de Periféricos:** Verificación del *timer* virtual y caída automática al físico EL1; driver GIC unificado v2 (GICC MMIO) y v3 (redistribuidor + `ICC_*_EL1`).
- **[T28.7] Paridad de Display: virtio-gpu-pci, ramfb y Cadena de Fallback de Framebuffer:** Cadena GOP → DTB → virtio-gpu MMIO → virtio-gpu-pci → ramfb, con *flush* explícito por rectángulo donde el transporte lo permite.
- **[T28.8] Descubrimiento por Firmware (DTB/ACPI) y Bring-Up sin Direcciones Hardcodeadas:** Parser FDT/DTB genérico y ACPI (`RSDP → XSDT → MCFG/MADT`); las bases de GIC, ECAM y framebuffer se leen del firmware.
- **[T28.9] Periféricos x86_64: Ratón PS/2 y Pila USB xHCI en x86_64:** Máquina de estados del ratón PS/2 (i8042) y la misma pila xHCI/HID compartida con AArch64.
- **[T28.10] Banco de Pruebas de Periféricos: Matriz de Emulación y Tests de Integración de Entrada:** `run.sh` / `system/run-arm.sh` parametrizados (`--kbd`/`--gpu`/`--gic`), humo de inyección de entrada (`--test-input`, `system/qemu-smoke.py`) y *job* `peripheral-smoke` en CI.

#### 🔹 Endurecimiento Runtime: antOS sobre VirtualBox ARM64 y UTM (posterior a Fase 28)
Iteración de depuración ejecutando antOS en hipervisores reales de macOS. Todas las correcciones verificadas en la VM.
- **GICv3 por ACPI en VirtualBox ARM64:** VirtualBox no expone un DTB alcanzable; el kernel parsea MADT y configura GICv3 con las bases reales (`d=0xfcd3_0000`, `r=0xfcd4_0000`) antes de `gic::init()`. Antes se quedaba en GICv2 y colgaba en `verificando fuente de temporizador…`.
- **GICv3 sin firmware (`qemu -M virt,gic-version=3 -kernel`):** al no haber DTB/ACPI, el kernel sondea `GICC_IIDR` con recuperación de fallos y conmuta a `init_v3()` en lugar de hacer *panic* con un Data Abort sobre el bloque GICC inexistente.
- **BAR PCIe sin asignar en arranque directo:** `-kernel` sin firmware no asigna ventanas a los BAR; `decode_bar()` trata un BAR de memoria con dirección cero como `PciBar::None` y el kernel omite el controlador con un aviso (`pcie-xhci … BAR0 sin asignar · omitido`) en vez de desreferenciar un puntero nulo.
- **Compositor sobre framebuffer GOP crudo:** el cursor ya no deja «descuadre» ni va lento en VirtualBox — `present_best` recompone por bandas de daño y `present_rows` vuelca líneas de barrido completas (el *scanout* GOP Non-Cacheable no reflejaba escrituras parciales estrechas).
- **Teclado USB en VirtualBox:** *Configure Endpoint* deja de poner a cero el *Root Hub Port Number* del Slot Context; se alimentan (`PP`) todos los puertos raíz antes de enumerar; el rol HID se clasifica por la colección `Application` (un teclado se detectaba como ratón).
- **Diagnósticos y estabilidad:** el *spam* de `input-rx:` se reduce a primer evento + una línea cada 100; corregido un *deadlock* al re-tomar el lock de `CONSOLE` dentro del log `fb-geom`.
- **Tooling:** `system/run-arm.sh` abre ventana gráfica con `--gpu <≠none>` y respeta `--release` (obligatorio en Apple Silicon: UTM/VirtualBox emulan el kernel con TCG). Guía de emulación (UTM/VirtualBox/QEMU) revisada de arriba abajo.

---

## [0.1.0] - 2026-09-02

¡Primer lanzamiento oficial de **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa!

Esta versión culmina las **14 Fases** de desarrollo del plan maestro con **35 tickets técnicos completados al 100%**, más de 129 pruebas automatizadas pasando en verde y soporte multi-plataforma tanto en Linux (con confinamiento estricto por Landlock / eBPF) como en macOS (con Seatbelt).

> Antes de la Fase 1, **[T0.1]** renombró íntegramente el proyecto de `syso` a **antOS**.

---

### 🌟 Resumen de las 14 Fases Implementadas

#### 🔹 Fase 1: Fundaciones y Demonio del SO (`antosd`)
- **[T1.1] Protocolo IPC Tipado:** Protocolo de mensajes bidireccional serializable con `serde_json` a través de sockets de dominio Unix (`antos-protocolo`), libre de `unwrap()` en rutas críticas.
- **[T1.2] Catálogo de Capacidades Declarativas:** Especificaciones en TOML de capacidades del sistema con definición de parámetros, efectos secundarios (lectura/escritura) y niveles de confirmación.
- **[T1.3] Planificador de Intenciones:** Motor local de PLN capaz de traducir intenciones de desarrolladores en planes atómicos ejecutables sin tocar el sistema de archivos sin permiso.

#### 🔹 Fase 2: Git Semántico, Worktrees Efímeros y Aislamiento de Ejecución
- **[T2.1] Analizador Semántico de Git:** Detección de estado del árbol de trabajo con optimización de caché sub-30ms para consultas instantáneas.
- **[T2.2] Aislamiento en Worktrees:** Flujo de desarrollo seguro en ramas aisladas (`git worktree`) gestionadas automáticamente por el demonio.
- **[T2.3] Sandbox de Doble Motor (Landlock / Seatbelt):** Confinamiento a nivel de kernel de escrituras fuera del workspace y protección estricta contra lectura no autorizada de secretos (.env, llaves SSH).

#### 🔹 Fase 3: Orquestador Multi-Agente antFlow Core
- **[T3.1] Máquina de Estados Multi-Agente:** Roles especializados para el ciclo de vida del software:
  - *Arquitecto:* Descomposición de tickets y diseño de dependencias.
  - *Coder:* Implementación modular en el worktree efímero.
  - *QA / Tester:* Generación y ejecución de pruebas en sandbox.
  - *Auditor de Seguridad:* Revisión de diffs, análisis de radio de impacto y verificación de criterios de aceptación.
- **[T3.2] Pipeline de Auto-Corrección:** Ciclo automático de reintento ante fallos en pruebas de integración antes de presentar el diff final al humano.
- **[T3.3] Consolidador de Diffs y Rollback por Ticket (`antos undo --ticket`):** Reversión granular de todos los commits y cambios asociados a un ticket técnico específico.

#### 🔹 Fase 4: Puertos, Servicios NixOS y Diagnóstico de Red
- **[T4.1] Diagnóstico y Liberación de Puertos:** Detección inteligente de conflictos TCP/UDP con identificación de procesos bloqueantes (PID, comando) y sugerencias de liberación seguras.
- **[T4.2] Gestión Declarativa de Servicios:** Integración con servicios locales NixOS y monitorización de demonios en segundo plano.

#### 🔹 Fase 5: Vault de Secretos y Concesiones Dinámicas
- **[T5.1] Detección de Rutas Sensibles:** Identificación proactiva de credenciales (.env, .pem, claves privadas SSH) para blindar el entorno de ejecución.
- **[T5.2] Motor de Concesiones Temporales:** Comandos `antos grant` y `antos revoke` para permitir accesos temporales explícitos a variables o rutas restringidas mediante reapertura segura en el sandbox.

#### 🔹 Fase 6: Inferencia LLM Local con Ollama y Fallback Offline
- **[T6.1] Motor de Inferencia Ollama:** Integración HTTP local con soporte de streaming, tool-calling y selección de modelos de razonamiento (Llama 3, DeepSeek, Qwen).
- **[T6.2] Fallback Determinista Offline:** Operación continua e ininterrumpida de antOS mediante heurísticas locales ante indisponibilidad de conexión o servidor LLM.

#### 🔹 Fase 7: Visualizador Interactivo de Diffs en Terminal (TUI) y Emulador VTE
- **[T7.1] Diff Viewer Estructurado:** Comparador visual en terminal con coloreado de sintaxis ANSI, métricas de inserción/borrado y navegación intuitiva.
- **[T7.2] Emulador de Terminal Virtual (VTE):** Captura y emulación de terminal para renderizado fiel de salidas de compilación y comandos interactivos.

#### 🔹 Fase 8: Bandeja de Notificaciones y Aprobaciones Asíncronas
- **[T8.1] Centro de Notificaciones Persistente:** Bandeja de eventos y alertas clasificada por urgencia (baja, media, alta, crítica).
- **[T8.2] Aprobaciones Asíncronas:** Sistema de solicitudes diferidas que permite a los agentes continuar trabajando en tareas no bloqueantes mientras esperan confirmación del usuario para cambios de alto riesgo.

#### 🔹 Fase 9: Malla P2P Distribuida (antMesh) y Despacho a Nodos GPU
- **[T9.1] Protocolo P2P antMesh:** Identidad criptográfica ED25519 para nodos y emparejamiento seguro mediante tokens efímeros.
- **[T9.2] Despacho Remoto de Roles:** Capacidad de transferir el rol Coder o QA a máquinas de alta potencia (workstations con GPU o clusters locales) sincronizando automáticamente los worktrees de Git.

#### 🔹 Fase 10: VFS Semántico (`antfs`) y Guardián de Código
- **[T10.1] Sistema de Archivos Virtual por Símbolos (`antfs`):** Proyección semántica de ficheros basada en funciones, structs, traits y tickets técnicos.
- **[T10.2] Guardián de Código (VFS Guard):** Intercepción en tiempo real de escrituras con errores de sintaxis (llaves huérfanas, JSON inválido) previniendo corrupción del código fuente en disco.

#### 🔹 Fase 11: Monitorización en Tiempo Real con eBPF y Profiler Continuo
- **[T11.1] Monitorización eBPF de Syscalls:** Registro y auditoría de violaciones de acceso y eventos bloqueados a nivel de kernel Linux.
- **[T11.2] Profiler Continuo de CPU y Memoria:** Muestreo de consumo de recursos con detección de picos y sugerencias de optimización generadas automáticamente para los agentes.

#### 🔹 Fase 12: Servidor LSP Unificado y Pair Programming Colaborativo
- **[T12.1] Servidor LSP Embebido:** Soporte nativo para Language Server Protocol con autocompletado de tickets y capacidades, diagnóstico de sintaxis y hover informativo.
- **[T12.2] Edición Colaborativa en Vivo (CRDT) y DAP:** Pair programming humano-agente con resolución determinista de conflictos y depurador paso a paso mediante Debug Adapter Protocol (DAP).

#### 🔹 Fase 13: Entorno Gráfico Wayland, Barra GTK4 y Pipeline Bare Metal
- **[T13.0] Compositor Wayland y Sesión de Escritorio:** Configuración declarativa de atajos de teclado, variables de entorno y soporte para compositores ligeros (Labwc / Sway).
- **[T13.1] Shell Gráfico (`antos-barra`):** Barra de estado en GTK4 integrada con telemetría eBPF en tiempo real, métricas del profiler y estado de agentes antFlow.
- **[T13.2] Pipeline de Arranque Bare Metal:** Kernel `no_std` compilado para `x86_64-unknown-none`, generador de disco BIOS con `builder` y emulación automatizada en QEMU.

#### 🔹 Fase 14: Ecosistema WebAssembly (WASI), QA Multimodal y Release v0.1.0
- **[T14.1] Motor de Capacidades y Plugins WebAssembly (WASI):** Parser y máquina virtual WASM con aislamiento de memoria lineal (cuota configurable de hasta 64 MB), medición de combustible (*fuel metering*) e imports de host antOS seguros.
- **[T14.2] Agente Multimodal con Captura Wayland (VisualQA):** Rol de inspección visual de interfaces gráficas mediante screencopy (`grim`/XDG portal), evaluando fidelidad de diseño, contraste WCAG 2.1 AAA y desalineaciones CSS con modelos de visión (Llava vía Ollama) o análisis heurístico.
- **[T14.3] Generador de Live ISO y Distribución Oficial:** Receta declarativa NixOS (`live-image.nix`), script de empaquetado release (`system/build-release.sh`), imagen booteable híbrida y sumas criptográficas SHA256.

---

### 📦 Artefactos de Distribución
- `antos-v0.1.0-<os>-<arch>.tar.gz`: Binarios de antOS (`antos`, `antosd`, `builder`), capacidades declarativas y documentación.
- `antos-live-x86_64.iso`: Imagen de disco arrancable híbrida para pruebas en bare metal, pendrives Live USB o hipervisores (QEMU, UTM, VirtualBox).
- `antos-live-x86_64.iso.sha256`: Suma de comprobación SHA-256 generada automáticamente para verificación de integridad.
- `SHA256SUMS`: Sumas de comprobación criptográficas de todos los artefactos de la distribución.

