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
     - [E. Escritorio Nativo Bare-Metal y Shell Interactivo Soberano (Fase 26)](#e-escritorio-nativo-bare-metal-y-shell-interactivo-soberano-fase-26)
     - [F. Soporte Real del Protocolo de Arranque Limine (Fase 27)](#f-soporte-real-del-protocolo-de-arranque-limine-fase-27)
     - [G. Periféricos Nativos y Endurecimiento Runtime (Fase 28 y depuración en hipervisores)](#g-periféricos-nativos-y-endurecimiento-runtime-fase-28-y-depuración-en-hipervisores)
   - [Método 7: Live USB Booteable e Instalación en Hardware Real (Bare Metal)](#método-7-live-usb-booteable-e-instalación-en-hardware-real-bare-metal)
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
   - [4.38 Matriz de CI/CD Local Paralela y Git Hooks Inteligentes (`antos ci` / `antos hook`)](#438-matriz-de-cicd-local-paralela-y-git-hooks-inteligentes-antos-ci--antos-hook)
   - [4.39 Instantáneas Atómicas de Entorno y Time Machine de Estado (`antos snapshot`)](#439-instantáneas-atómicas-de-entorno-y-time-machine-de-estado-antos-snapshot)
   - [4.40 Benchmarking Continuo y Detección de Regresiones de Rendimiento (`antos bench`)](#440-benchmarking-continuo-y-detección-de-regresiones-de-rendimiento-antos-bench)
   - [4.41 Sincronización con Forjas Git: Issues y Pull Requests (`antos issue` / `antos pr`)](#441-sincronización-con-forjas-git-issues-y-pull-requests-antos-issue--antos-pr)
   - [4.42 Generador y Sincronizador de Documentación Viva y Diagramas Mermaid (`antos doc`)](#442-generador-y-sincronizador-de-documentación-viva-y-diagramas-mermaid-antos-doc)
   - [4.43 Gestor y Grabador Seguro de Memorias Live USB (`antos usb`)](#443-gestor-y-grabador-seguro-de-memorias-live-usb-antos-usb)
   - [4.44 Gestor y Puente de Aplicaciones Flatpak y Contenedores Gráficos (`antos app`)](#444-gestor-y-puente-de-aplicaciones-flatpak-y-contenedores-gráficos-antos-app)
   - [4.45 Abstracción de Plataforma Runtime (`antos runtime`)](#445-abstracción-de-plataforma-runtime-antos-runtime)
5. [Recetas y Combinaciones de Uso Avanzadas](#5-recetas-y-combinaciones-de-uso-avanzadas)

---

## 1. Métodos de Arranque del Sistema Operativo

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                      7 MODOS DE EJECUCIÓN DE antOS                          │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 1. CLI y Centro de Control (Host): Desarrollo diario en macOS y Linux       │
 │ 2. Demonio IPC en Segundo Plano: Escucha en socket UNIX y atiende clientes  │
 │ 3. Shell Gráfico Wayland (GTK4): HUD contextual flotante y Kanban (Super+A) │
 │ 4. Contenedor Linux (Landlock LSM): Verificación de aislamiento kernel      │
 │ 5. Máquina Virtual NixOS en QEMU: Sistema operativo completo y servicios    │
 │ 6. Kernel Bare-Metal no_std en QEMU: x86_64 (BIOS/UEFI) y AArch64 (ARM64)   │
 │ 7. Live USB e Instalación Bare-Metal: Instalación en NVMe/SATA físicos       │
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

Una imagen de NixOS con antOS integrado como demonio de sistema `systemd`
(`antos.service`, T31.18), su `antos-doctor` de arranque y —opcionalmente— la
sesión gráfica Wayland completa. **Tres formas de arrancarla:**

| Quieres… | Comando | Cómo |
| :--- | :--- | :--- |
| **El escritorio, en macOS, usable** | `./system/arrancar-vm-macos.sh` | Construye la ISO en vivo en un contenedor y arranca **QEMU en el host con `-accel hvf`**. El invitado AArch64 va casi a velocidad nativa y la sesión Wayland arranca en segundos. **Recomendado en Apple Silicon** (T30.7). |
| **Ver el arranque por consola** (CI, depurar) | `./system/arrancar-vm.sh` | VM headless, consola serie, construida y ejecutada **dentro** del contenedor. Rápida de iterar. Salir de QEMU: `Ctrl-A` y luego `X`. |
| **El escritorio sin salir del contenedor** | `./system/arrancar-vm.sh --grafica` | VM gráfica por **VNC en `localhost:5901`**. QEMU corre dentro del contenedor **sin KVM → TCG**: lento y, para la sesión Wayland, **inestable** (aviso abajo). Vale para un vistazo. |

> ⚠️ **`--grafica` bajo TCG no sostiene la sesión gráfica (T30.6).** Sin
> aceleración por hardware, `labwc` sale por *timeout* del *ping* de
> `libseat`→`logind` (D-Bus) y por una carrera con el KMS de `virtio-gpu`;
> `greetd` lo reinicia en bucle y los clientes (`antos-barra`, `waybar`) se
> apilan. El `autostart` de la sesión es idempotente (guarda `_running`,
> un `pgrep` anclado al ejecutable) para que el bucle no duplique nada, pero
> el arreglo real es **HVF/KVM**: usa
> `arrancar-vm-macos.sh` en macOS, o `./system/arrancar-vm.sh` (headless) en un
> Linux con `/dev/kvm`.

#### `arrancar-vm-macos.sh` — arranque acelerado en macOS (T30.7)

```bash
./system/arrancar-vm-macos.sh                # construye la ISO si falta y abre la ventana cocoa (HVF)
./system/arrancar-vm-macos.sh --build-only   # solo construye y copia -> target/antos-linux-aarch64.iso
./system/arrancar-vm-macos.sh --headless     # serie a stdio, sin ventana (CI / depuración)
./system/arrancar-vm-macos.sh --rebuild      # fuerza reconstruir la ISO
./system/arrancar-vm-macos.sh --fullscreen   # pantalla completa: resolución Retina nativa + escala 2
ANTOS_VM_RES=1680x1050 ./system/arrancar-vm-macos.sh   # tamaño inicial de la ventana (1440x900 por defecto)
```

> La resolución del invitado **es** el tamaño de la ventana Cocoa en puntos
> (QEMU se lo pasa por el EDID de `virtio-gpu`). En un Mac Retina la ventana
> normal se dibuja escalada a 2×; solo `--fullscreen` entrega la resolución
> nativa, y entonces `services.antos.desktop.outputScale = "auto"` pone el
> escritorio a escala 2 para que panel, barra y fuentes no salgan
> minúsculos. Para diagnosticar sin ventana: `nc -U
> target/antos-vm-serial.sock` (consola serie, shell `nixos`) y `nc -U
> target/antos-vm-monitor.sock` (monitor de QEMU). Tras 10 min sin actividad
> la sesión se bloquea (`swaylock`, contraseña `antos`);
> `services.antos.desktop.panel.idleLockSeconds = 0` lo desactiva.
>
> **Sabor del escritorio (T30.9).** La ISO en vivo arranca **KDE Plasma 6
> Wayland** (`services.antos.desktop.flavor = "plasma"` en `iso.nix`) con
> `antos-barra` anclada arriba y su icono en la bandeja de Plasma; la escala
> HiDPI se aplica con `kscreen-doctor`. `flavor = "labwc"` (por defecto en
> el módulo, y lo que usa la VM de CI) devuelve la sesión ligera Labwc +
> mobiliario `wlroots` de T30.6. El flake se construye con `git+file:///src`:
> Nix solo ve ficheros seguidos por git (`git add` para los nuevos), y así
> `target/` no acaba copiado al store del volumen `antos-nix-store`.
>
> **Arte.** El icono y el fondo oficiales viven en `system/desktop/assets/`
> (`antos-icon.png`, `antos-wallpaper.png`); `system/nixos/branding.nix` los
> deriva en construcción (menú de GRUB, Plymouth, fondo de escritorio y de
> bloqueo de Plasma, icono del lanzador, fondo de Labwc). Para cambiarlos
> basta sustituir esos dos ficheros y reconstruir (`--rebuild`).

Requiere macOS con `qemu` (`brew install qemu`) y `podman`. Construye
`.#iso` en el contenedor `nixos/nix` (la primera vez descarga el cierre
entero; la ISO pesa ~2.4 GB), la copia a `target/`, y arranca
`qemu-system-aarch64` **en el host** con `-accel hvf -cpu host -M virt`,
firmware UEFI EDK2 (el `edk2-aarch64-code.fd` de la propia instalación de
QEMU; las variables se crean en `target/edk2-aarch64-vars.fd`),
`virtio-gpu-pci` + `-display cocoa` y entrada USB (`usb-tablet` = puntero
absoluto). Ruta de arranque verificada: UEFI → GRUB → kernel NixOS →
`antos-doctor` pasa (Landlock, cgroups-v2, eBPF-LSM) → `antos.service`
arranca → sesión gráfica. Se niega con un mensaje claro fuera de macOS o si
falta `qemu`/firmware.

Para **UTM**: importa esa misma ISO (`target/antos-linux-aarch64.iso`),
Display `virtio-gpu-pci`, entrada USB, ≥ 3 GiB de RAM — UTM usa HVF
automáticamente para invitados AArch64.

#### Escritorio antOS Linux (Wayland + `antos-barra`) — T30.1

El módulo `services.antos.desktop` (`system/nixos/desktop.nix`) describe la
sesión gráfica completa de forma declarativa: compositor **Labwc** +
**`antos-barra`** (GTK4 layer-shell) + terminal + **Neovim** + Git + autologin
Wayland por `greetd`. Reutiliza los mismos `rc.xml` / `autostart` /
`environment` de `system/desktop/` (T13.0).

```nix
# system/nixos/configuration.nix
services.antos.enable = true;
services.antos.desktop.enable = true;   # ← una línea activa el escritorio
```

Opciones (`services.antos.desktop.*`): `compositor` (por defecto `pkgs.labwc`),
`autologinUser` (`antos`), `terminal` (`pkgs.foot`), `editor` (`pkgs.neovim`),
`barra` (`pkgs.antos-barra`).

```bash
# Evaluar la máquina física con el escritorio (sin construir la imagen)
nix eval .#nixosConfigurations.antos-desktop.config.system.build.toplevel.drvPath

# Construir el paquete de la barra por separado
nix build .#antos-barra          # -> result/bin/antos-barra
```

##### Primer arranque: `antos setup` y `antos doctor --desktop` (T36.5)

La primera sesión del escritorio abre una terminal con `antos setup`
(autostart XDG bajo Plasma, paso 5 del `autostart` de Labwc; solo una vez,
mientras no exista `$ANTOS_STATE/setup.toml`). Es idempotente: cada paso
detecta si ya está hecho (`→ saltado (ya hecho)`) y una segunda pasada no
cambia ningún fichero.

| Paso | Qué hace | Cómo se salta |
| :--- | :--- | :--- |
| identidad git | `user.name` / `user.email` en `~/.config/git/config` | ya presentes (ahí o en `~/.gitconfig`) |
| clave SSH | `ssh-keygen -t ed25519` en `~/.ssh/id_ed25519`; muestra la pública | ya existe, o «n» |
| flathub | `flatpak remote-add --if-not-exists --user flathub …` (también lo hace el servicio de usuario `antos-flathub`) | sin `flatpak`, o ya presente |
| modelos | perfil `local` / `hybrid` / `cloud` (T34.4); descarga del modelo solo con confirmación | «saltar» |
| claves API | `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` a la bóveda cifrada, tecleadas sin eco | ya en la bóveda, o «n» |
| gh auth | `gh auth login` en primer plano | sin `gh`, ya autenticado, o «n» |

```bash
antos setup                          # interactivo
antos setup --yes --config setup.toml   # sin preguntas: git_name, git_email, ssh_key, flathub,
                                     # llm_profile, pull_model, gh_login (las claves API nunca van en el TOML)
antos setup --status                 # qué quedó hecho la última vez
antos doctor --desktop               # recinto del usuario, sesión Wayland, demonio por el socket,
                                     # antos-barra, ollama (aviso), flathub (aviso), identidad git (aviso),
                                     # rc.xml/autostart de Labwc editados con versión nueva pendiente (aviso)
```

> **antOS Linux es monousuario en esta versión.** El usuario del escritorio
> es el dueño del recinto (`ANTOS_STATE`, `ANTOS_WORKSPACE`) y del socket
> `0600` del demonio (`services.antos.user = autologinUser`, que el módulo
> exige con una `assertion`). `antos doctor --desktop` lo comprueba: si lo
> ejecuta otro usuario, lo dice en vez de fallar de forma críptica. Un
> demonio por usuario es un ticket futuro.
>
> La sesión de Labwc instala `rc.xml` / `autostart` / `menu.xml` en
> `~/.config/labwc` solo si faltan o si no los has tocado (guarda una copia
> de lo que instaló en `.antos-orig-<fichero>`); una edición tuya se
> respeta y `doctor --desktop` avisa cuando hay una versión nueva.

##### Actualizar y deshacer: `antos system` (T36.6)

Actualizar antOS Linux es una intención más: ver el diff, aprobar,
aplicar, poder deshacer. Todo sobre `/etc/nixos` (el `flake.nix` que dejó
`antos install`) y con lo que ya trae `nix` — `nix flake update`, `nix
build`, `nix store diff-closures` (sin `nvd`), `nixos-rebuild switch` /
`--rollback` / `list-generations`. `sudo` lo invoca el CLI en primer
plano; el demonio nunca.

```bash
antos system update            # copia /etc/nixos a $ANTOS_STATE/system-update, refresca el flake.lock,
                               # construye (del caché de T36.3 si lo hay), muestra el diff de closures,
                               # pide confirmación, sudo nixos-rebuild switch, y devuelve el lock a /etc/nixos
antos system update --check    # solo evalúa: salida 0 sin cambios, 10 con cambios; deja update-available.json
antos system update --source github:juandevelop85/antOS/v0.2.0   # cambia la fuente de antOS (por defecto path:/etc/nixos/antos)
antos system update --yes      # sin confirmación
antos system rollback [N]      # sudo nixos-rebuild switch --rollback (o --switch-generation N), con bitácora
antos system generations       # las generaciones del sistema, la actual marcada
antos system status            # lo que dejó el último --check
antos undo                     # si lo último ejecutado fue una actualización, la deshace con rollback
```

- Sin red a la fuente (`github:…`), `update` lo dice y no toca nada; con
  `path:` no hace falta red.
- Si `configuration.nix` cambió después de la última generación, el diff lo
  avisa: «además de la actualización, se aplicarán tus cambios locales».
  `antos-paquetes.nix` no se toca nunca.
- Cada actualización queda en la bitácora (`antos log`) como
  `system.update` con la generación anterior y la nueva y el resumen del
  diff; el rollback como `system.rollback`.
- Un temporizador de usuario (`antos-update-check`, diario) ejecuta
  `--check`; la barra leerá `update-available.json` para mostrar
  «actualización disponible» cuando un ticket de barra lo implemente.
  Nada se descarga ni se aplica solo.

##### Máquina física vs VM de desarrollo (T36.4)

`nixosConfigurations.antos-desktop` es **la máquina física**: `installed.nix`
(lo que `antos install` genera) + `services.antos.machine`
([`machine.nix`](../system/nixos/machine.nix)): NetworkManager, firmware
propietario, PipeWire (+ `rtkit`), bluetooth (+ `blueman` con Labwc),
teclado coherente en consola / Labwc (`XKB_DEFAULT_LAYOUT`) / Plasma
(`services.xserver.xkb`), `i18n.defaultLocale`, `time.timeZone`, `sudo`
con contraseña, `power-profiles-daemon`, `fstrim`, `zramSwap` y
`systemd-boot`. Opciones: `keyboardLayout` (`us`), `keyboardVariant`,
`locale` (`en_US.UTF-8`), `timeZone` (`UTC`); las escribe el asistente.
Nada de VM: lo que solo tiene sentido en QEMU (`qemu-guest.nix`, consola
serie, `root` con sesión abierta y contraseña `antos`) está en
[`vm-common.nix`](../system/nixos/vm-common.nix) y solo lo heredan
`antos`, `antos-vm` y `antos-desktop-vm`. `desktop.nix` ya **no** pone
ninguna contraseña por defecto: la VM y la ISO en vivo ponen la suya, la
máquina instalada lleva la que el usuario eligió.

Sobre un Linux **no-NixOS** (desarrollo), la sesión se lanza con el guion
equivalente, que instala la misma configuración de Labwc:

```bash
system/desktop/start-session.sh          # requiere `labwc` en el PATH
```

#### VM gráfica e ISO instalable de antOS Linux — T30.2

Tres variantes de máquina (flake):

| Salida | Qué es | Comando |
| :--- | :--- | :--- |
| `nixosConfigurations.antos-vm` | VM headless (serie), para CI | `./system/arrancar-vm.sh` |
| `nixosConfigurations.antos-desktop-vm` | VM **gráfica**: arranca directa al escritorio antOS (`virtio-gpu` + `usb-tablet`, 3 GiB). Por VNC en el contenedor (TCG) o con HVF si se usa `system.build.vm` en un host con aceleración | `./system/arrancar-vm.sh --grafica` |
| `packages.<arch>.iso` / `nixosConfigurations.antos-iso-{x86_64,aarch64}` | **ISO en vivo e instalable de antOS Linux** (`installation-cd-graphical-base` + `services.antos.desktop`). **Autosuficiente (T36.3):** lleva la closure de la máquina instalada de referencia (`installed.nix`), el árbol fuente de antOS en `/etc/antos/source` y la fuente de `nixpkgs` en el store, para que `antos install --apply` funcione **sin red**. Es la que arranca `arrancar-vm-macos.sh` por HVF, la que se importa en UTM/VirtualBox y la que publica la release | `nix build .#iso` → `result/iso/antos-linux-<versión>-<arch>.iso` |
| `nixosConfigurations.antos-installed-{x86_64,aarch64}` | La **máquina instalada de referencia**: el espejo en Nix de lo que `antos install` genera (mismos paquetes; el test `test_generated_configuration_matches_installed_nix` los mantiene alineados) | `nix build .#nixosConfigurations.antos-installed-x86_64.config.system.build.toplevel` |

```bash
# macOS, escritorio usable: construye la ISO y la arranca con HVF en el host
./system/arrancar-vm-macos.sh

# Vistazo rápido sin salir del contenedor (VNC en localhost:5901, lento bajo TCG)
./system/arrancar-vm.sh --grafica

# Solo la ISO en vivo (NO es el Live del kernel bare-metal de `antos usb build`, Método 6)
nix build .#iso
```

##### ISO de la release, firma y caché binario (T36.3)

El workflow [`release.yml`](../.github/workflows/release.yml) construye la
ISO de las dos arquitecturas en cada push a `master` (artefacto de 7 días)
y, en un tag `v*`, publica una **GitHub Release** con las ISO, `SHA256SUMS`
y `SHA256SUMS.sig`. Verificación al descargar:

```bash
sha256sum -c SHA256SUMS
# Firma (ssh-keygen -Y, sin herramienta nueva); la clave pública está en docs/release-signing-key.pub
printf 'antos-release %s\n' "$(grep -v '^#' docs/release-signing-key.pub)" > allowed
ssh-keygen -Y verify -f allowed -I antos-release -n antos-release -s SHA256SUMS.sig < SHA256SUMS
```

El mismo workflow (job `nix-cache`) firma y publica las closures de
`antosd` y `antos-barra` como **caché binario estático** en la rama
`nix-cache` (`nix copy --to file://…`), que GitHub Pages sirve bajo
`https://juandevelop85.github.io/antOS/cache`; el módulo
[`cache.nix`](../system/nixos/cache.nix) (`services.antos.binaryCache`) lo
añade como *substituter* a toda máquina antOS — también a la que genera
`antos install` — **solo si hay clave pública conocida**. Un
`nixos-rebuild switch` con caché descarga `antosd`/`antos-barra`; sin caché
los compila (varios minutos, ≥ 4 GiB de RAM).

Puesta en marcha, una sola vez, por el mantenedor (los dos pares de claves
son suyos; el repositorio solo lleva las públicas):

```bash
# Firma de releases
ssh-keygen -t ed25519 -N '' -f release.key -C antos-release
# → contenido de release.key  ⇒ secreto RELEASE_SIGNING_KEY del repositorio
# → release.key.pub           ⇒ docs/release-signing-key.pub (sustituyendo los comentarios)

# Caché binario
nix key generate-secret --key-name antos-cache-1 > cache.key
nix key convert-secret-to-public < cache.key
# → contenido de cache.key    ⇒ secreto NIX_CACHE_SIGNING_KEY
# → la línea pública          ⇒ system/nixos/cache-public-key.txt
# y en Settings → Pages: servir la rama `nix-cache` (raíz).
```

Sin esos secretos el workflow lo avisa: la release sale sin firma y el
caché no se publica; nada se rompe, solo falta.

> ℹ️ Para el porqué de HVF vs. TCG y el bucle de `greetd`, ver el aviso al
> principio del Método 5 y el hallazgo de T30.6. Resultado esperado del
> escritorio: `antos-barra` anclada arriba, `waybar` abajo (panel de T30.6).

#### Herramientas de desarrollo incluidas — T30.3

`services.antos.desktop.devTools` (por defecto `true`) añade a la imagen, de
forma declarativa:

| | |
| :--- | :--- |
| **Editor** | `neovim` (por defecto), configurado contra el LSP unificado de antOS. **No hay binario `antos-lsp`**: es el subcomando `antos lsp` (lo aporta `antosd`). `nvim` registra el *language server* `antos_lsp` con `cmd = { "antos", "lsp" }`. |
| **Git** | `git`, `gh`, `delta`, `direnv` + `nix-direnv` |
| **CLI** | `tmux`, `ripgrep`, `fd`, `bat`, `jq`, `htop`, `fastfetch`, `yazi`, `wget`, `curl` |
| **Navegador** | `firefox` (opción `services.antos.desktop.browser`, `null` para omitir) |
| **Apps** | `services.flatpak.enable` para `antos app`. Añade el remoto una vez: `flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo` |

```bash
# Dentro de la sesión antOS:
antos dev        # o Super+W — layout Neovim + monitor de agentes + diffs + VTE (T20.1)
antos edit <f>   # o Super+E — abre el editor por defecto
antos lsp config # genera la config del LSP para nvim/vscode/helix/emacs
antpkg install <paquete>       # o `antos app install <flatpak>`
```

#### Panel de escritorio tradicional — T30.6

`services.antos.desktop.panel` (por defecto **`enable = true`**) añade a la
sesión Labwc, sin dejar de ser `wlroots` ligero:

| Pieza | Herramienta | Nota |
| :--- | :--- | :--- |
| Panel inferior | `waybar` | menú (`≡ Aplicaciones` → `fuzzel`), reloj, CPU/RAM/red, bandeja |
| Lanzador | `fuzzel` | `Super+P`; escanea los `.desktop` del sistema |
| Fondo | `swaybg` | color sólido, o `panel.wallpaper = ./ruta.png` |
| Notificaciones | `mako` | |
| Bloqueo / idle | `swaylock` + `swayidle` | `Super+L`; auto-bloqueo a los 600 s |
| Archivos | `thunar` | entrada del menú de clic derecho |

Opciones: `panel.{enable,package,launcher,fileManager,wallpaper}`. Menú de
clic derecho en `/etc/antos/desktop/labwc/menu.xml`. Atajos nuevos en
`rc.xml`: `Super+P` (lanzador), `Super+L` (bloquear), `Print` (captura a
`~/Pictures`). `panel.enable = false` devuelve la sesión mínima de solo
`antos-barra`.

#### Verificación del escritorio — T30.4

**Smoke automatizado** (`system/desktop/smoke.sh`, *job* `antos-linux-desktop`
de CI): arranca un compositor `wlroots` sin pantalla (`labwc` + backend
headless) + `antosd` + `antos-barra` y comprueba `zwlr_layer_shell_v1`, el
socket IPC, que la barra sobrevive y una captura `grim` no vacía. Ejecutable en
un Linux de desarrollo:

```bash
ANTOS_BIN=target/release/antos \
ANTOS_BARRA_BIN=system/barra/target/release/antos-barra \
  bash system/desktop/smoke.sh        # -> "SMOKE OK" o "SMOKE FALLO: <causa>"
```

**Integración host** (sin GUI, corre en cualquier plataforma):
`cargo test -p antosd --test desktop_ipc` — lanza `antos demonio`, le envía una
`Request::Intent` por el socket UNIX igual que la barra, y verifica que
despacha y emite el flujo de `Event` (`Start` → terminal) y que responde a
`QueryGitStatus` (la insignia de git de la barra).

**Checklist manual** en la VM gráfica — hazla sobre `arrancar-vm-macos.sh`
(HVF) o un Linux con `/dev/kvm`; bajo TCG (`--grafica` en contenedor) la
sesión no se sostiene el tiempo suficiente (T30.6):

| # | Comprobación | Esperado |
| :- | :--- | :--- |
| 1 | La sesión arranca por `greetd` sin login manual | Escritorio antOS visible; `systemctl status greetd` activo, sin reinicios |
| 2 | `systemctl status antos-doctor` y `systemctl status antos` | `antos-doctor` `active (exited)` sin fallos de recinto; `antos` `active (running)` |
| 3 | `pgrep -a antos-barra` / `pgrep -a waybar` | Una sola instancia de cada; `antos-barra` anclada arriba, `waybar` abajo (T30.6) |
| 4 | La barra ya no muestra «no hay demonio antOS …» | Insignias de git y telemetría con estado real (T31.18) |
| 5 | `Super+Space` | La barra toma foco de teclado (modo *on-demand*) |
| 6 | Escribir una intención + `Enter` | El panel refleja el estado de `antFlow` en vivo (evento `FlowTransition`) |
| 7 | `Super+A` / `Super+Return` / `Super+W` / `Super+Q` / `Super+P` | Panel de agentes / terminal / `antos dev` / cerrar ventana / lanzador `fuzzel` |
| 8 | `grim ~/shot.png` | PNG no vacío del escritorio |
| 9 | `nvim` sobre un proyecto del workspace | `:LspInfo` muestra `antos_lsp` adjuntado; diagnósticos/hover |

---

### Método 6: Núcleo Bare-Metal `no_std` Multi-Arquitectura (x86_64 y AArch64) en QEMU y UEFI

antOS cuenta con un kernel bare-metal `no_std` unificado bajo una Capa de Abstracción de Hardware (HAL) que soporta tanto **x86_64** (BIOS Legacy y UEFI GPT) como **AArch64 / ARM 64-bit** (QEMU `virt`, UEFI EDK2, Apple Silicon y Raspberry Pi).

> ⚠️ **UEFI real (VirtualBox, hardware físico, OVMF/EDK2) exige `--features limine` (T27.1).**
> El binario por defecto (`cargo build`, sin flags) se enlaza en la mitad **baja** del espacio de
> direcciones — funciona con el crate `bootloader` (BIOS, `run.sh`) y con el arranque directo de
> QEMU (`-kernel`, sin bootloader), pero el Limine real embebido en `antos-uefi-*.img` lo rechaza
> con `PANIC: elf: Lower half PHDRs are not allowed`. Compila **con** `--features limine`
> específicamente para generar la imagen que vas a arrancar por UEFI; los pasos 3 y 4 de A, y 3 de
> B, lo indican explícitamente. Ver la sección [E](#e-escritorio-nativo-bare-metal-y-shell-interactivo-soberano-fase-26)
> de este método para el detalle técnico completo.

#### A. Arquitectura x86_64 (BIOS Legacy y UEFI GPT)

```bash
# 1. Compilación y arranque rápido en QEMU (BIOS Legacy vía run.sh) — sin --features limine
./run.sh

# 2. Imagen BIOS (crate `bootloader`, sin --features limine)
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format bios

# 3. Imagen UEFI real (Limine) — requiere recompilar el kernel con --features limine antes
cargo build --features limine
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format uefi

# 4. Ejecución directa en QEMU x86_64 (BIOS):
qemu-system-x86_64 -drive format=raw,file=kernel/target/x86_64-unknown-none/debug/antos-bios.img -serial stdio
```

#### B. Arquitectura AArch64 / ARM 64-bit (Bare Metal y UEFI)

> ⚡ **En Apple Silicon compila en `--release`.** UTM y VirtualBox emulan el
> kernel con TCG (`-cpu cortex-a72`, sin HVF); el binario `debug` puede tardar
> minutos en llegar al banner. La vía corta es
> `system/run-arm.sh --release [--uefi] --build-only`, que deja los artefactos
> en `kernel/target/aarch64-unknown-none/release/`.

```bash
# 1. Instalar target de compilación si no está presente
rustup target add aarch64-unknown-none

# 2. Arranque directo de QEMU virt (Direct Kernel Boot) — sin --features limine
cargo build --target aarch64-unknown-none --manifest-path kernel/Cargo.toml --release
# Valida: consola serie PL011 MMIO, tabla VBAR_EL1 de 16 vectores, MMU (L0/L1/L2),
# heap dinámico, temporizador virtual ARM a 100 Hz, transición a EL0 y syscalls svc #0.
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/release/kernel \
  -serial stdio -monitor none
# GIC: sin DTB/ACPI el kernel asume GICv2; si arrancas con -M virt,gic-version=3
# sondea GICC_IIDR y conmuta a GICv3 solo (no hace falta fijar la versión).
# Dispositivos PCIe (-device qemu-xhci / virtio-gpu-pci) NO funcionan en -kernel:
# sin firmware nadie asigna sus BAR y el kernel los omite (avisa
# "pcie-xhci … BAR0 sin asignar · omitido"). Para GPU/USB usa la ruta UEFI (paso 3).

# 2b. Con teclado y ratón/tablet nativos por VirtIO-Input MMIO (T28.1):
#     añade -device virtio-keyboard-device y -device virtio-tablet-device
#     (transporte MMIO en -M virt; usa un display, p.ej. -display cocoa, para
#     poder teclear en la ventana).
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 512M \
  -kernel kernel/target/aarch64-unknown-none/release/kernel \
  -device virtio-keyboard-device -device virtio-tablet-device \
  -serial mon:stdio -display cocoa

# 3. Imagen UEFI real (Limine) — recompila el kernel con --features limine antes:
cargo build --target aarch64-unknown-none --manifest-path kernel/Cargo.toml --release --features limine
cargo run -p builder -- kernel/target/aarch64-unknown-none/release/kernel --arch aarch64 --format uefi

# 4. Arrancar con firmware UEFI EDK2 en QEMU AArch64 (par pflash: código + variables):
cp /opt/homebrew/share/qemu/edk2-arm-vars.fd /tmp/antos-efivars.fd   # una vez
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -drive if=pflash,format=raw,file=/tmp/antos-efivars.fd \
  -drive format=raw,file=kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img \
  -serial stdio
# Con firmware los BAR quedan programados: añade
#   -device virtio-gpu-pci -device qemu-xhci -device usb-kbd -device usb-tablet -display cocoa
# para escritorio gráfico con entrada USB xHCI. (En Linux/apt el firmware es 'QEMU_EFI.fd'.)
```

#### B-bis. Matriz de periféricos y humo de entrada automatizado (T28.10)

El soporte de periféricos depende mucho del entorno (transporte de teclado/ratón,
GIC v2 vs v3, framebuffer). Los scripts parametrizados generan el comando QEMU
canónico de cada combinación y el modo `--test-input` inyecta teclas y movimiento
de ratón por el **monitor de QEMU**, exigiendo que el kernel los acuse en la
consola serie con `input-rx:` (fallando si no llegan a tiempo).

```bash
# x86_64 — run.sh
./run.sh --kbd ps2 --gpu std                 # i8042 (teclado + ratón PS/2) + VGA
./run.sh --kbd usb --gpu std                 # -device qemu-xhci -device usb-kbd -device usb-tablet
./run.sh --gpu virtio-pci                    # -vga none -device virtio-gpu-pci
./run.sh --gpu ramfb                         # -vga none -device ramfb
./run.sh --test-input --kbd usb              # arranque headless + inyección de entrada

# AArch64 — system/run-arm.sh (QEMU -M virt, arranque directo -kernel)
# Añade --release en Apple Silicon (emulación TCG); --gpu <no-none> abre ventana gráfica.
system/run-arm.sh --release --gic 2 --kbd virtio --gpu virtio-mmio
system/run-arm.sh --release --gic 3 --kbd virtio --gpu ramfb
system/run-arm.sh --release --kbd virtio --gpu ramfb
system/run-arm.sh --test-input --gic 3 --kbd virtio --gpu virtio-mmio
system/run-arm.sh --uefi --release           # imagen UEFI (Limine) — ver aviso abajo
```

> ℹ️ En **arranque directo `-kernel`** (todos los comandos salvo `--uefi`) los
> dispositivos **PCIe** —`--kbd usb` (`qemu-xhci`) y `--gpu virtio-pci`
> (`virtio-gpu-pci`)— se detectan pero se **omiten**: sin firmware nadie asigna
> sus BAR (`pcie-xhci … BAR0 sin asignar · omitido`). Usa `--kbd virtio` con
> `--gpu virtio-mmio` o `--gpu ramfb` en `-kernel` (esta es la vía que funciona
> en UTM).
>
> ⚠️ El arranque **`--uefi` (Limine) no completa** en QEMU 10-11 + EDK2 2024.08:
> Limine carga `KERNEL.ELF` y el kernel no da salida. Bug de *handoff* Limine
> AArch64, en investigación.

| Arch | `--kbd` | Dispositivos QEMU | `--gpu` | Dispositivos QEMU |
| :--- | :--- | :--- | :--- | :--- |
| x86_64  | `ps2` | *(i8042 implícito en `-machine pc`)* | `std` | `-vga std` |
| x86_64  | `usb` | `-device qemu-xhci -device usb-kbd -device usb-tablet` | `virtio-pci` | `-vga none -device virtio-gpu-pci` |
| aarch64 | `virtio` | `-device virtio-keyboard-device -device virtio-tablet-device` | `virtio-mmio` | `-device virtio-gpu-device` |
| aarch64 | `usb` (¹) | `-device qemu-xhci -device usb-kbd -device usb-tablet` | `virtio-pci` (¹) | `-device virtio-gpu-pci` |
| ambas   | —     | — | `ramfb` | `-device ramfb` (x86: `-vga none`) |

(¹) En aarch64 los dispositivos PCIe requieren firmware que asigne los BAR
(sólo VirtualBox ARM64 lo hace hoy); en `-kernel` directo se omiten. El
arranque `--uefi` los programaría, pero actualmente no completa (ver aviso
arriba).

**Resultado esperado de `--test-input`:** el log serie contiene el banner de
arranque (`antOS · kernel …`), las líneas de enumeración del periférico elegido
(`ps2-mouse IRQ12 activo`, `usb-xhci …`, `virtio-input …`, `roothub …`) y, tras
la inyección, `input-rx: primer evento recibido · total=N`. El *job*
`peripheral-smoke` de `.github/workflows/ci.yml` ejecuta la matriz
`aarch64 {virtio, usb} × {virtio-mmio, ramfb}` y `x86_64 {ps2, usb}`.

> ⚠️ **VirtualBox ARM64** (usa `--uefi --release`): arranca a `antos>` con
> **GICv3 por ACPI MADT**, teclado **USB xHCI** y ratón operativos y cursor
> rápido sobre el GOP crudo. Limitación permanente: la IRQ del *timer virtual*
> (PPI 27) no se dispara — el kernel cae al *timer físico EL1* (PPI 30, T28.6) —
> y el *event ring* de xHCI puede perder *Transfer Events* (`ev 0` en `info`,
> T28.3; hay barrido PORTSC de reserva). El detalle y la lista de correcciones
> están en [`guia-emulacion-utm-virtualbox.md`](guia-emulacion-utm-virtualbox.md)
> §7.

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

> ⚡ **Compila en `--release`** para UTM: emula con TCG y el binario `debug` no
> llega ni al banner en 40 s. Genera los artefactos con
> `system/run-arm.sh --release [--uefi] --build-only` (van a `…/release/`).

Pasos detallados y capturas en la
[guía de emulación](guia-emulacion-utm-virtualbox.md) §4.

* **Método A — Kernel directo + `ramfb` (✅ el que funciona en UTM 4.7.x).**
  1. `system/run-arm.sh --release --build-only` → usa el ELF
     `kernel/target/aarch64-unknown-none/release/kernel` (~8 MB; **no** el de
     `--uefi`/`--features limine`, que pesa ~460 KB y no arranca por `-kernel`).
  2. UTM → **+** → **Emular** → **Linux** → marca **«Boot from kernel image»** y
     elige ese `kernel` (UTM lo copia al *bundle* y añade `-kernel` solo).
  3. Configuración: `UEFI Boot` **DESMARCADO**; **Display Card = `ramfb`**;
     en **Argumentos** añade `-device virtio-keyboard-device` y
     `-device virtio-tablet-device`; deja el **Serial** en *Built-in Terminal*.
  4. La ventana gráfica muestra el log (`antOS · ramfb`), el teclado/ratón van
     por VirtIO-Input y `pcie-xhci … BAR0 sin asignar · omitido` es normal.
* **Método B — Imagen de disco UEFI / Limine.** ⛔ **No arranca hoy** en
  QEMU 10-11 + EDK2 2024.08 (UTM 4.7.x): Limine carga `KERNEL.ELF` y el kernel
  queda mudo. Bug de *handoff* Limine AArch64, en investigación — usa el
  Método A.

---

##### 2. Ejecución en VirtualBox

###### A. VirtualBox en macOS Apple Silicon (ARM64 / AArch64):
VirtualBox en Mac M1/M2/M3/M4 **solo permite crear VMs ARM64** y requiere firmware UEFI ARM64 —
por lo que también exige el kernel compilado con `--features limine` (T27.1). Sin ese flag verás
`PANIC: elf: Lower half PHDRs are not allowed` nada más arrancar. **Compila en `--release`**
(VirtualBox ARM64 no acelera el kernel; el `debug` es inutilizable):
```bash
# 1. Compilar (Limine, release) y generar el disco UEFI GPT para ARM64
system/run-arm.sh --uefi --release --build-only

# 2. Convertir la imagen RAW generada a disco virtual VDI nativo
VBoxManage convertfromraw \
  kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img \
  antos-arm64.vdi --format VDI
```
* **Configuración en VirtualBox:**
  - Tipo: `Linux` / `Other (ARM 64-bit)` con 2 CPUs y 1024 MB RAM.
  - Almacenamiento: Añadir `antos-arm64.vdi` como **Disco Duro** SATA/SCSI (no unidad óptica CD/DVD).
  - Puertos Serie: Activar **Puerto 1** (COM1) en modo *"Archivo sin formato"* (ej. `/tmp/antos-serial.log`) para capturar la salida UART PL011.
* **Arranque:** En la UEFI Shell, escribe `map -r` y ejecuta `FS0:\EFI\BOOT\BOOTAA64.EFI` (o `startup.nsh`).
* **Estado:** arranca a `antos>` con GICv3 (ACPI MADT), teclado **USB xHCI** y
  ratón operativos y cursor rápido. Correcciones y limitaciones en la
  [guía de emulación](guia-emulacion-utm-virtualbox.md) §7.

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

#### E. Escritorio Nativo Bare-Metal y Shell Interactivo Soberano (Fase 26)

A partir de la Fase 26 el kernel ya no se limita a diagnósticos por consola serie: trae su propio
driver gráfico, un compositor 2D nativo y un runtime de espacio de usuario (`libantos`) que arranca
como **PID 1** con un shell interactivo real, sin depender de `glibc`/`musl`.

* **T26.1 — Framebuffer y VirtIO-GPU (AArch64):** detección de `simple-framebuffer` vía DTB o de un
  dispositivo `virtio-gpu` MMIO, con doble buffer y escaneo 1024x768x32bpp.
* **T26.2 — Desktop Shell nativo y compositor 2D:** módulo `kernel/src/ui/` (compositor, cursor,
  HUD de intenciones, barra de estado, ventana de terminal) renderizado directamente sobre el
  framebuffer, sin GTK/Wayland.
* **T26.3 — Entrada VirtIO-Input:** teclado y ratón sobre la cola global de eventos tipados
  (`kernel/src/input/`), compartida con el driver PS/2 de x86_64.
* **T26.4 — Cargador ELF64 e initramfs/tarfs:** `kernel/src/elf.rs` carga ejecutables ELF64 de
  usuario desde el VFS (`tarfs` montado sobre el `initrd.tar` embebido en el binario del kernel).
* **T26.5 — `libantos` y el shell interactivo:** biblioteca de runtime de usuario (`user/libantos`)
  con asignador de heap respaldado por `SYS_MMAP`, canales IPC, macros `print!`/`println!` y
  `read_line()`; y `antos-init` (`user/src/main.rs`), el binario que el kernel ejecuta como PID 1.

```bash
# Arrancar x86_64 (BIOS) o AArch64 (Direct Kernel Boot) como en la sección A/B de este método.
# El shell aparece automáticamente en la consola serie al final del arranque:
./run.sh
# — o —
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 512M -serial stdio -display none \
  -kernel kernel/target/aarch64-unknown-none/debug/kernel
```

Una vez en el prompt `antos>` (consola serie), los comandos integrados (*builtins*) del shell son:

| Comando | Descripción |
| :--- | :--- |
| `help` | Lista los comandos disponibles (tabla `BTreeMap` ordenada alfabéticamente). |
| `info` | Arquitectura, memoria del heap del kernel usada y ticks de actividad (`SYS_SYSINFO`). |
| `ls [ruta]` | Lista ficheros y directorios del VFS/initramfs (`SYS_FS_LIST`); por defecto `/`. |
| `cat <ruta>` | Muestra el contenido de un fichero de texto del VFS (`SYS_FS_READFILE`). |
| `desktop` | Renderiza el Desktop Shell (T26.2). En x86_64 lo hace bajo demanda y sin interrumpir el shell (corren en paralelo gracias al planificador preemptivo, T23.2); en AArch64 cede permanentemente el control al bucle gráfico reactivo del kernel, ya que esa arquitectura aún no tiene planificador preemptivo. |
| `agent <intención>` | Crea un canal IPC del kernel (`SYS_CHANNEL_CREATE`/`SEND`) y encola la intención con el prefijo `antflow.intent:` para un futuro puente con el `antFlow` del host — todavía no hay conexión real entre el kernel bare-metal y `antosd`. |

> ⚠️ **Nota sobre el teclado:** en modo headless (`-display none`), QEMU no entrega eventos PS/2 ni
> VirtIO-Input, así que el prompt queda esperando indefinidamente — comportamiento esperado. Para
> escribir comandos de verdad usa una VM con ventana gráfica (quita `-display none`) o UTM/VirtualBox
> con la VM en primer plano.

**Tabla completa de syscalls disponibles para programas de usuario** (`kernel/src/syscall/mod.rs`,
compartida por `user/libantos/src/syscall.rs`):

| # | Syscall | Arg1 | Arg2 | Arg3 | Arg4 |
| :-: | :--- | :--- | :--- | :--- | :--- |
| 1 | `SYS_EXIT` | código de salida | | | |
| 2 | `SYS_WRITE` | ptr | len | | |
| 3 | `SYS_READ` | ptr destino | max len | | |
| 4 | `SYS_YIELD` | | | | |
| 5 | `SYS_GETPID` | | | | |
| 6 | `SYS_MMAP` | tamaño | | | |
| 7 | `SYS_MUNMAP` | ptr | tamaño | | |
| 8 | `SYS_SPAWN` | ptr ruta | len ruta | modo | |
| 9 | `SYS_WAITPID` | pid | | | |
| 10 | `SYS_CHANNEL_CREATE` | | | | |
| 11 | `SYS_CHANNEL_SEND` | canal | ptr | len | |
| 12 | `SYS_CHANNEL_RECV` | canal | ptr destino | max len | |
| 13 | `SYS_FS_LIST` | ptr ruta | len ruta | ptr salida | len salida |
| 14 | `SYS_FS_READFILE` | ptr ruta | len ruta | ptr salida | len salida |
| 15 | `SYS_SYSINFO` | ptr salida | len salida | | |
| 16 | `SYS_LAUNCH_DESKTOP` | | | | |

---

#### F. Soporte Real del Protocolo de Arranque Limine (Fase 27)

La imagen UEFI (`antos-uefi-x86_64.img` / `antos-uefi-aarch64.img`) embebe el bootloader Limine
real (T24.1) desde el principio, pero hasta T27.1 el kernel nunca hablaba su protocolo de
arranque — solo el crate `bootloader` (BIOS) y el arranque directo de QEMU. Limine exige un
ejecutable enlazado en la mitad alta del espacio de direcciones; el binario por defecto no lo
está, y Limine lo rechazaba con `PANIC: elf: Lower half PHDRs are not allowed` (el mismo pánico
que ves al intentar arrancar en VirtualBox, que solo soporta UEFI).

```bash
# Compilar el kernel hablando el protocolo Limine (mitad alta + boot requests)
cargo build --features limine                                          # x86_64
cargo build --target aarch64-unknown-none --features limine            # AArch64

# Generar la imagen UEFI a partir de ese binario (igual que en A/B, pero con el kernel correcto)
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format uefi
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64 --format uefi
```

Sin `--features limine`, `antos-uefi-*.img` sigue conteniendo un kernel no apto para Limine y el
arranque por UEFI (VirtualBox, hardware físico, OVMF/EDK2) fallará con el mismo pánico — este
flag **no** es necesario para `run.sh` (BIOS) ni para el arranque directo de QEMU AArch64
(`-kernel`), que siguen funcionando exactamente igual que siempre.

---

#### G. Periféricos Nativos y Endurecimiento Runtime (Fase 28 y depuración en hipervisores)

La Fase 28 (T28.1–T28.10) llevó los periféricos bare-metal a paridad entre
arquitecturas, y una iteración posterior de depuración sobre **VirtualBox
ARM64** y **UTM** endureció el arranque en emuladores reales.

**Fase 28 — periféricos y firmware:**

* **T28.1–T28.2** — VirtIO-Input MMIO robusto; mapeo PCIe ECAM/MMIO
  multiplataforma con escaneo de puentes.
* **T28.3–T28.4** — xHCI con *rings*/buffers por endpoint, *control transfers*
  extendidas, *hotplug* y hubs; parser de HID Report Descriptor y decodificador
  genérico dirigido por *usages*.
* **T28.5** — ergonomía de entrada: LEDs de teclado, auto-repeat, layouts
  US/ES, aceleración de puntero.
* **T28.6** — timer AArch64 resiliente (fallback a físico EL1) y GIC v2/v3 con
  enrutado de IRQ de periféricos.
* **T28.7** — paridad de display: `virtio-gpu-pci`, `ramfb` y cadena de
  *fallback* de framebuffer (GOP → DTB → virtio-gpu MMIO → virtio-gpu-pci →
  ramfb).
* **T28.8** — descubrimiento por firmware (DTB/ACPI) sin direcciones
  *hardcodeadas*.
* **T28.9** — periféricos x86_64: ratón PS/2 y pila USB xHCI.
* **T28.10** — banco de pruebas de periféricos: `run.sh` / `system/run-arm.sh`
  parametrizados (`--kbd`/`--gpu`/`--gic`), humo de inyección de entrada
  (`--test-input`, `qemu-smoke.py`) y *job* `peripheral-smoke` en CI.

**Depuración runtime sobre VirtualBox ARM64 / UTM (posterior a T28.10):**

* **GICv3 por ACPI MADT.** VirtualBox ARM64 no expone un DTB alcanzable; el
  kernel parsea ahora `RSDP → XSDT → MADT` y configura GICv3 con las bases
  reales (`d=0xfcd3_0000`, `r=0xfcd4_0000`) antes de `gic::init()`. Antes se
  quedaba en GICv2 y colgaba en `verificando fuente de temporizador…`.
* **GICv3 sin firmware.** En `qemu -M virt,gic-version=3 -kernel` (sin DTB/ACPI)
  el kernel sondea `GICC_IIDR` con recuperación de fallos y conmuta a GICv3 en
  vez de hacer *panic* con un Data Abort sobre el bloque GICC inexistente.
* **BAR PCIe sin asignar.** En arranque directo `-kernel` no hay asignador de
  recursos PCI: los BAR de `qemu-xhci` / `virtio-gpu-pci` quedan a cero.
  `decode_bar()` los trata como `PciBar::None` y el kernel omite el controlador
  con un aviso (`pcie-xhci … BAR0 sin asignar · omitido`) en lugar de
  desreferenciar un puntero nulo.
* **Compositor sobre GOP crudo.** El cursor ya no deja «descuadre» ni va lento
  en VirtualBox: `present_best` recompone por bandas de daño y `present_rows`
  vuelca líneas de barrido completas (el *scanout* GOP Non-Cacheable no
  reflejaba escrituras parciales estrechas).
* **Teclado USB.** *Configure Endpoint* ya no pone a cero el *Root Hub Port*
  del Slot Context; se alimentan (`PP`) todos los puertos raíz antes de
  enumerar; el rol HID se clasifica por el *usage* de la colección
  `Application` (un teclado se detectaba como ratón).
* **Diagnósticos.** El *spam* de `input-rx:` se reduce a primer evento + una
  línea cada 100; corregido un *deadlock* al re-tomar el lock de `CONSOLE`
  dentro del log `fb-geom`.
* **Tooling.** `system/run-arm.sh` abre ventana gráfica con `--gpu <≠none>` y
  respeta `--release` (obligatorio en Apple Silicon por emulación TCG).

> Guía práctica paso a paso (UTM y VirtualBox, con resolución de problemas):
> [`guia-emulacion-utm-virtualbox.md`](guia-emulacion-utm-virtualbox.md).

---

### Método 7: Live USB Booteable e Instalación en Hardware Real (Bare Metal)

Este método permite arrancar un medio de instalación extraíble en cualquier computadora física (PC, laptop o servidor) y desplegar el sistema operativo antOS directamente en discos físicos NVMe o SATA:

```bash
# 1. Construir la imagen híbrida autoarrancable (UEFI + BIOS MBR) y generar suma SHA-256
antos usb build --arch x86_64 --out antos-live.iso

# 2. Listar unidades USB extraíbles detectadas de forma segura (bloquea discos internos)
antos usb list

# 3. Grabar la imagen en la memoria USB con progreso en tiempo real y verificación de integridad
antos usb flash --image antos-live.iso --target /dev/sdb --apply
```

#### Flujo de Instalación en la Máquina Física:
1. Conecta el pendrive USB en el equipo destino y enciéndelo presionando la tecla de arranque (`F12`, `F11`, `F8` o `F9`).
2. Selecciona la entrada UEFI correspondiente a la memoria USB.
3. El gestor Limine cargará el kernel `no_std`, montará el initramfs en RAM y desplegará la sesión Live.
4. Abre una terminal e inicia el asistente de instalación interactivo:
   ```bash
   antos install
   ```
5. Selecciona el disco de destino (`/dev/nvme0n1` o `/dev/sda`), elige entre instalación limpia en disco completo (escribiendo `SI`) o Dual-Boot seguro, y completa la configuración.

> **Estado real de `antos install` (T36.1 / T36.2):** la simulación y la
> instalación real recorren **la misma tubería**; lo único que cambia es
> quién ejecuta los comandos. Sin `--apply`, cada paso sale marcado
> `○ simulado` con el **comando literal** que la instalación real lanzaría
> (`parted`, `mkfs.vfat`/`mkfs.ext4`, `blkid`, `mount`,
> `nixos-generate-config`, `nixos-install --flake … --override-input antos …`,
> `umount -R`), y la configuración de antOS Linux queda en
> `<workspace>/target/installer-staging/etc/nixos/` (`flake.nix` con nixpkgs
> fijado y `nix.registry` para que `nixos-rebuild` funcione sin red,
> `flake.lock`, `configuration.nix`, `hardware-configuration.nix` de
> relleno, copia del árbol de antOS en `antos/`); el disco no se toca. Con
> `--apply` (desde la ISO en vivo, como root) se comprueban las
> precondiciones y se ejecuta de verdad: en **Disco Completo** tabla GPT
> nueva (ESP 512 MiB `ANTOS_ESP` + raíz `antos-root`), en **Dual-Boot** se
> reutiliza la ESP existente sin formatearla y la raíz va al mayor hueco
> libre (≥ 20 GiB; si no lo hay, dice cuánto falta — antOS no redimensiona
> particiones ajenas). Cualquier fallo aborta con el error del comando y
> deja el disco desmontado. Ningún informe con `simulated = true` se anuncia
> como instalación completada. Hasta T36.1 el modo real escribía en un
> directorio sin montar, registraba en la NVRAM un stub EFI que no arranca y
> terminaba con «retira el USB y reinicia».
>
> **Verificación end-to-end:** `system/nixos/install-smoke.sh <iso> clean|dual`
> (QEMU/OVMF: instala desde la ISO, reinicia desde el disco y comprueba
> `greetd`, `antos-barra`, `antos ping` y `nixos-rebuild build` sin red;
> workflow `install-smoke.yml`). Necesita la ISO autosuficiente de
> **T36.3**: hasta entonces el `nixos-install` del live no tiene la closure
> ni la fuente de antOS.
>
> **Dos destinos de `antos install`:**
> - Desde la **Live del kernel bare-metal** (arriba): la disposición de la
>   ESP (`antos bootloader install --bare-metal --efi-binary <antos.efi>`)
>   necesita el binario EFI real; nunca se fabrica un stub fuera de la
>   simulación.
> - Desde la **ISO gráfica de antOS Linux** (`nix build .#iso`, T30.2): el
>   asistente genera `/etc/nixos/{flake.nix,flake.lock,configuration.nix}`
>   con `services.antos.desktop.enable = true` (autologin, hostname,
>   timezone, **keymap**, `system` detectado, `password_hash` opcional →
>   `initialHashedPassword`) + `systemd-boot` —que en Dual-Boot encadena los
>   demás SO sin tocar sus entradas—, y con `--apply` ejecuta
>   `nixos-install`. El `flake.nix` consume
>   `antos.nixosModules.{default,desktop,llm}` y `antos.overlays.default`
>   (nunca rutas internas del árbol) y la CI comprueba que evalúa. Tras
>   `reboot` arranca al escritorio antOS y se evoluciona con
>   `sudo nixos-rebuild switch --flake /etc/nixos#<hostname>`. Detalle en
>   [`docs/guia-live-usb-e-instalacion-fisica.md`](guia-live-usb-e-instalacion-fisica.md) §4-bis.
>
> **No interactivo** (`antos install --config install.toml --apply`): las
> claves son `target_device`, `clean_install`, `confirm_wipe` (obligatorio
> `true` con `clean_install` real: es el «SI» del asistente), `hostname`,
> `username`, `timezone`, `keymap`, `system` y `password_hash`
> (`mkpasswd -m yescrypt`). `antos ping` comprueba desde cualquier shell que
> el demonio responde por el socket (`QueryGitStatus`, lo mismo que hace la
> barra al arrancar).

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

#### Proyectos nuevos por tecnología (`project.scaffold`, T35.1)

La tecnología, el lenguaje y el framework de un proyecto nuevo son **datos**:
el catálogo de stacks de [`system/stacks/`](../system/stacks/README.md).
Basta nombrar la tecnología en cualquier posición; el nombre va tras
«llamado» / «llamada» / «nombre» (sin nombre, antOS pregunta, no inventa):

```bash
antos "crea un proyecto en nestjs llamado antostest"     # typescript/nestjs
antos "nueva api fastapi llamada catalogo"               # python/fastapi
antos "crea un servicio axum llamado core"               # rust/axum
antos "crea una api con express llamada shop"            # javascript/express
antos "crea una app next llamada web"                    # typescript/nextjs
antos "crea un proyecto en go llamado svc"               # go
antos "crea un proyecto llamado demo"                    # rust base, como siempre
```

Cada stack deja el proyecto **listo para instalar**: sus ficheros, un test
que pasa tras `install`, `.gitignore`, repositorio Git inicializado
(T17.3) y `.antos/project.toml` con el stack, los comandos (`install`,
`test`, `dev`, `build`), el puerto y el toolchain — la fuente de verdad que
`test.run`, `ci` y los agentes leerán (T35.3). Stacks incluidos: `rust`,
`axum`, `typescript`, `nestjs`, `nextjs`, `javascript/express`, `python`,
`fastapi`, `go`. Añadir uno es escribir un TOML (el cargador rechaza alias
repetidos, rutas con `..`, comandos vacíos o fuera de la lista blanca de
programas), y dentro del árbol de antOS o con `ANTOS_STACKS=<dir>` se ve sin
recompilar.

**Instalar, probar y compilar sin shell (`project.run`, T35.2).** Crear un
proyecto propone **dos pasos** en una sola confirmación: `project.scaffold`
y `project.run command=install`; después, los verbos sobre el proyecto:

```bash
antos "crea un proyecto en nestjs llamado antostest"     # andamio + npm install
antos "crea un proyecto en nestjs llamado api sin instalar"
antos "instala las dependencias del proyecto antostest"
antos "prueba el proyecto antostest"                     # npm test → TESTS EN VERDE/ROJO
antos "compila el proyecto antostest"
```

Cada comando sale del `.antos/project.toml` del proyecto (es decir, del
stack), **nunca** del texto que escribiste: el programa tiene que estar en la
lista blanca (`npm`, `npx`, `pnpm`, `yarn`, `node`, `cargo`, `python3`,
`uv`, `pip`, `pytest`, `go`, `dotnet`, `make`) y ningún argumento puede
llevar metacaracteres de shell — se comprueba al cargar el catálogo y otra
vez al ejecutar, porque el manifiesto es un fichero editable. Corre dentro
del recinto con cuota de 15 min, `cwd` en el proyecto y un entorno mínimo
sin credenciales en el que **todo lo que un gestor escribe fuera del
proyecto va dentro de él**: `.antos/cache/{npm,cargo,uv,go,…}`, `HOME` en
`.antos/home`, temporales en `.antos/tmp`, log completo en
`.antos/logs/<comando>.log` (todo ello en el `.gitignore`). `node_modules/`,
`target/`, `.venv/` y las cachés son artefactos declarados: no se confirman
como escrituras ni se fotografían; `antos undo` tras crear el proyecto lo
elimina entero.

Dos cosas que hay que saber:

- **La red del recinto es todo o nada.** `project.run` la abre para el paso
  y la previsualización dice a dónde va según el stack (`registry.npmjs.org`,
  `crates.io`, `pypi.org`…), pero no filtra por dominio: con la red abierta,
  el comando —y los scripts del proyecto que ejecute— pueden hablar con
  cualquier host. Un filtro por dominio es otro ticket.
- **Toolchain.** Se usa el binario de la máquina (`PATH`, Homebrew,
  `~/.cargo/bin`, `~/.local/bin`, nvm); si no está pero hay `nix`,
  `nix shell nixpkgs#<toolchain del stack> -c <programa>` — así la imagen de
  antOS Linux no necesita traer `node`/`cargo`/`uv`. El andamio deja además
  `flake.nix`, `devbox.json` y `.antos/env.toml` con ese toolchain. Quien
  prefiera toolchains globales en la imagen:
  `services.antos.desktop.toolchains = [ "node" "python" "rust" "go" ]`.
  Rust por `rustup` bajo Landlock no funciona (`~/.rustup` no es legible desde
  el recinto, límite conocido de T33.2); con `cargo` de nixpkgs sí.

**Verificación del andamio y fuente de verdad del stack (T35.3).** Crear un
proyecto son **tres pasos** en una confirmación: `project.scaffold` →
`project.run install` → `project.run verify` (el test del stack; en rojo el
paso falla y el proyecto se deshace solo). `.antos/project.toml` es la
fuente de verdad: `test.run`, `antos ci` y `antos env` lo leen antes de
adivinar por ficheros, y los proyectos que no creó antOS se adoptan:

```bash
antos project info antostest        # stack, comandos, puerto, toolchain (y de dónde sale)
antos project adopt legacy          # deduce el stack por los ficheros y escribe .antos/project.toml
antos project stacks                # el catálogo y de dónde se cargó
antos "prueba el proyecto legacy"   # ya usa el comando de test del manifiesto
```

**El agente crea proyectos.** `project.scaffold` y `project.run` están en el
toolset del agente, también en el compacto de los modelos pequeños (medido
con el 7B, ver abajo):

```bash
antos -y agent do "crea un proyecto rust llamado demo y haz que cargo test pase con un test de la función sum"
```

Caso de evaluación `scaffold-and-extend` (`evals/agent/`): determinista con
`fake` en CI; con `--live` y `qwen2.5-coder:7b`, 10/10 (2026-09-19, dos
tandas de `--repeat 5`, mediana 6 pasos, 10 s) tras dos ajustes del runtime
que la medida destapó: el corte de bucle cuenta la misma llamada fallida
aunque haya lecturas en medio (patrón real: `fs.read` → `fs.patch` ✗ →
`fs.read` → el mismo `fs.patch` ✗…), y antes de cortar se pide el cierre
tipado (`finalizar` por `format`), porque a menudo el trabajo ya está hecho
—tests en verde— y el modelo se atasca en un parche que ya no aplica. Sin
regresión en el caso Rust (5/5).

`dev` (el servidor en marcha) no se ejecuta con `project.run` a propósito:
un proceso que no termina no cabe en un paso con cuota; será un servicio
(`env.service_up` para proyectos) en la fase siguiente.

Verificado el 2026-09-19 en el Mac, **por el recinto Seatbelt**: NestJS
(566 paquetes, 24 s) 2/2, Express 2/2, FastAPI (`uv sync`) 2/2, Axum
(`cargo fetch`, índice del registro dentro del proyecto) 2/2; `~/.npm`
intacto; `antos undo` elimina el proyecto. Next.js necesita Node ≥ 22.6.
Go sin verificar (sin toolchain). El camino `nix shell` solo se verificó por
construcción del comando: la máquina de desarrollo no tiene `nix`.

---

### 4.2 Orquestación Multi-Agente (`antos agent`)

> **Estado honesto (T33.1–T33.3).** `antos agent run <ticket> --auto` ejecuta
> el pipeline **real** (T33.3): Arquitecto → Coder → QA → Auditor, cada rol
> como un run de agente con el modelo de `antos agent config`. `--simulated`
> conserva el pipeline de demostración de T3.1 (sin modelo), y el CLI, la
> barra y el IPC lo marcan como «SIMULACIÓN». Lo que sigue sin hacer ni el
> pipeline real: fusionar la rama del agente al aprobar (la rama y el
> worktree se conservan para inspección). El despacho desde el tablero de la
> barra (`StartFlow`) sigue arrancando la tarea simulada hasta T33.4.

#### Pipeline de roles (`antos agent run --auto`, T33.3)

- **Arquitecto** (solo lectura sobre el workspace): recibe el ticket y
  entrega un `ImplementationPlan` tipado — `files_to_touch` (las únicas
  rutas que el Coder podrá modificar), `steps`, `acceptance_checks`. Sin plan
  válido, o con `files_to_touch` vacío («no realizable»), la tarea falla.
- **Coder** (edita **en el worktree** `agent/<ticket>`): `fs.patch`,
  `fs.write`, `test.run`… con el plan como objetivo.
- **QA** (sin modelo): ejecuta la suite real (`cargo test`/`npm test`/
  `pytest`); si está en rojo, el extracto del fallo vuelve al Coder como
  nuevo objetivo, hasta `max_qa_retries` (3).
- **Auditor** (solo lectura): recibe el diff consolidado, el plan, los
  criterios y el informe de QA, y emite un `AuditVerdict` (`approve`,
  `findings`, `risk`). Un diff que toque ficheros fuera de `files_to_touch`
  se rechaza **aunque el modelo apruebe**; con hallazgos accionables vuelve
  al Coder, sin ellos la tarea falla.
- Aprobado ⇒ `ReadyForApproval` con `diff_preview` y `audit_summary`; la
  aprobación humana (`antos agent approve` / notificación) no cambia.

```bash
antos agent config --role coder --llm claude          # proveedor[:modelo] por rol
antos agent run T33.4 --auto                          # pipeline real
antos -y agent run T33.4 --auto                       # aprueba los pasos confirm del Coder
antos agent run T33.4 --auto --simulated              # demo sin modelo (T3.1)
antos agent status T33.4                              # fase, modelo real, pasos y tokens por transición
```

**Desde la barra (T33.4).** En la barra de intención, `agente: <objetivo>`
lanza un run con herramientas y `ticket: T1.2` (o «desarrolla ticket T1.2»,
o el botón «Despachar» del tablero) el pipeline de roles, por la misma sesión
que una intención: los pasos aparecen en vivo con un botón «■ Detener»
(`Request::AgentStop`, corta antes de la siguiente herramienta), cada paso
`confirm` se aprueba inline con su diff, y el informe final ofrece «↶
Deshacer el run» (restaura la instantánea del run como una intención
normal). Las tarjetas del tablero muestran fase, si la tarea es simulación
o agente, y modelo/pasos/tokens del último run. Sin proveedor configurado,
«Despachar» arranca la tarea simulada y lo dice. Bajo Plasma, **Super+Space**
abre o cierra la barra (atajo global registrado por `.desktop`; activa el
icono SNI, lo mismo que un clic en la bandeja).

**Evaluar antes de cambiar prompts o modelos (T33.5).** Los casos de
`evals/agent/*.toml` (fixture + objetivo + guion `fake` + lo esperado) son
el contrato del runtime y corren en `cargo test` y en CI:

```bash
antos eval agent                       # determinista: proveedor fake, sin red
antos eval agent --case rust-fix-failing-test
antos eval agent --live [--provider claude]   # los mismos casos con el modelo real (cuesta dinero)
antos eval diff                        # última ejecución vs anterior: regresiones
```

Cada ejecución queda en `.antos/evals/<fecha>.json` con pasos, tokens,
segundos, motivo de parada, tests en verde y ficheros escritos por caso.
`eval diff` señala regresión si un caso pasaba y falla, sus tests pasaban y
están en rojo, ya no termina con `finished`, o sube un 30 % en pasos o
tokens. Flujo recomendado: `antos eval agent --live` → cambiar el prompt o
el modelo del rol → `antos eval agent --live` → `antos eval diff`.

**Modelos locales con Ollama (gratis).** Desde T34.2 una instalación sin
ninguna clave llega a un agente en tres comandos; `auto` (el proveedor por
defecto) elige el Ollama local si responde y tiene un modelo con `tools`,
y nunca llama a un proveedor de red que no hayas configurado:

```bash
antos llm setup            # arranca/adopta Ollama, diagnostica, descarga el modelo recomendado por RAM, lo fija
antos -y agent do "haz que pase el test sums de src/lib.rs"
antos -y agent run T99.1 --auto
antos eval agent --live    # mide antes de tocar prompts o modelos
```

`setup` pregunta en cada paso (`-y` responde que sí). Por piezas:
`antos llm doctor` (RAM, tier, modelos con tamaño/cuantización/`tools`/
contexto máximo, contexto efectivo), `antos llm pull|rm <modelo>` (por la
API HTTP de Ollama, sin necesitar el binario), `antos llm ctx <tokens>
[--role coder]`, `antos llm temperature <t>`.

**Contexto.** Ollama usa **4096 tokens** si no se le pide otra cosa y, al
superarlos, recorta la conversación **por el principio**: el prompt de
sistema y las herramientas. Un `fs.read` de dos ficheros medianos basta. El
proveedor de agente pide siempre `num_ctx` (16 384 por defecto; por rol con
`antos llm ctx 8192 --role architect`), acotado al máximo del modelo
(`/api/show`) y al techo del tier de RAM (`system/llm/models.toml`: 8 GB →
8k, 16 GB → 16k, 32 GB+ → 32k). Si se acota, el informe del run lo dice
(`contexto: 8192 tokens (contexto pedido 16384, efectivo 8192: …)`). También
envía `keep_alive` (10 min) para que el modelo no se descargue entre pasos.

**Temperatura.** El planificador va a 0.0; el agente **no**: con
`qwen2.5-coder:7b` a 0.0 el 7B entra en un bucle determinista
(`fs.list`/`fs.read` hasta agotar el presupuesto). Medido en el Mac de 18 GB,
escenario «arregla un test en rojo», 5 ejecuciones por valor (2026-09-18,
con 16k de contexto): 0.0 → 0/5 · 0.3 → 3/5 · 0.7 → 2/5. El valor por
defecto es 0.3.

**Perfiles de roles (T34.4).** `antos llm profile local | hybrid | cloud`
escribe los modelos de los cuatro roles de antFlow de un golpe, con los
modelos del tier de RAM de la máquina (`system/llm/models.toml`):

| Perfil | Arquitecto | Coder | QA | Auditor | Requiere |
|---|---|---|---|---|---|
| `local` | Ollama, modelo «grande» del tier | Ollama, «código» | Ollama, «rápido» | Ollama, «grande» | solo Ollama |
| `hybrid` | nube gratuita con clave (OpenRouter / Groq / Gemini / Claude) | Ollama | Ollama | nube | Ollama + una clave; sin clave avisa y aplica `local` |
| `cloud` | OpenRouter | Ollama | Groq | Groq | claves (el reparto anterior a T34.4) |

Una instalación limpia **es `local` sin pedirlo**: con `active_provider =
auto` y sin `role_models`, cada rol resuelve a `ollama` y el runtime elige
el modelo descargado con `tools` (el configurado, si no el recomendado por
RAM). `antos llm profile` sin argumento muestra el reparto vigente.

**Lo que el runtime hace por un modelo pequeño (T34.4).** Cuando el
proveedor declara ≤ 9B (por el nombre `:7b` o por `parameter_size` de
`/api/show`):

- **Toolset compacto**: Coder = `fs.read`, `fs.list`, `fs.patch`,
  `test.run`; Arquitecto = `fs.read`, `fs.list`; Auditor = `fs.read`;
  `agent do` = `fs.read`, `fs.list`, `fs.patch`, `test.run`. Sin `fs.write`
  (lo confunde con `fs.patch` y pisa ficheros enteros). Se anota al empezar
  el run. Un modelo grande conserva el toolset completo.
- **Cierre estructurado**: si tras el recordatorio sigue respondiendo en
  prosa, se le pide el JSON de `finalizar` con `format` (Ollama) o
  `response_format` (OpenAI-compat) y el run termina `Finished` con el
  cierre tipado, no `ModelStopped`.
- **Reintento gratuito**: la primera llamada con argumentos malformados de
  cada herramienta devuelve el esquema de entrada y no consume paso; los
  rechazos de política (fuera del toolset, fuera del workspace) no tienen
  reintento.
- **Corte de bucle**: la misma llamada (herramienta + argumentos) fallando
  tres veces seguidas termina el run como `Looping`; a la segunda el modelo
  recibe «no la repitas: lee el fichero y usa el texto exacto».

Los prompts (genérico, Coder, Arquitecto, Auditor) se reescribieron para un
7B: la regla más violada en las medidas —«nunca pidas el fichero al usuario:
léelo con fs.read»— va la primera, con un ejemplo de llamada.

**Medido (2026-09-18, Mac de 18 GB, `qwen2.5-coder:7b`, contexto 16k,
temperatura 0.3, 5 ejecuciones por fila):**

| Cambio | Rust «arregla el test» | Node «haz que pase test.js» |
|---|---|---|
| T34.2 (contexto + temperatura) | 3/5 | — |
| + toolset compacto, reintento, cierre estructurado, corte de bucle | 2/5 | — |
| + prompts para 7B | **5/5** (y 5/5 al repetir: 10/10) | 0/5 |

El caso Node sigue en 0/5 por un motivo concreto: el objetivo nombra el
fichero de test y el 7B empieza editándolo (añade un mensaje al `assert`,
lo quita…) antes de mirar `sum.js`; con presupuesto de 10 pasos llega a
corregir `sum.js` en el paso 9 y no le queda sitio para `test.run` y
`finalizar`, o «termina» antes con la suite en rojo. Es un fallo de
seguimiento de instrucción («nunca modifiques un test»), no del runtime, y
queda anotado como la siguiente medida a atacar.

**Medir con repeticiones.** `antos eval agent --live --repeat 5` ejecuta
cada caso cinco veces y guarda tasa de éxito y medianas de pasos, tokens y
segundos (`✓ rust-fix-failing-test 5 pasos · 5171 tokens · 7 s · 5/5 ok`).
`antos eval diff [--margin 0.2]` compara tasas y solo señala regresión si la
caída supera el margen: 5/5 → 4/5 es ruido; 5/5 → 2/5 no. El job
[`agents-nightly.yml`](../.github/workflows/agents-nightly.yml) hace esto
cada noche contra Ollama en un runner (o a mano con *Run workflow*) y sube
el JSON como artefacto: la serie histórica contra la que se mira antes de
tocar un prompt.

Un modelo que declara no soportar `tools` se rechaza al resolverlo para un
agente (el error sugiere uno descargado que sí). Para más fiabilidad, un
modelo mayor (`qwen2.5-coder:14b` cabe en 32 GB con 16k de contexto) o un
proveedor remoto para el rol Coder; la evaluación `--live --repeat` es la
forma de comparar sin adivinar.

Presupuesto en pasos por rol: 8 / 30 / 8 (Arquitecto / Coder / Auditor),
configurable en `llm_config.json` (`role_steps`). **Autopilot** (T16.3) ya no
inventa correcciones: al aprobar un incidente lanza un run de Coder con el
error como objetivo (si hay proveedor; si no, el incidente queda en `manual`).

#### Agente con herramientas (`antos agent do`, T33.2)

Un modelo trabaja varios turnos sobre el espacio de trabajo con **las
capacidades del catálogo como herramientas**: `fs.read`, `fs.list`,
`fs.patch` (sustitución exacta de un bloque único), `fs.write`, `test.run`
(`cargo test` / `npm test` / `pytest` dentro del sandbox), `git.status` y
`memory.search`, más la terminal `finalizar`. Cada llamada pasa por lo mismo
que un paso de intención —validación del catálogo, radio de impacto, puerta
de confirmación (`confirm` pregunta; `-y` aprueba), instantánea, ejecución
confinada, journal— y el resultado vuelve al modelo. No existe ninguna
herramienta que ejecute comandos arbitrarios. Un solo registro de journal e
instantánea por run: `antos undo` deshace el run entero.

```bash
# Con el proveedor activo (antos llm use claude|ollama|openrouter|…)
antos agent do "haz que pase el test sums de src/lib.rs"

# Proveedor explícito (o proveedor:modelo), presupuesto en pasos, toolset acotado
antos agent do "añade un test para parse_port" --provider claude
antos agent do "corrige el warning de clippy en git.rs" -p ollama:qwen2.5-coder:latest --budget 12
antos agent do "explícame qué hace exec/mod.rs" --tools fs.read,fs.list,memory.search

# Ver qué haría sin escribir nada; aprobar todo sin preguntar
antos -n agent do "renombra la función"      # dry-run
antos -y agent do "arregla el build"         # sin preguntar en los pasos confirm

# Runs registrados (id, pasos, instantánea)
antos agent report
antos undo                                   # deshace el último run

# Proveedor determinista para tests y demos: un guion JSON de turnos
ANTOS_AGENT_FAKE_SCRIPT=guion.json antos agent do "…" --provider fake
```

Presupuesto por defecto: 20 pasos, 400 000 tokens, 10 minutos. Los
artefactos de construcción (`target/`, `Cargo.lock`) están declarados como
`scratch` en `test.run`: el recinto los deja escribir, pero ni se confirman
ni se fotografían. Limitación conocida: bajo Landlock (Linux) el recinto
solo lee `/usr`, `/lib`, `/etc`…; en antOS Linux el toolchain vive en
`/nix/store` y `~/.cargo`, así que `test.run` necesita ampliar esas raíces
de solo lectura (seguimiento en T33.2).

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

Motor de análisis estático, vocabulario global compacto (`TermDictionary`), vectores dispersos `Vec<(u32, f32)>` y grafo bidireccional de relaciones en tiempo amortizado $O(1)$:

```bash
# Consultar el estado de la memoria semántica, total de fragmentos y aristas
antos memory
antos memory status

# Indexar o reindexar el espacio de trabajo (.antos/memory.json)
antos memory index
antos memory reindex

# Búsqueda semántica por similitud coseno dispersa (rápida, zero-allocation)
antos memory search "orquestador de agentes y roles"
antos memory search "bóveda de secretos y permisos" --limit 10
antos memory find "liberar puerto listener" -n 3

# Inspeccionar el total de nodos y aristas del grafo de contexto del proyecto
antos memory graph

# Visualizar el grafo de dependencias de un símbolo, ticket o archivo específico en O(grado)
antos memory graph file:system/antosd/src/main.rs
antos memory graph ticket:T6.2
antos memory graph symbol:system/antosd/src/memory.rs:compute_sparse_vector
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

`antos service up <svc>` arranca un **proceso real** que escucha en
`127.0.0.1`, registra PID, log y salud en `$STATE/services/<svc>/` e inyecta
la variable de conexión en el `.env` del workspace — solo cuando la sonda de
salud ha pasado. Servicios conocidos: `ollama`, `postgres`, `redis`,
`meilisearch` (con plan de arranque) y `mariadb`, `rabbitmq` (solo adopción:
si ya escuchan, antOS los registra; arrancarlos es un ticket aparte).

De dónde sale el proceso lo dice la columna **ORIGEN** (T34.1):

| Origen | Qué pasó | `service down` |
| :--- | :--- | :--- |
| `external` | El puerto ya respondía (Ollama.app, `services.ollama` de NixOS, un postgres del sistema). antOS lo **adopta**. | Retira el registro; **no** mata el proceso. |
| `system` | Binario encontrado en `PATH` o en los directorios habituales (`/usr/lib/postgresql/*/bin`, Homebrew, Postgres.app…). | `SIGTERM` al proceso; `SIGKILL` si no atiende en 10 s. |
| `nix` | Sin binario pero con `nix`: `nix shell nixpkgs#<pkg> -c …`. | Igual que `system`. |

Si no aplica ninguno, `service up` falla nombrando los tres caminos y **no
escribe nada** (ni registro ni `.env`). La columna **SALUD** es el resultado
de la sonda (`sí` / `no` / `?` cuando no se pudo sondear, p. ej. desde un
recinto sin red); **ESTADO** combina PID y sonda: `running`, `external`,
`unhealthy` (vivo pero no responde) o `stopped`. Antes de señalar un PID se
comprueba que sigue siendo el proceso arrancado (un PID reutilizado por otro
programa cuenta como `stopped` y no se toca).

```bash
# Ollama: adopta el que ya corre en 11434, o arranca uno propio
antos service up ollama
antos service up ollama 11500          # segundo Ollama, con sus propios modelos en $STATE/services/ollama/data

# PostgreSQL (initdb la primera vez; usuario antos, auth trust en loopback)
antos service up postgres
antos service up postgres 5433 mi_db   # puerto y nombre de base de datos

# Redis
antos service up redis

# Tabla de servicios: puerto, estado, origen, salud, PID y variable inyectada
antos services

# Últimas líneas del log de un servicio arrancado por antOS
antos service logs ollama
antos service logs postgres 100

# Detener (los datos en $STATE/services/<svc>/data se conservan siempre)
antos service down postgres
antos service down ollama              # si era external, solo retira el registro
```

Por intención (pasa por el recinto: `env.service_up` declara escrituras en
`$STATE/services/<svc>/` y red en `127.0.0.1`, `tier confirm`):

```bash
antos "levanta ollama en el puerto 11500"
antos "estado del servicio ollama"
antos "detén el servicio ollama"
```

El proceso se separa de la sesión (`setsid`), así que sobrevive al CLI, a la
terminal y al ejecutor confinado que lo lanzó; no sobrevive a un reinicio
(eso es de la imagen, T34.3). Su entorno es mínimo: `PATH`, un `HOME` propio
en `$STATE/services/<svc>/home`, y ninguna credencial del usuario.

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
antos llm use ollama --model qwen2.5-coder:7b
antos llm use opencode --endpoint http://127.0.0.1:8080/v1
antos llm use local               # Planificador determinista local offline

# Restablecer selección automática (local primero: Ollama → OpenCode → claves → determinista)
antos llm use --clear

# Prueba interactiva de inferencia y Tool Calling con el motor activo
antos llm test
antos llm test --prompt "crea un microservicio en rust"
```

**Motor local con Ollama (T34.2).** Todo por la API HTTP de Ollama en
loopback, así que funciona aunque el binario `ollama` no esté en `PATH`
(backend `nix` de `antos service up ollama`):

```bash
# De cero a agente sin claves: servicio → diagnóstico → modelo → configuración
antos llm setup                      # pregunta en cada paso
antos -y llm setup --model qwen2.5-coder:14b

# RAM, tier recomendado, modelos (tamaño, params, cuantización, tools, ctx), contexto efectivo
antos llm doctor

# Modelos: descargar con progreso, listar, borrar
antos llm pull qwen2.5-coder:7b
antos llm list
antos llm rm qwen2.5-coder:0.5b      # pide confirmación (-y para scripts)

# Ventana de contexto pedida a Ollama (por defecto 16384; Ollama solo usaría 4096)
antos llm ctx                        # ver
antos llm ctx 32768                  # para todos los roles
antos llm ctx 8192 --role architect  # solo un rol

# Temperatura del agente (0.3 por defecto; el planificador va a 0.0)
antos llm temperature 0.7

# Reparto de roles antFlow: todo local, híbrido (nube con clave para Arquitecto/Auditor) o el reparto en nube
antos llm profile                    # ver
antos llm profile local
antos llm profile hybrid
```

**En antOS Linux, Ollama ya viene en la imagen** (T34.3): el escritorio
activa `services.antos.llm` (sobre `services.ollama` de nixpkgs), que deja el
demonio en `127.0.0.1:11434` desde el arranque, sin modelos. `antos services`
lo muestra como `external · systemd: ollama.service`; `antos service up
ollama` lo adopta en vez de arrancar otro; `antos service down ollama` te
remite a `systemctl stop ollama`. Para quitarlo de la imagen:
`services.antos.llm.enable = false;` en `/etc/nixos`; para exponerlo fuera de
loopback (no recomendado: no tiene autenticación) `services.antos.llm.host`,
que avisa al evaluar. `models = [ "qwen2.5-coder:7b" ]` lo preinstala en la
activación si lo quieres declarado. La ISO en vivo no lo trae (corre desde
RAM). `antos pkg install ollama` ya no existe como receta: apunta a esto.

`doctor` y `setup` eligen el modelo por la RAM de la máquina según
[`system/llm/models.toml`](../system/llm/models.toml) (editable sin
recompilar; el binario lleva una copia de respaldo). El Ollama al que hablan
es, por orden: el `endpoint` configurado con `antos llm use ollama
--endpoint …`, el registrado por `antos service up ollama` (aunque esté en
otro puerto), `OLLAMA_HOST`, `127.0.0.1:11434`.

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

```bash
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
* `Super + Space`: Abre o enfoca la barra de intenciones de antOS (`antos-barra`) en **modo dual**
  (T25.4): si el texto coincide con el nombre o ejecutable de una aplicación instalada (`antpkg`,
  Flatpak o del sistema anfitrión) se muestra una lista flotante de resultados con icono estilo
  Spotlight/Raycast — navegable con `↑`/`↓`/`Tab` y lanzable con `Enter` (inyectando
  `$ANTOS_WORKSPACE` si es un IDE); si no, mantiene el flujo normal de intenciones de IA.
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

Motor de despliegue guiado (interactivo por consola) y desatendido (archivo de configuración TOML) para instalar antOS en discos duros físicos o unidades NVMe/SATA SSD durante la sesión Live USB:

* **Paso 1: Detección de Hardware y Discos:** Inspecciona CPU, memoria RAM, modo de arranque (UEFI vs BIOS) y lista los dispositivos físicos (`/dev/nvme0n1`, `/dev/sda`) indicando capacidad, tipo de bus y si contienen sistemas preexistentes.
* **Paso 2: Modo de Instalación:**
  * **Opción 1: Disco Completo (Instalación Limpia):** Requiere confirmación explícita escribiendo en mayúsculas `SI`. Genera tabla GPT con partición ESP (512 MB, FAT32), partición Swap y partición raíz `/` (`ext4`).
  * **Opción 2: Dual Boot Seguro:** Preserva particiones de Windows/Linux y la partición ESP preexistente, utilizando espacio libre contiguo o redimensionando sin pérdida.
* **Paso 3: Parámetros del Sistema:** Configuración interactiva de `hostname`, zona horaria, distribución de teclado (`keymap`), nombre de usuario y contraseña.
* **Paso 4: Despliegue con Barra de Progreso:** Formateo de sistemas de archivos, montaje en `/mnt/target`, copia secuencial del sistema base con porcentaje visual, y generación de `/etc/fstab` con UUIDs persistentes.
* **Paso 5: Registro UEFI y Finalización:** Inscribe la entrada `"antOS Linux"` en la NVRAM con `efibootmgr`, desmonta particiones de manera limpia y notifica que se puede extraer la memoria USB para reiniciar.

```bash
# Iniciar el asistente guiado interactivo por terminal (5 pasos)
antos install

# Listar discos compatibles y recomendaciones (Limpio vs Dual Boot)
antos install --list
antos install list

# Instalación desatendida / automatizada mediante archivo de configuración TOML
antos install --config /etc/antos/install.toml
antos install -c mis_parametros.toml

# Despliegue directo en modo Dual Boot (simulación segura por defecto)
antos install --target /dev/nvme0n1 --dual-boot

# Despliegue directo en modo Disco Completo (simulación segura)
antos install --target /dev/sda --clean

# Aplicar la instalación definitiva en el hardware real (requiere confirmación)
antos install --target /dev/nvme0n1 --clean --apply
```

#### Ejemplo de Archivo de Configuración Desatendido (`install.toml`):
```toml
target_device = "/dev/nvme0n1"
clean_install = false          # false = Dual Boot seguro, true = disco completo
hostname = "antos-workstation"
timezone = "America/Bogota"
keymap = "es"
username = "developer"
password_hash = "$6$rounds=50000$..."
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

Gestor de paquetes inmutable y reproducible `antpkg` basado en almacén direccionado por contenido
(CAS) con rollback instantáneo por generaciones (T16.2). Desde la Fase 25 también entiende
aplicaciones gráficas: entradas `.desktop` XDG (T25.1) y un catálogo oficial de recetas para
navegadores e IDEs (T25.3, ej. `firefox`, `google-chrome`, `visual-studio-code`, `zed`).

```bash
# Buscar recetas en el catálogo oficial (incluye navegadores e IDEs, T25.3)
antos pkg search ripgrep
antos pkg search chrome

# Ver información detallada de un paquete o receta
antos pkg info curl

# Instalar paquete o receta declarativa TOML
antos pkg install curl
antos pkg install recetas/ripgrep.toml
antos pkg install visual-studio-code       # receta GUI: genera y registra su .desktop (T25.1)

# Simular instalación (Dry-Run)
antos pkg install jq --dry-run

# Listar paquetes en el perfil activo y generaciones anteriores
antos pkg list
antos pkg list --gui                       # solo aplicaciones gráficas instaladas

# Listar aplicaciones de escritorio con entrada XDG (.desktop) registrada (T25.1)
antos pkg apps

# Validar un archivo .desktop contra la especificación Freedesktop (T25.1)
antos pkg validate ~/.local/share/applications/code.desktop

# Comprobar que el store tiene lo que el perfil declara (existencia; no hashes ni firmas)
antos pkg verify

# Desinstalar un paquete del perfil activo
antos pkg remove curl

# Revertir el perfil activo a una generación previa
antos pkg rollback
antos pkg rollback 1

# Estado general del almacén y del perfil activo
antos pkg status
```

**Lo que antpkg hace y lo que no (T34.5).** El store por generaciones, el
`rollback` y las entradas XDG son reales. **No se descarga ninguna fuente**:
`source.url`/`source.sha256` de una receta son declaraciones que nada
comprueba contra bytes reales, y el binario que queda en el store es un
envoltorio simulado que imprime «antpkg wrapper …». El informe de `install`
lo dice en dos líneas (`Firma:` y `Fuente:`) y `checksum_verified` es
siempre `false`. La **firma sí se verifica de verdad**: Ed25519 con
`ed25519-dalek` sobre `nombre:versión:sha256` con la `signer_public_key`
de la receta; una receta firmada que no verifica no se instala, y una sin
firma se instala como `sin firma en la receta` (ninguna de las recetas del
repo va firmada: llevaban una firma de relleno que solo se comprobaba por
formato, y se retiró). Los nombres que no son fichero, receta ni utilidad
conocida (`ripgrep`, `fd`, `bat`, `jq`, `git`, `curl`, `tree`, `htop`) son
un error con sugerencia, no un paquete inventado. Para firmar una receta:

```bash
# mensaje = "<name>:<version>:<sha256>"; clave y firma en hexadecimal
signature = "<ed25519 sobre el mensaje>"
signer_public_key = "<clave pública, 64 hex>"
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

### 4.38 Matriz de CI/CD Local Paralela y Git Hooks Inteligentes (`antos ci` / `antos hook`)

Motor de integración continua local ultra-rápido y paralelo (T20.3) con soporte para configuración declarativa (`.antos/ci.toml`), detección automática de tecnologías (Rust, Node.js, Python), auditoría de secretos de alta entropía y gestión de *Git Hooks* (pre-commit y pre-push) supervisados por el Auditor:

```bash
# 1. Ejecutar pipeline completo de CI local en sandboxes
antos ci run
antos ci

# 2. Ejecutar únicamente etapas rápidas (linters, formato, escaneo de secretos)
antos ci run --fast
antos ci -f

# 3. Ejecutar una etapa específica
antos ci run --stage security
antos ci run --stage lint
antos ci run --stage test

# 4. Consultar el estado y métricas del último pipeline ejecutado
antos ci status

# 5. Instalar hooks de Git pre-commit y pre-push protegidos por antOS
antos hook install

# 6. Auditar manualmente el área de stage / workspace contra fuga de secretos
antos hook check

# 7. Consultar estado de los hooks instalados
antos hook status

# 8. Desinstalar hooks de antOS
antos hook uninstall

# 9. Ejecución declarativa vía lenguaje natural
antos "ejecuta ci rápido"
antos "instala pre-commit hook"
antos "audita pre-commit"
```

---

### 4.39 Instantáneas Atómicas de Entorno y Time Machine de Estado (`antos snapshot`)

Motor de snapshots atómicos y máquina del tiempo (T20.4) para congelar y revertir en menos de 200 ms el estado completo del entorno: árbol de código (incluidos archivos untracked), bases de datos de servicios locales efímeros (PostgreSQL, Redis, MariaDB) y grafo de memoria semántica:

```bash
# 1. Crear una instantánea atómica con etiqueta descriptiva
antos snapshot create pre-refactor-auth
antos snapshot create milestone-1 --author "dev-team"

# 2. Listar la cronología de instantáneas registradas
antos snapshot list
antos snapshot ls

# 3. Restaurar el entorno al punto exacto capturado (con snapshot de rescate previo automático)
antos snapshot restore pre-refactor-auth
antos snapshot restore snap-1a06984

# 4. Restaurar sin generar snapshot de rescate previo
antos snapshot restore pre-refactor-auth --no-rescue

# 5. Eliminar una instantánea liberando espacio
antos snapshot delete pre-refactor-auth

# 6. Uso declarativo mediante lenguaje natural
antos "crea snapshot llamado pre-deploy"
antos "restaura snapshot pre-deploy"
antos "lista snapshots del time machine"
```

---

### 4.40 Benchmarking Continuo y Detección de Regresiones de Rendimiento (`antos bench`)

Motor de benchmarking continuo y comparación de rendimiento estadístico (T21.1) para medir el impacto de las modificaciones de código antes de fusionarlas a la rama base, calculando latencia media, p95, p99, operaciones por segundo y consumo de memoria RSS pico:

```bash
# 1. Ejecutar la suite de benchmarks del proyecto o microbenchmarks integrados
antos bench
antos bench [nombre_suite]

# 2. Comparar rendimiento entre ramas o worktrees detectando regresiones
antos bench diff
antos bench diff --against master
antos bench diff --against main --threshold 10

# 3. Consultar el historial cronológico de ejecuciones
antos bench history

# 4. Uso declarativo mediante lenguaje natural
antos "ejecuta benchmarks del proyecto"
antos "compara rendimiento contra master"
antos "muestra historial de rendimiento"
```

---

### 4.41 Sincronización con Forjas Git: Issues y Pull Requests (`antos issue` / `antos pr`)

Integración bidireccional nativa con plataformas remotas de control de versiones (GitHub y GitLab) a través de tokens seguros gestionados en la bóveda (`system/antosd/src/vault.rs`):

```bash
# 1. Listar issues abiertos en el repositorio remoto configurado en origin
antos issue list

# 2. Importar un issue remoto convirtiéndolo en un ticket técnico en docs/tickets/
antos issue import 42
antos issue import https://github.com/usuario/repo/issues/42

# 3. Formular y publicar un Pull Request / Merge Request con certificación del Auditor
antos pr create
antos pr create --draft
antos pr create --title "feat(core): nueva funcionalidad" --base master

# 4. Consultar el estado y resultados de CI checks del Pull Request
antos pr status
antos pr status 1

# 5. Planificación declarativa en lenguaje natural
antos "lista issues remotos"
antos "importa issue 42"
antos "crea pull request"
antos "revisa estado del pull request"
```

---

### 4.42 Generador y Sincronizador de Documentación Viva y Diagramas Mermaid (`antos doc`)

Genera diagramas arquitectónicos vivos en formato Mermaid a partir del análisis en tiempo real del AST del workspace, los contratos IPC (`system/protocolo`) y el catálogo de capacidades (`system/capabilities`), manteniéndolos permanentemente sincronizados con el código fuente mediante bloques delimitados `<!-- ANTOS_ARCH_START -->`:

```bash
# 1. Generar e imprimir diagramas arquitectónicos en terminal
antos doc arch                           # Arquitectura completa (C4 + Flujo IPC + antFlow)
antos doc arch --type components         # Diagrama C4 de componentes y límites de seguridad
antos doc arch --type flow               # Diagrama de secuencia de intenciones y eventos IPC
antos doc arch --type antflow            # Diagrama de máquina de estados del equipo multi-agente

# 2. Exportar diagrama Mermaid a un archivo
antos doc arch --type components --output arquitectura.mmd

# 3. Sincronizar e incrustar diagramas vivos en la documentación Markdown
antos doc sync                           # Sincroniza docs/arquitectura.md y README.md
antos doc sync docs/arquitectura.md      # Sincroniza un archivo específico

# 4. Modo CI: Verificar que la documentación no haya divergido del código fuente
antos doc check                          # Falla con código != 0 si la doc está desactualizada
antos doc check docs/arquitectura.md

# 5. Planificación declarativa en lenguaje natural
antos "genera diagrama de arquitectura"
antos "sincroniza documentacion de arquitectura"
antos "verifica documentacion de arquitectura"
```

---

### 4.43 Gestor y Grabador Seguro de Memorias Live USB (`antos usb`)

Automatiza la creación de medios de arranque extraíbles y el volcado seguro en memorias pendrive USB para instalar o probar antOS en hardware real. Incluye mecanismos de protección de disco que impiden la sobreescritura accidental de unidades internas (NVMe/SATA del sistema anfitrión), reporte de progreso en bloques de 4 MiB y validación criptográfica SHA-256:

```bash
# 1. Listar unidades USB extraíbles detectadas de forma segura (excluye discos internos)
antos usb list
antos usb devices

# 2. Generar la imagen híbrida autoarrancable (UEFI + BIOS MBR) y su checksum SHA-256
antos usb build
antos usb build --arch x86_64 --out target/antos-live-x86_64.iso
antos usb build --arch aarch64 --out target/antos-live-aarch64.iso

# 3. Grabar la imagen en una memoria USB (Dry-Run de simulación segura por defecto)
antos usb flash --image target/antos-live-x86_64.iso --target /dev/sdb

# 4. Aplicar la grabación real en el pendrive (requiere confirmación y --apply)
antos usb flash --image target/antos-live-x86_64.iso --target /dev/sdb --apply

# 5. Verificar la integridad de la memoria USB grabada frente a la imagen original
antos usb verify --image target/antos-live-x86_64.iso --target /dev/sdb
```

#### Características Clave del Grabador USB:
* **Protección de Discos Internos:** Filtra automáticamente discos marcados como internos o del sistema operativo (`diskutil` en macOS, `lsblk` en Linux), permitiendo seleccionar únicamente unidades extraíbles con bus USB.
* **Streaming en Bloques de 4 MiB:** Escritura directa con buffer de alto rendimiento, cálculo de velocidad de transferencia en MB/s y barra de progreso porcentual.
* **Sincronización a Hardware y Verificación:** Ejecuta `sync`/`fsync` al finalizar y verifica el hash SHA-256 para asegurar que no existan sectores corruptos en el pendrive.
* **Compatibilidad Multiboot:** La imagen ISO híbrida generada es compatible directamente con herramientas como **Rufus** (modo DD y modo ISO), **BalenaEtcher** y arranque directo en **Ventoy**.

---

### 4.44 Gestor y Puente de Aplicaciones Flatpak y Contenedores Gráficos (`antos app`)

Capa unificada sobre `antpkg` (nativas) y Flatpak/Flathub (en sandbox) para descubrir, instalar y
lanzar aplicaciones de escritorio con inyección automática del workspace activo (T25.2) — el mismo
motor (`AppEngine`) que alimenta el lanzador `Super + Space` de `system/barra` (T25.4, ver
[4.22](#422-entorno-de-escritorio-wayland-y-atajos-globales-antos-desktop)).

```bash
# Buscar aplicaciones en Flathub y en las recetas antpkg (unificado)
antos app search visual studio code

# Listar aplicaciones instaladas (opcionalmente filtradas por origen)
antos app list
antos app list --flatpak
antos app list --pkg

# Instalar una aplicación, forzando el origen si hace falta
antos app install com.visualstudio.code
antos app install org.mozilla.firefox --source flatpak

# Ejecutar una aplicación inyectando el contexto Wayland y el workspace activo
antos app run code
antos app run code --workspace ~/proyectos/api-service

# Desinstalar una aplicación y revocar sus accesos
antos app remove com.visualstudio.code
```

---

### 4.45 Abstracción de Plataforma Runtime (`antos runtime`)

Diagnóstico de la capa `PlatformRuntime` (T22.4): detecta el sistema operativo anfitrión y reporta,
por subsistema, la **fidelidad** real de aislamiento disponible — no todo backend impone lo mismo
(p. ej. Landlock LSM aplica en Linux; en macOS `Seatbelt` cubre ese rol y Landlock aparece como no
disponible en vez de fallar en silencio).

```bash
# Matriz de capacidades del runtime detectado (Landlock/Seatbelt, Cgroups v2, eBPF, KVM, Wayland)
antos runtime

# Salida en JSON para integraciones y scripts
antos runtime --json
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
