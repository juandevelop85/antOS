# antOS · Manual Completo de Comandos y Métodos de Arranque 🐜⚡

Este manual detalla **todos los métodos para arrancar y ejecutar antOS** (CLI, demonio IPC, entorno Wayland GTK4, contenedor aislado, máquina virtual QEMU y núcleo bare-metal), así como la **referencia exhaustiva de comandos, banderas y combinaciones de opciones** para cada uno de los subsistemas del sistema operativo.

---

## 📑 Tabla de Contenidos
1. [Métodos de Arranque del Sistema Operativo](#1-métodos-de-arranque-del-sistema-operativo)
   - [Método 1: CLI Directo y Centro de Control en Host (macOS y Linux)](#método-1-cli-directo-y-centro-de-control-en-host-macos-y-linux)
   - [Método 2: Demonio en Segundo Plano y Socket IPC (`antos escucha`)](#método-2-demonio-en-segundo-plano-y-socket-ipc-antos-escucha)
   - [Método 3: Shell Gráfico Wayland GTK4 (`antos-barra`)](#método-3-shell-gráfico-wayland-gtk4-antos-barra)
   - [Método 4: Confinamiento Kernel en Linux con Landlock y Cgroups v2](#método-4-confinamiento-kernel-en-linux-con-landlock-y-cgroups-v2)
   - [Método 5: Máquina Virtual antOS NixOS Completa en QEMU](#método-5-máquina-virtual-antos-nixos-completa-en-qemu)
   - [Método 6: Núcleo Bare-Metal `no_std` Multi-Arquitectura (x86_64 y AArch64) en QEMU y UEFI](#método-6-núcleo-bare-metal-no_std-multi-arquitectura-x86_64-y-aarch64-en-qemu-y-uefi)
     - [A. Arquitectura x86_64 (BIOS Legacy y UEFI GPT)](#a-arquitectura-x86_64-bios-legacy-y-uefi-gpt)
     - [B. Arquitectura AArch64 / ARM 64-bit (Bare Metal y UEFI)](#b-arquitectura-aarch64--arm-64-bit-bare-metal-y-uefi)
     - [C. Opciones del CLI `builder`](#c-opciones-del-cli-builder)
     - [D. Guía Paso a Paso para Hipervisores (UTM y VirtualBox)](#d-guía-paso-a-paso-para-hipervisores-utm-y-virtualbox)
2. [Variables de Entorno Globales](#2-variables-de-entorno-globales)
3. [Banderas Globales del Comando `antos`](#3-banderas-globales-del-comando-antos)
4. [Catálogo Exhaustivo de Comandos y Subcomandos](#4-catálogo-exhaustivo-de-comandos-y-subcomandos)
   - [4.1 Intenciones y Lenguaje Natural](#41-intenciones-y-lenguaje-natural)
   - [4.2 Orquestación Multi-Agente (`antos agent`)](#42-orquestación-multi-agente-antos-agent)
   - [4.3 Tablero Kanban y Centro de Control (`antos panel`)](#43-tablero-kanban-y-centro-de-control-antos-panel)
   - [4.4 Especificaciones y Tickets (`antos tickets` / `antos ticket`)](#44-especificaciones-y-tickets-antos-tickets--antos-ticket)
   - [4.5 Visor de Diffs Interactivo y Consola VTE (`antos diff` / `antos terminal`)](#45-visor-de-diffs-interactivo-y-consola-vte-antos-diff--antos-terminal)
   - [4.6 Bandeja de Notificaciones y Aprobaciones (`antos notify`)](#46-bandeja-de-notificaciones-y-aprobaciones-antos-notify)
   - [4.7 Memoria Semántica y Grafo de Contexto (`antos memory`)](#47-memoria-semántica-y-grafo-de-contexto-antos-memory)
   - [4.8 Perfiles de Entorno Declarativo Nix y Devbox (`antos env`)](#48-perfiles-de-entorno-declarativo-nix-y-devbox-antos-env)
   - [4.9 Cuotas y Límites de Recursos para Sandboxes (`antos quota`)](#49-cuotas-y-límites-de-recursos-para-sandboxes-antos-quota)
   - [4.10 Bóveda de Secretos y Concesiones (`antos secret`, `grant`, `revoke`)](#410-bóveda-de-secretos-y-concesiones-antos-secret-grant-revoke)
   - [4.11 Servicios Locales Efímeros (`antos service` / `services`)](#411-servicios-locales-efímeros-antos-service--services)
   - [4.12 Diagnóstico del Sistema y Red (`antos doctor`, `antos ports`)](#412-diagnóstico-del-sistema-y-red-antos-doctor-antos-ports)
   - [4.13 Gestión Multi-LLM y Catálogo de Proveedores Gratuitos (`antos llm`)](#413-gestión-multi-llm-y-catálogo-de-proveedores-gratuitos-antos-llm)
   - [4.14 Bitácora Inmutable y Reversión Atómica (`antos log`, `antos undo`)](#414-bitácora-inmutable-y-reversión-atómica-antos-log-antos-undo)
   - [4.15 Red P2P Cifrada antMesh (`antos mesh`)](#415-red-p2p-cifrada-antmesh-antos-mesh)
   - [4.16 Swarm Multi-Nodo y Despacho Distribuido (`antos swarm`)](#416-swarm-multi-nodo-y-despacho-distribuido-antos-swarm)
   - [4.17 Sistema de Ficheros Virtual Semántico (`antos vfs`)](#417-sistema-de-ficheros-virtual-semántico-antos-vfs)
   - [4.18 Supervisor Kernel eBPF LSM (`antos ebpf`)](#418-supervisor-kernel-ebpf-lsm-antos-ebpf)
   - [4.19 Profiler Continuo de CPU y Memoria (`antos profile`)](#419-profiler-continuo-de-cpu-y-memoria-antos-profile)
   - [4.20 Servidor Language Server Protocol (LSP) Unificado (`antos lsp`)](#420-servidor-language-server-protocol-lsp-unificado-antos-lsp)
   - [4.21 Edición Colaborativa Humano-Agente y Depuración DAP (`antos pair` / `antos debug`)](#421-edición-colaborativa-humano-agente-crdt-y-depuración-aislada-dap-antos-pair--antos-debug)
   - [4.22 Entorno de Escritorio Wayland y Atajos Globales (`antos desktop`)](#422-entorno-de-escritorio-wayland-y-atajos-globales-antos-desktop)
   - [4.23 Telemetría en Tiempo Real y Alertas en la Barra (`antos barra`)](#423-telemetría-en-tiempo-real-y-alertas-visuales-en-la-barra-antos-barra)
   - [4.24 Pipeline de Arranque Bare Metal y Emulación QEMU (`antos boot`)](#424-pipeline-de-arranque-bare-metal-y-emulación-qemu-antos-boot)
   - [4.25 Motor de Capacidades y Plugins WebAssembly (`antos plugin`)](#425-motor-de-capacidades-y-plugins-webassembly-antos-plugin)
   - [4.26 Captura Wayland e Inspección Visual QA (`antos screenshot` / `antos qa visual`)](#426-captura-de-pantalla-wayland-e-inspección-visual-multimodal-antos-screenshot--antos-qa-visual)
   - [4.27 Live ISO y Empaquetado Release (`antos boot iso` / `antos release`)](#427-live-iso-y-empaquetado-release-antos-boot-iso--antos-release)
   - [4.28 Inspección de Almacenamiento y Particionamiento GPT (`antos disk`)](#428-inspección-de-almacenamiento-y-particionamiento-gpt-antos-disk)
   - [4.29 Asistente e Instalador de Sistema Base a Disco Duro (`antos install`)](#429-asistente-e-instalador-de-sistema-base-a-disco-duro-antos-install)
   - [4.30 Gestor de Arranque UEFI y Dual Boot (`antos bootloader`)](#430-gestor-de-arranque-uefi-y-dual-boot-antos-bootloader)
   - [4.31 MicroVMs Efímeras y Aislamiento por Hipervisor (`antos vm`)](#431-microvms-efímeras-y-aislamiento-por-hipervisor-antos-vm)
   - [4.32 Gestor de Paquetes y Recetas Inmutables (`antos pkg`)](#432-gestor-de-paquetes-y-recetas-inmutables-antos-pkg)
   - [4.33 Modo Agente Autónomo Continuo (`antos autopilot`)](#433-modo-agente-autónomo-continuo-antos-autopilot)
   - [4.34 Consola Web Remota en Tiempo Real y Bridge WebSocket (`antos web`)](#434-consola-web-remota-en-tiempo-real-y-bridge-websocket-antos-web)
   - [4.35 Gestión de Proyectos, Selección Activa y Git en Workspace (`antos use` / `antos project` / `antos git`)](#435-gestión-de-proyectos-selección-activa-y-git-en-workspace-antos-use--antos-project--antos-git)
   - [4.36 Espacio de Trabajo Integrado Dev TUI (`antos dev`)](#436-espacio-de-trabajo-integrado-dev-tui-antos-dev)
   - [4.37 Reproducción Autónoma de Bugs TDD y Generación de Tests (`antos reproduce` / `antos testgen`)](#437-reproducción-autónoma-de-bugs-tdd-y-generación-de-tests-antos-reproduce--antos-testgen)
5. [Recetas y Combinaciones de Uso Avanzadas](#5-recetas-y-combinaciones-de-uso-avanzadas)

---

## 1. Métodos de Arranque del Sistema Operativo

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                      6 MODOS DE EJECUCIÓN DE antOS                          │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 1. CLI y Centro de Control (Host): Desarrollo diario en macOS y Linux       │
 │ 2. Demonio IPC en Segundo Plano: Escucha en socket UNIX y atiende clientes  │
 │ 3. Shell Gráfico Wayland (GTK4): HUD contextual flotante y Kanban (Super+A) │
 │ 4. Contenedor Linux (Landlock LSM): Verificación de aislamiento kernel      │
 │ 5. Máquina Virtual NixOS en QEMU: Sistema operativo completo y servicios    │
 │ 6. Kernel Bare-Metal no_std en QEMU: x86_64 (BIOS/UEFI) y AArch64 (ARM64)   │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### Método 1: CLI Directo y Centro de Control en Host (macOS y Linux)

El método estándar para trabajar en cualquier máquina de desarrollo:

```bash
# Compilar el binario del demonio y CLI
cargo build --bin antos

# Opción A: Ejecutar mediante cargo
cargo run --bin antos -- <comando o intención>

# Opción B: Instalar en PATH (~/.cargo/bin/antos)
cargo install --path system/antosd
antos <comando o intención>

# Opción C: Usar alias local
alias antos="$(pwd)/target/debug/antos"
antos <comando o intención>
```

---

### Método 2: Demonio en Segundo Plano y Socket IPC (`antos escucha`)

Permite ejecutar `antosd` como servicio persistente. Todos los clientes CLI y la barra Wayland se comunican con él a través del protocolo serializado `antos-protocolo` por socket UNIX:

```bash
# Iniciar el demonio en la terminal actual (socket por defecto: .antos/antos.sock)
antos escucha

# Iniciar indicando un socket y directorio de estado personalizado
ANTOS_STATE=/tmp/antos_state antos escucha

# Ejecutar en segundo plano con nohup o daemonizer
nohup antos escucha > /tmp/antosd.log 2>&1 &
```

Cualquier comando posterior (`antos status`, `antos agent run T1.1`) detectará el socket abierto y delegará la ejecución al demonio remoto de forma transparente.

---

### Método 3: Shell Gráfico Wayland GTK4 (`antos-barra`)

> **Nota:** La interfaz gráfica GTK4 Layer Shell requiere un compositor Wayland (Linux: Sway, Hyprland, GNOME Wayland) o la máquina virtual QEMU de antOS:

```bash
# Lanzar la barra de intenciones flotante
cd system/barra && cargo run

# Lanzar directamente con el Centro de Misión y Tablero Kanban desplegado
cd system/barra && cargo run -- --panel

# Lanzar pasando una intención preconfigurada
cd system/barra && cargo run -- --intent "muestra el estado del proyecto"
```

#### Atajos de Teclado Globales en el Shell Wayland:
* **`Super + Espacio`:** Abre o enfoca el HUD de intenciones con badges Git y diffs en vivo.
* **`Super + A`:** Despliega el Centro de Misión y el Tablero Kanban multi-agente.
* **`Super + T`:** Despliega la consola terminal interactiva VTE flotante.
* **`Escape`:** Cierra la ventana activa sin aplicar cambios pendientes.

---

### Método 4: Confinamiento Kernel en Linux con Landlock y Cgroups v2

Valida las políticas de seguridad del kernel Linux atacando el recinto sandbox desde un contenedor confinado:

```bash
# Requiere Podman o Docker en la máquina host
./system/verificar-linux.sh
```

El script ejecuta automáticamente:
1. Compilación de `antosd` en entorno puro de Linux.
2. Comprobación del diagnóstico `antos doctor`.
3. Intento de ataque a credenciales (`/root/.ssh/id_rsa`), comprobando que Landlock devuelva `EPERM`.
4. Prueba del watchdog y límites de cgroups v2 en cuotas de CPU y memoria.

---

### Método 5: Máquina Virtual antOS NixOS Completa en QEMU

Construye una imagen de máquina virtual con NixOS y antOS completamente integrado como demonio de sistema `systemd`:

```bash
# Construir la imagen y arrancar la VM en QEMU
./system/arrancar-vm.sh
```
* Para salir de la consola serial de QEMU presiona: `Ctrl-A` y luego `X`.

---

### Método 6: Núcleo Bare-Metal `no_std` Multi-Arquitectura (x86_64 y AArch64) en QEMU y UEFI

antOS cuenta con un kernel bare-metal `no_std` unificado bajo una Capa de Abstracción de Hardware (HAL) que soporta tanto **x86_64** (BIOS Legacy y UEFI GPT) como **AArch64 / ARM 64-bit** (QEMU `virt`, UEFI EDK2, Apple Silicon y Raspberry Pi).

#### A. Arquitectura x86_64 (BIOS Legacy y UEFI GPT)

```bash
# 1. Compilación y arranque rápido en QEMU (BIOS Legacy vía run.sh)
./run.sh

# 2. Generación manual de imágenes con el builder:
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format bios
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format uefi

# 3. Ejecución directa en QEMU x86_64:
qemu-system-x86_64 -drive format=raw,file=kernel/target/x86_64-unknown-none/debug/antos-bios.img -serial stdio
```

#### B. Arquitectura AArch64 / ARM 64-bit (Bare Metal y UEFI)

```bash
# 1. Instalar target de compilación si no está presente
rustup target add aarch64-unknown-none

# 2. Compilar el kernel para AArch64
cargo build --target aarch64-unknown-none --manifest-path kernel/Cargo.toml

# 3. Arrancar directamente el kernel AArch64 en QEMU virt (Direct Kernel Boot)
# Valida: consola serie PL011 MMIO, tabla VBAR_EL1 de 16 vectores, MMU (L0/L1/L2),
# heap dinámico, temporizador virtual ARM a 100 Hz, transición a EL0 y syscalls svc #0.
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/debug/kernel \
  -serial stdio -monitor none

# 4. Generar imágenes UEFI GPT (ESP FAT32 BOOTAA64.EFI) e ISO híbrida con builder:
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel

# 5. Arrancar con firmware UEFI EDK2 en QEMU AArch64:
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -bios QEMU_EFI.fd \
  -drive format=raw,file=kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img \
  -serial stdio
```

#### C. Opciones del CLI `builder`

El crate `builder/` permite empaquetar binarios ELF del kernel en imágenes de disco GPT con partición ESP (FAT32) e ISOs híbridas:

```bash
cargo run -p builder -- <ruta-al-kernel.elf> [--arch x86_64|aarch64] [--format all|uefi|bios|iso]
```

* `--arch <x86_64|aarch64>`: Sobrescribe la arquitectura del disco (autodetectada por defecto desde la cabecera ELF).
* `--format <all|uefi|bios|iso>`: Tipo de artefacto a emitir.

#### D. Guía Paso a Paso para Hipervisores (UTM y VirtualBox)

> 📖 **Para la guía completa ilustrada con resolución de problemas y capturas, consulta la [Guía de Emulación y Ejecución en UTM, VirtualBox y QEMU](guia-emulacion-utm-virtualbox.md).**

##### 1. Ejecución en UTM (macOS Apple Silicon e Intel)

UTM es el hipervisor recomendado en macOS para ejecutar antOS tanto en arquitectura ARM64 como x86_64.

> ⚠️ **Punto clave:** El kernel de antOS emite su telemetría y diagnósticos por el **puerto serie (UART PL011 en ARM64 / COM1 en x86_64)**. Para ver los mensajes del sistema en UTM es necesario tener habilitada la consola serie.

###### Modo Directo (Kernel Boot - Recomendado para desarrollo):
1. Abrir UTM y pulsar **Crear una nueva máquina virtual (+)**.
2. Seleccionar **Virtualizar** (o *Emular* si estás en Intel y deseas ARM64) -> **Otro (Other)**.
3. En la configuración de la máquina virtual (**Editar**):
   * **Sistema:**
     * **Arquitectura:** `ARM64 (aarch64)`.
     * **Sistema:** `QEMU 7.x / 8.x / 9.x ARM Virtual Machine (virt)`.
     * **Memoria RAM:** `512 MB` o `1024 MB`.
   * **QEMU:**
     * **Desmarcar** *"UEFI Boot"*.
   * **Dispositivos:**
     * Pulsar **Nuevo...** -> **Puerto serie** -> Modo: *Terminal / Emulado*.
   * **Arranque / Kernel:**
     * Seleccionar la ruta al binario compilado:
       ```text
       kernel/target/aarch64-unknown-none/debug/kernel
       ```
4. Iniciar la máquina virtual (Play). La pestaña de terminal mostrará el arranque de antOS en tiempo real.

###### Modo Imagen de Disco UEFI en UTM:
1. Generar la imagen UEFI:
   ```bash
   cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64
   ```
2. En UTM -> **Editar VM** -> **Unidades**:
   * Pulsar **Nuevo...** -> **Imagen de disco**.
   * **Interfaz:** `VirtIO` o `NVMe` (no CD/DVD).
   * Importar el archivo:
     ```text
     kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img
     ```
3. En **Dispositivos**, añadir un **Puerto serie**.
4. Iniciar la VM. Si ingresas a la UEFI Shell, escribe `map -r` y ejecuta `FS0:\EFI\BOOT\BOOTAA64.EFI`.

---

##### 2. Ejecución en VirtualBox

###### A. VirtualBox en macOS Apple Silicon (ARM64 / AArch64):
VirtualBox en Mac M1/M2/M3/M4 **solo permite crear VMs ARM64** y requiere firmware UEFI ARM64:
```bash
# 1. Compilar kernel y generar disco UEFI GPT para ARM64
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64

# 2. Convertir la imagen RAW generada a disco virtual VDI nativo
VBoxManage convertfromraw kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img antos-arm64.vdi --format VDI
```
* **Configuración en VirtualBox:**
  - Tipo: `Linux` / `Other (ARM 64-bit)` con 2 CPUs y 1024 MB RAM.
  - Almacenamiento: Añadir `antos-arm64.vdi` como **Disco Duro** SATA/SCSI (no unidad óptica CD/DVD).
  - Puertos Serie: Activar **Puerto 1** (COM1) en modo *"Archivo sin formato"* (ej. `/tmp/antos-serial.log`) para capturar la salida UART PL011.
* **Arranque:** En la UEFI Shell, escribe `map -r` y ejecuta `FS0:\EFI\BOOT\BOOTAA64.EFI` (o `startup.nsh`).

###### B. VirtualBox en PCs y Macs Intel (x86_64):
```bash
# 1. Generar imagen BIOS x86_64 y convertir a VDI
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format bios
VBoxManage convertfromraw kernel/target/x86_64-unknown-none/debug/antos-bios.img antos-x86.vdi --format VDI
```
* **Configuración en VirtualBox:**
  - Desmarcar *"Habilitar EFI"* en *Sistema -> Placa Base*.
  - Añadir `antos-x86.vdi` como disco duro SATA/IDE. Arrancará en modo BIOS MBR nativo.

---

## 2. Variables de Entorno Globales

| Variable | Descripción | Valor por Defecto |
| :--- | :--- | :--- |
| `ANTOS_WORKSPACE` | Directorio raíz del proyecto en el que opera antOS | Raíz del repositorio Git actual o `pwd` |
| `ANTOS_STATE` | Directorio de estado persistente (bitácora, servicios, secretos) | `$ANTOS_WORKSPACE/.antos` o `~/.local/state/antos` |
| `ANTOS_SOCKET` | Ruta del socket UNIX del demonio | `$ANTOS_STATE/antos.sock` |
| `ANTOS_CAPABILITIES` | Directorio con los manifiestos TOML de capacidades | `system/capabilities/` |
| `OLLAMA_HOST` | URL base del servidor Ollama para inferencia local | `http://localhost:11434` |
| `OLLAMA_MODEL` | Modelo por defecto de Ollama | `qwen2.5-coder:7b` |
| `ANTHROPIC_API_KEY` | Clave de API de Anthropic para el planificador Claude | `~/.config/antos/anthropic.key` |

---

## 3. Banderas Globales del Comando `antos`

```bash
antos [BANDERAS_GLOBALES] <SUBCOMANDO_O_INTENCIÓN>
```

| Bandera | Alias | Descripción |
| :--- | :--- | :--- |
| `--dry-run` | `-n` | Modo simulación: genera el plan, calcula el blast radius y el diff, pero **no modifica el disco**. |
| `--yes` | `-y` | Modo automático: asume `sí` en confirmaciones interactivas (ideal para scripts CI/CD). |
| `--planner <TIPO>` | `-p` | Selecciona el motor de inferencia: `local` (reglas rápidas offline), `ollama` (LLM local), `claude` (nube). |
| `--model <MODELO>` | `-m` | Sobrescribe el nombre del modelo de lenguaje (ej. `llama3:8b`, `codellama`, `deepseek-coder`). |
| `--workspace <DIR>` | `-w` | Define la ruta del espacio de trabajo objetivo. |
| `--help` | `-h` | Muestra la ayuda y el listado de subcomandos disponibles. |

---

## 4. Catálogo Exhaustivo de Comandos y Subcomandos

### 4.1 Intenciones y Lenguaje Natural

El planificador de antOS traduce peticiones en lenguaje natural a planes de ejecución con análisis de radio de impacto:

```bash
# Ejecutar una intención básica (solicitará confirmación interactiva s/N)
antos "crea un proyecto rust llamado cli-tool"

# Modo simulación (dry-run): inspecciona lo que el sistema haría sin tocar nada
antos -n "actualiza las dependencias de cargo y compila"

# Modo desatendido con confirmación automática
antos -y "limpia los archivos temporales y ramas huérfanas"

# Forzar el planificador con modelo LLM local Ollama
antos -p ollama -m qwen2.5-coder:7b "analiza la seguridad de los endpoints en api.rs"

# Forzar el planificador en la nube de Claude (requiere ANTHROPIC_API_KEY)
antos -p claude "refactoriza la gestión de errores usando thiserror"
```

---

### 4.2 Orquestación Multi-Agente (`antos agent`)

Gestiona el equipo autónomo de agentes especializados (`antFlow`) que ejecutan tareas en *Git Worktrees* efímeros:

```bash
# Ver los roles especializados y sus directivas de sistema (Arquitecto, Coder, QA, Auditor)
antos agents

# Ejecutar un ticket técnico con la cadena completa de agentes
antos agent run T1.1

# Ejecutar el ticket en modo completamente automatizado (auto-reintento en QA)
antos agent run T1.1 --auto

# Despachar el ticket delegando roles a un nodo remoto específico o clúster de cómputo
antos agent run T9.2 --node node-gpu-01
antos agent run T9.2 --remote

# Consultar el estado del Swarm distribuido desde el subcomando de agentes
antos agent swarm

# Consultar el estado actual, worktree asignado e historial de transiciones del ticket
antos agent status T1.1

# Consultar la matriz de modelos LLM asignados por rol de agente (T19.4)
antos agent config

# Asignar modelos específicos por rol (Ollama local, Groq gratuito, OpenRouter)
antos agent config --role coder --llm ollama:qwen2.5-coder:latest
antos agent config --role architect --llm openrouter:deepseek/deepseek-r1:free
antos agent config --role qa --llm groq:llama-3.3-70b-versatile
antos agent config --role auditor --llm groq:llama-3.3-70b-versatile

# Aprobar manualmente la revisión humana y fusionar los cambios del worktree a la rama base
antos agent approve T1.1

# Rechazar la revisión y ejecutar rollback del worktree
antos agent reject T1.1
```

---

### 4.3 Tablero Kanban y Centro de Control (`antos panel`)

Visualiza el tablero de control del proyecto sincronizado en tiempo real con `docs/tickets/`:

```bash
# Abrir el tablero Kanban interactivo en consola (4 columnas + monitor de agentes)
antos panel

# Despachar directamente un ticket desde la terminal al equipo multi-agente
antos panel --dispatch T1.3
antos panel -d T2.1
```

---

### 4.4 Especificaciones y Tickets (`antos tickets` / `antos ticket`)

Motor *SpecEngine* para indexación, creación y actualización de especificaciones de desarrollo con soporte multi-proyecto y aislamiento estricto de frontera (*ceiling*):

```bash
# Listar tickets en el contexto actual:
# - Si se invoca desde workspace/<proyecto>: lista exclusivamente los tickets de ese proyecto.
# - Si se invoca desde workspace/: muestra un resumen de tickets por proyecto.
# - Si se invoca desde la raíz de antOS: lista los tickets del sistema operativo.
antos tickets

# Listar tickets de un proyecto específico del workspace (desde cualquier directorio)
antos tickets api-service
antos tickets --project api-service

# Forzar la inspección de tickets del sistema operativo antOS
antos tickets --system

# Ver el detalle técnico estructurado de un ticket específico
antos ticket T1.1 --project api-service
antos ticket T1.3                               # En antOS o ámbito actual

# Crear un nuevo ticket técnico dinámico en el catálogo independiente de un proyecto
antos ticket new T1.1 "Autenticación JWT" --project api-service --fase "Fase 1" --desc "Endpoints de login y tokens"

# Actualizar el estado de un ticket en el catálogo del proyecto correspondiente
antos ticket status T1.1 en_progreso --project api-service
antos ticket status T1.1 completado --project api-service
```

---

### 4.5 Visor de Diffs Interactivo y Consola VTE (`antos diff` / `antos terminal`)

Inspección de cambios de código con resaltado sintáctico, aislamiento de frontera de repositorio (`GIT_CEILING_DIRECTORIES`) y soporte multi-proyecto:

```bash
# Ver el diff del proyecto activo (si cwd está dentro de workspace/<proyecto>) o escanear workspace/
antos diff

# Inspeccionar exclusivamente los cambios de un proyecto específico
antos diff api-service

# Comparar un proyecto contra una rama, commit o ticket específico
antos diff api-service main
antos diff api-service feature/auth
antos diff api-service HEAD~1

# Si un proyecto en workspace/ no tiene Git inicializado, antos diff muestra un resumen limpio de archivos detectados
antos diff demo

# Abrir el shell interactivo embebido VTE
antos terminal

# Ejecutar un comando puntual en la consola VTE con captura de salida y código de retorno
antos terminal "cargo check"
antos terminal "git status --short"
```

---

### 4.6 Bandeja de Notificaciones y Aprobaciones (`antos notify`)

Bandeja de alertas asíncronas para revisiones de código y aprobaciones multi-agente:

```bash
# Listar notificaciones y solicitudes de aprobación activas
antos notify
antos notify list

# Aprobar una notificación de revisión (fusiona el worktree del agente)
antos notify approve notif-t8-2

# Rechazar una notificación de revisión (ejecuta rollback y limpia el worktree)
antos notify reject notif-t8-2

# Descartar una notificación puntual
antos notify dismiss notif-t8-2

# Limpiar todas las notificaciones ya leídas
antos notify clear
```

---

### 4.7 Memoria Semántica y Grafo de Contexto (`antos memory`)

Motor de análisis estático, cálculo de vectores de términos L2 y grafo bidireccional de dependencias:

```bash
# Indexar el espacio de trabajo en la base SQLite vectorial (.antos/memory.db)
antos memory index

# Búsqueda semántica por similitud coseno
antos memory search "orquestador de agentes y roles"
antos memory search "bóveda de secretos y permisos"

# Visualizar el grafo de dependencias de un módulo o archivo específico
antos memory graph system/antosd/src/main.rs
antos memory graph system/antosd/src/flow.rs
```

---

### 4.8 Perfiles de Entorno Declarativo Nix y Devbox (`antos env`)

Gestor de toolchains y entornos de compilación reproducibles por proyecto:

```bash
# Consultar el estado del perfil activo y toolchains disponibles en PATH
antos env
antos env status

# Inicializar un perfil declarativo (.antos/env.toml, devbox.json, flake.nix)
antos env init rust
antos env init node
antos env init python
antos env init go
antos env init base

# Sincronizar e instalar las dependencias del perfil en el workspace
antos env sync
```

---

### 4.9 Cuotas y Límites de Recursos para Sandboxes (`antos quota`)

Control del confinamiento de CPU, memoria RAM y timeouts en la ejecución de sandboxes:

```bash
# Consultar las cuotas de recursos activas y el mecanismo del kernel en uso
antos quota status

# Modificar los límites de recursos del sandbox
antos quota set --timeout 60 --memory 1024 --cpu 75 --pids 128

# Restablecer las cuotas a los valores seguros por defecto (120s, 2048MB, 100%, 256 PIDs)
antos quota reset
```

---

### 4.10 Bóveda de Secretos y Concesiones (`antos secret`, `grant`, `revoke`)

Modelo de **Cero Autoridad Ambiental**: los secretos están cifrados con permisos `0600` y bloqueados por defecto a menos que exista una concesión explícita (`grant`):

```bash
# Guardar un secreto o clave de API en la bóveda
antos secret set STRIPE_API_KEY sk_live_9988776655
antos secret set DATABASE_PASSWORD mi_clave_segura

# Listar las credenciales almacenadas y el estado de sus concesiones
antos secrets

# Intentar leer un secreto (denegado si no hay concesión activa)
antos secret get STRIPE_API_KEY

# Otorgar una concesión temporal con justificación y auditoría
antos grant secret.STRIPE_API_KEY --minutos 15 --para "ejecutar pruebas de pago en sandbox"

# Leer el secreto con la concesión vigente
antos secret get STRIPE_API_KEY

# Revocar de inmediato una concesión activa
antos revoke secret.STRIPE_API_KEY
```

---

### 4.11 Servicios Locales Efímeros (`antos service` / `services`)

Aprovisionamiento bajo demanda de servicios de apoyo en `$STATE/services/` con inyección de variables de conexión:

```bash
# Levantar una base de datos PostgreSQL local
antos service up postgres
antos service up postgres 5432

# Levantar una instancia de Redis
antos service up redis
antos service up redis 6379

# Levantar MariaDB / MySQL
antos service up mariadb 3306

# Consultar la tabla de servicios activos y cadenas de conexión inyectadas
antos services

# Detener y liberar los recursos de un servicio
antos service down postgres
antos service down redis
```

---

### 4.12 Diagnóstico del Sistema y Red (`antos doctor`, `antos ports`)

Herramientas de autodiagnóstico del host, sandboxing y resolución de conflictos de puertos:

```bash
# Autodiagnóstico integral de antOS (Sandbox kernel, Git, sockets, herramientas)
antos doctor

# Diagnosticar puertos TCP en escucha en el sistema y sus PIDs asociados
antos ports

# Inspeccionar un puerto específico en conflicto
antos ports 8080

# Liberar un puerto conflictivo matando el proceso asociado de forma controlada
antos "libera el puerto 8080"
```

---

### 4.13 Gestión Multi-LLM y Catálogo de Proveedores Gratuitos (`antos llm`)

Gestión integral de motores de inferencia (locales offline y cloud tiers gratuitos sin coste) con conmutación dinámica:

```bash
# Diagnóstico completo de motores, estado de conexión, endpoints y latencia
antos llm status

# Explorar catálogo curado de modelos 100% gratuitos (Groq, OpenRouter, Gemini, OpenCode, Ollama)
antos llm free

# Fijar dinámicamente el motor de inferencia activo del sistema operativo
antos llm use groq --model llama-3.3-70b-versatile
antos llm use openrouter --model deepseek/deepseek-r1:free
antos llm use gemini --model gemini-2.0-flash
antos llm use ollama --model qwen2.5-coder:latest
antos llm use opencode --endpoint http://127.0.0.1:8080/v1
antos llm use local               # Planificador determinista local offline

# Restablecer selección automática inteligente
antos llm use --clear

# Prueba interactiva de inferencia y Tool Calling con el motor activo
antos llm test
antos llm test --prompt "crea un microservicio en rust"

# Asistente de configuración guiada y recomendaciones de modelos de desarrollo
antos llm setup
antos llm setup --model qwen2.5-coder:7b

# Instalar el motor Ollama mediante antpkg y receta declarativa
antos pkg install recipes/ollama.toml
antos pkg install ollama

# Listar modelos configurados y modelos locales descargados
antos llm list
```

---

### 4.14 Bitácora Inmutable y Reversión Atómica (`antos log`, `antos undo`)

Trazabilidad transaccional completa de todas las modificaciones y reversión instantánea:

```bash
# Consultar la bitácora histórica de intenciones, planes y hashes de instantánea
antos log

# Revertir la última transacción ejecutada restaurando el snapshot atómico
antos undo

# Revertir todos los commits y cambios generados a lo largo de un ticket técnico
antos undo --ticket T1.1
antos undo --ticket T3.2
```

---

### 4.15 Red P2P Cifrada antMesh (`antos mesh`)

Gestión de la malla peer-to-peer cifrada para clústeres de desarrollo y delegación remota:

```bash
# Consultar el estado del nodo local y los peers vecinos conectados
antos mesh
antos mesh status

# Generar un token seguro de emparejamiento con 15 minutos de expiración
antos mesh pair

# Conectar a un nodo peer remoto mediante dirección IP/puerto o multiaddr
antos mesh connect 192.168.1.50:9042
antos mesh connect /ip4/192.168.1.50/udp/9042/quic
```

---

### 4.16 Swarm Multi-Nodo y Despacho Distribuido (`antos swarm`)

Centro de control para orquestación de agentes distribuidos y sincronización transparente de worktrees:

```bash
# Visualizar la matriz de nodos del clúster Swarm, cores de CPU, VRAM y tareas activas
antos swarm

# Despachar una subtarea de desarrollo delegándola a un nodo remoto específico
antos swarm dispatch T9.2 --node node-3b95c1d25f

# Despachar delegando el rol de QA a un nodo con múltiples cores de CPU
antos swarm dispatch T9.2 --qa --node node-cpu-cluster

# Sincronización automática de worktrees efímeros mediante bundles Git sobre la malla
antos swarm dispatch T1.1
```

---

### 4.17 Sistema de Ficheros Virtual Semántico (`antos vfs`)

Proyección virtual del código como jerarquías navegables de símbolos AST, grafo de dependencias y diffs de Git:

```bash
# Consultar el estado del VFS, punto de montaje y total de símbolos indexados
antos vfs

# Listar todos los símbolos descubiertos agrupados por categoría (structs, functions, enums, traits)
antos vfs symbols

# Explorar la jerarquía virtual de directorios en /antfs
antos vfs ls /antfs
antos vfs ls /antfs/symbols/structs
antos vfs ls /antfs/git/uncommitted

# Leer el código o diff de un inodo virtual específico
antos vfs read /antfs/symbols/structs/MeshStatus
antos vfs read /antfs/git/status

# Montar la proyección de /antfs en el disco local para inspección con herramientas nativas (ls, cat, find)
antos vfs mount
antos vfs mount .antos/mnt/antfs

# Desmontar y limpiar el punto de montaje virtual
antos vfs unmount

# Validar la integridad sintáctica de un fichero antes de persistir (T10.2)
antos vfs validate src/main.rs
antos vfs validate frontend/index.ts

# Consultar el estado y estadísticas del interceptor de escrituras semánticas (T10.2)
antos vfs guard
```

---

### 4.18 Supervisor Kernel eBPF LSM (`antos ebpf`)

Monitorización y centinela de seguridad a nivel de kernel para detección inmediata de evasiones de sandbox y llamadas al sistema anómalas:

```bash
# Diagnóstico de compatibilidad de eBPF LSM en el kernel y sondas activas
antos ebpf status

# Visualizar la traza de llamadas al sistema en tiempo real
antos ebpf trace

# Filtrar eventos y traza de syscalls para un PID específico
antos ebpf trace 12345

# Inspeccionar el registro de auditoría de seguridad del ring buffer (por defecto 20 eventos)
antos ebpf audit
antos ebpf audit 50

# Simular un intento de evasión de sandbox para comprobar la intercepción y alertas
antos ebpf simulate file /etc/shadow
antos ebpf simulate socket 192.168.1.100:4444
antos ebpf simulate bprm /bin/nc
```

---

### 4.19 Profiler Continuo de CPU y Memoria (`antos profile`)

Telemetría de rendimiento y profiling en tiempo de ejecución para detección de cuellos de botella y sugerencias de optimización para agentes Coder y QA:

```bash
# Ejecutar y perfilar cualquier comando en tiempo real midiendo CPU, memoria RSS y page faults
antos profile run "cargo check"
antos profile run "cargo test"

# Ver los puntos calientes (hotspots) consolidados en la ejecución
antos profile top

# Analizar la telemetría agregada y obtener sugerencias técnicas de optimización
antos profile analyze

# Consultar el histórico de reportes guardados
antos profile list
```

---

### 4.20 Servidor Language Server Protocol (LSP) Unificado (`antos lsp`)

Servidor LSP embebido para dotar a editores externos (VS Code, Neovim, Helix, Emacs) de autocompletado semántico enriquecido con el contexto de tickets, símbolos del workspace y capacidades tipadas de antOS:

# Abrir un archivo o iniciar el editor de texto predeterminado de antOS (Neovim / Super + E)
antos edit src/main.rs
antos edit

# Iniciar el espacio de trabajo integrado Dev TUI (Neovim + antFlow + Visor de Diffs / Super + W)
antos dev
antos dev --project api-service
antos dev --preview
antos dev --status

# Iniciar el servidor LSP sobre stdio (utilizado directamente por editores externos)
antos lsp
antos lsp stdio

# Consultar el estado del servidor y cantidad de símbolos AST indexados
antos lsp status

# Generar configuraciones automáticas listas para usar (por defecto Neovim: init.lua)
antos lsp config
antos lsp config neovim
antos lsp config vscode
antos lsp config helix
antos lsp config emacs
```

---

### 4.21 Edición Colaborativa Humano-Agente (CRDT) y Depuración Aislada DAP (`antos pair` / `antos debug`)

Permite programar en pareja en tiempo real con el agente `Coder` mediante cursores virtuales sincronizados y deltas de texto sin conflictos (CRDT), además de inspeccionar procesos bajo el protocolo estándar DAP en sandbox:

```bash
# Iniciar sesión interactiva de pair programming con el Coder en un archivo o ticket
antos pair
antos pair src/main.rs
antos pair T12.2 Cargo.toml

# Lanzar y supervisar un comando dentro del sandbox con el adaptador DAP aislado
antos debug
antos debug cargo test
antos debug cargo run --bin antos
```

### 4.22 Entorno de Escritorio Wayland y Atajos Globales (`antos desktop`)

Orquestación del compositor gráfico Wayland ultraligero (~30-40 MB RAM) basado en Labwc, soporte nativo de layer-shell para la barra de antOS y atajos de teclado globales.

```bash
# Diagnosticar estado de la sesión gráfica, compositor activo y clientes
antos desktop status
antos desktop

# Arrancar la sesión gráfica de escritorio de antOS
antos desktop start
antos desktop start --nested      # Iniciar dentro de una ventana de desarrollo anidada

# Mostrar la guía de atajos de teclado globales registrados
antos desktop keys
```

**Atajos de Teclado Globales Registrados:**
* `Super + Space`: Abre o enfoca la barra de intenciones de antOS (`antos-barra`).
* `Super + A`: Despliega el Centro de Agentes y Tablero de Tickets.
* `Super + Return`: Lanza la terminal virtual interactiva integrada (`vte`).
* `Super + D`: Abre el visor interactivo de diffs y reversión (`diff_view`).
* `Super + E`: Abre el editor de texto predeterminado de antOS (`Neovim` / `antos edit`).
* `Super + W`: Abre el espacio de trabajo integrado Dev TUI (`Neovim` + `antFlow` + `diff_view` / `antos dev`).
* `Super + Q`: Cierra la ventana enfocada actualmente.
* `Alt + Tab`: Conmuta a la siguiente ventana.
* `Super + F`: Alterna modo pantalla completa.
* `Super + Shift + E`: Finaliza la sesión gráfica de escritorio.

---

### 4.23 Telemetría en Tiempo Real y Alertas Visuales en la Barra (`antos barra`)

Consolidación en vivo del estado del supervisor eBPF LSM, telemetría de memoria RSS y CPU del Profiler, sesiones activas de Pair Programming con el Coder, conectividad P2P y emisión de alertas visuales hacia la barra de escritorio:

```bash
# Diagnosticar telemetría consolidada de la barra
antos barra status
antos barra

# Emitir una alerta informativa a la shell de escritorio
antos barra alert "Sincronización de worktrees completada"

# Emitir una alerta visual urgente
antos barra alert "Intento de violación de sandbox bloqueado por eBPF" --urgent
```

---

### 4.24 Pipeline de Arranque Bare Metal y Emulación QEMU (`antos boot`)

Automatización de la compilación cruzada para los targets `x86_64-unknown-none` y `aarch64-unknown-none`, generación de imágenes de disco arrancables BIOS/MBR y UEFI GPT (FAT32 ESP), y validación en máquinas virtuales QEMU:

```bash
# Diagnosticar estado de los artefactos (kernel ELF, imágenes BIOS/UEFI y disponibilidad de QEMU)
antos boot status
antos boot

# Compilar el kernel no_std y generar las imágenes arrancables de disco
antos boot build

# Ejecutar prueba automatizada de arranque en QEMU headless con verificación por serial
antos boot test

# Lanzar la máquina virtual QEMU x86_64 de forma interactiva
antos boot qemu

# Lanzar la máquina virtual QEMU AArch64 (ARM 64-bit virt con consola serie PL011)
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/debug/kernel \
  -serial stdio -monitor none
```

---

### 4.25 Motor de Capacidades y Plugins WebAssembly (`antos plugin`)

Arquitectura de extensibilidad modular en WebAssembly (WASM / WASI) con aislamiento de memoria lineal (cuota configurable de hasta 64 MB), limitación de ciclos de instrucción (*fuel metering*) e integración segura con llamadas de antOS:

```bash
# Listar los plugins WebAssembly instalados y sus capacidades
antos plugin list
antos plugin

# Instalar un nuevo plugin desde un directorio que contenga plugin.toml y el binario .wasm
antos plugin install ./mis-plugins/markdown-formatter

# Ejecutar una acción dentro de un plugin WASM en entorno aislado
antos plugin run markdown-formatter format target=README.md

# Ejecutar con parámetros clave=valor
antos plugin run json-validator validate schema=strict.json
```

---

### 4.26 Captura de Pantalla Wayland e Inspección Visual Multimodal (`antos screenshot` / `antos qa visual`)

Herramientas para captura gráfica (`wlr-screencopy` / `grim` / portal Wayland) y auditoría de interfaces con el rol especializado `VisualQA` de antFlow mediante LLMs de visión (Llava, Moondream, MiniCPM-V vía Ollama) o análisis heurístico determinista:

```bash
# Capturar la pantalla completa o una ventana activa
antos screenshot antos-barra capturas/barra.png
antos screenshot capturas/escritorio.bmp

# Lanzar una inspección de calidad visual (VisualQA) contra criterios de diseño
antos qa visual antos-barra "verificar_contraste_accesible" "comprobar_alineacion_geométrica"

# Inspección visual con criterios por defecto
antos qa visual desktop
```

---

### 4.27 Live ISO y Empaquetado Release (`antos boot iso` / `antos release`)

Construcción automatizada de la imagen Live ISO autoarrancable (con soporte híbrido BIOS/MBR y NixOS) y pipeline oficial de empaquetado de distribución con sumas SHA256 para el lanzamiento v0.1.0:

```bash
# Generar la imagen Live ISO híbrida autoarrancable (antos-live-x86_64.iso)
antos boot iso

# Ejecutar el pipeline de empaquetado release completo (tarball + Live ISO + SHA256SUMS)
antos release
antos boot release
```

---

### 4.28 Inspección de Almacenamiento y Particionamiento GPT (`antos disk`)

Subsistema de descubrimiento de hardware de almacenamiento (NVMe, SATA, USB, VirtIO), diagnóstico de tablas de particiones y generador de esquemas GPT con alineación de sectores a 1 MiB para instalación limpia o Dual Boot pacífico:

```bash
# Enumerar discos físicos, tamaños, buses y particiones hijas
antos disk list

# Inspeccionar mapa de particiones, UUIDs y sistemas de archivos de un disco
antos disk inspect /dev/nvme0n1
antos disk inspect /dev/disk0

# Calcular y previsualizar esquema de particionado GPT (Dry-Run seguro por defecto)
antos disk partition /dev/nvme0n1
antos disk partition /dev/nvme0n1 --clean

# Aplicar particionado GPT definitivo (acción destructiva controlada)
antos disk partition /dev/nvme0n1 --clean --apply
```

---

### 4.29 Asistente e Instalador de Sistema Base a Disco Duro (`antos install`)

Motor de despliegue guiado y no interactivo para instalar antOS en el disco duro o unidad NVMe/SSD de la máquina. Permite instalar como **sistema operativo principal** (particionamiento y formateo limpio de disco completo) o como **sistema secundario en Dual Boot** (preservando la partición EFI ESP y sistemas operativos Windows o Linux preexistentes):

```bash
# Listar discos compatibles y recomendación de modo (Principal vs Dual Boot)
antos install list

# Asistente guiado e interactivo por terminal (detecta discos y simula instalación)
antos install wizard
antos install gui

# Despliegue en modo Dual Boot (preservando Windows/Linux y cargadores existentes, simulación segura)
antos install run --target /dev/nvme0n1 --dual-boot --user antos --host antos-box

# Despliegue en modo Sistema Principal Limpio (simulación segura)
antos install run --target /dev/sda --clean

# Aplicar la instalación definitiva en el hardware (acción destructiva controlada)
antos install run --target /dev/nvme0n1 --dual-boot --apply
```

---

### 4.30 Gestor de Arranque UEFI y Dual Boot (`antos bootloader`)

Subsistema de integración con firmware UEFI y `systemd-boot`, con detección automática de sistemas operativos vecinos en la partición ESP (Windows Boot Manager, Ubuntu, Fedora, Arch, Debian) y registro en la NVRAM con `efibootmgr`:

```bash
# Sondear sistemas operativos instalados y particiones EFI
antos bootloader probe
antos bootloader probe --esp /boot/efi

# Instalar y validar configuración de systemd-boot (Dry-Run seguro por defecto)
antos bootloader install
antos bootloader install --target /dev/nvme0n1 --partition 1 --timeout 5

# Aplicar e inscribir entrada NVRAM en el firmware UEFI en hardware real
antos bootloader install --target /dev/nvme0n1 --partition 1 --apply
```

---

### 4.31 MicroVMs Efímeras y Aislamiento por Hipervisor (`antos vm`)

Aprovisionamiento y ejecución de entornos de ejecución efímeros ultra-aislados mediante hipervisores basados en KVM / Cloud-Hypervisor:

```bash
# Enumerar microVMs activas y recursos asignados
antos vm list

# Lanzar una microVM efímera con kernel directo y límites de CPU/RAM
antos vm spawn --vcpus 2 --memory 512

# Ejecutar un comando dentro de una microVM aislada
antos vm exec uvm-abc12345 "uname -a"

# Destruir y liberar recursos de una microVM
antos vm destroy uvm-abc12345
```

---

### 4.32 Gestor de Paquetes y Recetas Inmutables (`antos pkg`)

Gestor de paquetes inmutable y reproducible `antpkg` basado en almacén direccionado por contenido (CAS) con rollback instantáneo por generaciones:

```bash
# Instalar paquete o receta declarativa TOML
antos pkg install curl
antos pkg install recetas/ripgrep.toml

# Simular instalación (Dry-Run)
antos pkg install jq --dry-run

# Listar paquetes en el perfil activo y generaciones anteriores
antos pkg list
antos pkg ls

# Desinstalar un paquete del perfil activo
antos pkg remove curl

# Revertir el perfil activo a una generación previa
antos pkg rollback
antos pkg rollback --generation 1
```

---

### 4.33 Modo Agente Autónomo Continuo (`antos autopilot`)

Modo de vigilancia y resolución proactiva de incidencias en segundo plano (*Autopilot Daemon*):

```bash
# Consultar estado del centinela y métricas de incidencias
antos autopilot status

# Iniciar vigilancia en segundo plano (intervalo en segundos)
antos autopilot start --interval 60 --max-concurrent 2

# Escanear el workspace manualmente en busca de incidencias
antos autopilot scan

# Aprobar y fusionar una propuesta de corrección generada por el centinela
antos autopilot resolve inc-xyz789 --approve

# Descartar una propuesta
antos autopilot resolve inc-xyz789 --reject

# Detener el centinela
antos autopilot stop
```

---

### 4.34 Consola Web Remota en Tiempo Real y Bridge WebSocket (`antos web`)

Interfaz web ligera embebida en `antosd` para monitoreo remoto, telemetría y ejecución de comandos mediante WebSocket:

```bash
# Iniciar servidor de consola web en dirección y puerto configurados
antos web start --addr 127.0.0.1 --port 9090

# Consultar estado del servidor web y clientes conectados
antos web status

# Generar token seguro de acceso temporal
antos web token --ttl 3600 --label "laptop-remota"

# Detener servidor web
antos web stop
```

---

### 4.35 Gestión de Proyectos, Selección Activa y Git en Workspace (`antos use` / `antos project` / `antos git`)

Comandos declarativos y de frontera para gestionar proyectos en `workspace/`, fijar el proyecto de trabajo activo (para que todos los comandos del sistema operen sobre él de forma automática sin necesidad de cambiar de directorio) e inicializar repositorios Git aislados sin contaminar el repositorio del sistema operativo antOS:

```bash
# 1. Fijar el proyecto de trabajo activo
# (A partir de este momento: tickets, git, diffs, agentes, panel, etc. operarán sobre 'api-service')
antos use api-service
antos project use api-service

# 2. Consultar el proyecto activo actual y su origen (vía 'use' o detección automática)
antos use
antos project current

# 3. Listar todos los proyectos en workspace/ (destacando con [ACTIVO] el seleccionado)
antos project list
antos project ls

# 4. Operar directamente sobre el proyecto activo desde cualquier directorio
antos tickets           # Lista los tickets del proyecto activo
antos git status        # Inspecciona el repositorio Git del proyecto activo
antos git diff          # Previsualiza cambios locales del proyecto activo
antos panel             # Despliega el tablero Kanban del proyecto activo

# 5. Restablecer la selección activa (volver al ámbito global o automático por CWD)
antos use --clear

# 6. Inicializar repositorio Git aislado en un proyecto con rama 'main' y .gitignore adaptado
antos project init api-service
antos git init api-service

# Especificar rama principal y stack tecnológico explícitamente
antos project init web-frontend --branch develop --lang typescript

# Inicializar Git en el proyecto actual si la terminal ya está dentro de su directorio
cd workspace/api-service
antos project init
```

---

### 4.36 Espacio de Trabajo Integrado Dev TUI (`antos dev`)

Entorno de desarrollo unificado en terminal multipanel (T20.1) que integra **Neovim** (editor predeterminado), el panel lateral de monitoreo de agentes multi-nodo (`antFlow`), el visor interactivo de diffs y la terminal inferior colapsable:

```bash
# 1. Iniciar espacio de trabajo interactivo (Neovim + agentes + diffs)
antos dev

# 2. Iniciar en un proyecto específico
antos dev --project api-service
antos dev -p mi-app

# 3. Consultar estado, configuración y atajos registrados
antos dev --status

# 4. Previsualización en seco (renderizado sin tomar control del TTY)
antos dev --preview

# Atajos dentro del espacio de trabajo:
#   Ctrl + B        Alternar visibilidad del panel lateral de agentes (antFlow)
#   Ctrl + T        Abrir/cerrar terminal inferior integrada
#   Ctrl + D        Abrir visor de diffs y git status
#   Super + W       Lanzador global desde el shell Wayland de antOS
```

---

### 4.37 Reproducción Autónoma de Bugs TDD y Generación de Tests (`antos reproduce` / `antos testgen`)

Motor autónomo de ingeniería inversa de fallas y desarrollo dirigido por pruebas (TDD / T20.2). Ingesta volcados de pila y excepciones multilingües (Rust, Python, JavaScript/TypeScript), aísla el contexto del error, sintetiza tests reproducibles que fallan inicialmente (Fase *Red*), verifica parches correctivos (Fase *Green*) y certifica la protección contra regresiones (Fase *Verified*):

```bash
# 1. Reproducir un error o panic de Rust directamente desde el stack trace
antos reproduce "thread 'main' panicked at 'index out of bounds: the len is 3 but the index is 5', src/parser.rs:42:15"

# 2. Reproducir un traceback de Python indicando archivo objetivo
antos reproduce --target app/loader.py "Traceback (most recent call last): File 'app/service.py', line 88... FileNotFoundError: Missing schema file"

# 3. Reproducir leyendo la traza de un archivo de log
antos reproduce --log /var/log/app/crash.log
antos reproduce -l crash.log --target src/handler.rs

# 4. Generar tests unitarios e invariantes para un módulo o archivo fuente
antos testgen src/buffer.rs
antos testgen --target src/auth.rs --cases 4 --suite unit

# 5. Generar tests para proyectos en Python o JavaScript
antos testgen --target lib/parser.py --cases 3
antos testgen --target src/api.ts --cases 5 --suite regression

# 6. Uso declarativo mediante lenguaje natural
antos "reproduce el error: thread 'worker' panicked at 'division by zero', src/calc.rs:25:9"
antos "genera tests para src/service.rs"
```

---

## 5. Recetas y Combinaciones de Uso Avanzadas

### 🔹 Receta 1: Modo Autónomo Nocturno o Larga Duración (`/goal`)
Ejecuta un ciclo completo de desarrollo desatendido para un ticket técnico:
```bash
# 1. Definir cuotas para evitar bloqueos
antos quota set --timeout 300 --memory 4096

# 2. Despachar el ticket al equipo con confirmación automática
antos agent run T2.1 --auto

# 3. Monitorear desde el panel
antos panel
```

### 🔹 Receta 2: Auditoría y Despliegue con Bóveda y Servicios
```bash
# 1. Levantar servicios efímeros
antos service up postgres 5432
antos service up redis 6379

# 2. Conceder credenciales por 10 minutos
antos grant secret.DB_PASSWORD --minutos 10 --para "migraciones y pruebas de integración"

# 3. Ejecutar pruebas en el entorno sincronizado
antos env sync
cargo test --workspace

# 4. Apagar servicios al concluir
antos service down postgres
antos service down redis
antos revoke secret.DB_PASSWORD
```

### 🔹 Receta 3: Inspección Visual y Aprobación Rápida
```bash
# 1. Ver alertas y revisiones pendientes
antos notify list

# 2. Inspeccionar el diff estructurado del ticket listo para revisión
antos diff T8.1

# 3. Aprobar y fusionar
antos notify approve notif-t8-1
```
