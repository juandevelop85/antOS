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
   - [Método 6: Núcleo Bare-Metal `no_std` x86_64 en QEMU](#método-6-núcleo-bare-metal-no_std-x86_64-en-qemu)
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
   - [4.13 Inferencia LLM Local con Ollama (`antos llm`)](#413-inferencia-llm-local-con-ollama-antos-llm)
   - [4.14 Bitácora Inmutable y Reversión Atómica (`antos log`, `antos undo`)](#414-bitácora-inmutable-y-reversión-atómica-antos-log-antos-undo)
   - [4.15 Red P2P Cifrada antMesh (`antos mesh`)](#415-red-p2p-cifrada-antmesh-antos-mesh)
   - [4.16 Swarm Multi-Nodo y Despacho Distribuido (`antos swarm`)](#416-swarm-multi-nodo-y-despacho-distribuido-antos-swarm)
   - [4.17 Sistema de Ficheros Virtual Semántico (`antos vfs`)](#417-sistema-de-ficheros-virtual-semántico-antos-vfs)
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
 │ 6. Kernel Bare-Metal no_std en QEMU: Arranque x86_64 directo en firmware    │
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

### Método 6: Núcleo Bare-Metal `no_std` x86_64 en QEMU

Compila el kernel propio de antOS sin biblioteca estándar (`no_std`), genera la imagen arrancable con `builder` y la ejecuta en QEMU:

```bash
# Compilar núcleo y arrancar en QEMU
./run.sh
```

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

Motor *SpecEngine* para indexación, creación y actualización de especificaciones de desarrollo:

```bash
# Listar todos los tickets del proyecto con su fase, ID y estado actual
antos tickets

# Ver el detalle técnico estructurado de un ticket específico
antos ticket T1.3

# Crear un nuevo ticket técnico dinámico en docs/tickets/
antos ticket new T9.1 "Integración con sistema de métricas Prometheus" --fase "Fase 9" --desc "Añadir colector de métricas en antosd"

# Actualizar el estado de un ticket en su archivo Markdown y en el README
antos ticket status T9.1 en_progreso
antos ticket status T9.1 en_revision
antos ticket status T9.1 completado
```

---

### 4.5 Visor de Diffs Interactivo y Consola VTE (`antos diff` / `antos terminal`)

Inspección de cambios de código con resaltado sintáctico y acceso a consola de comandos embebida:

```bash
# Ver el diff sintáctico coloreado de los cambios de trabajo contra HEAD
antos diff

# Ver el diff contra una rama, commit o ticket específico
antos diff main
antos diff feature/auth
antos diff HEAD~1

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

### 4.13 Inferencia LLM Local con Ollama (`antos llm`)

Gestión de modelos de lenguaje locales para operación 100% offline y soberana:

```bash
# Consultar el estado de conexión con el servicio local de Ollama
antos llm status

# Listar modelos descargados y disponibles en la máquina
antos llm list

# Descargar un nuevo modelo de código local
antos llm pull qwen2.5-coder:7b
antos llm pull deepseek-coder:6.7b
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

```bash
# Iniciar el servidor LSP sobre stdio (utilizado directamente por editores externos)
antos lsp
antos lsp stdio

# Consultar el estado del servidor y cantidad de símbolos AST indexados
antos lsp status

# Generar configuraciones automáticas listas para usar en cada editor
antos lsp config vscode
antos lsp config neovim
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

Automatización de la compilación cruzada para el target `x86_64-unknown-none`, generación de imágenes de disco arrancables BIOS/MBR y validación en la máquina virtual QEMU:

```bash
# Diagnosticar estado de los artefactos (kernel ELF, imagen BIOS y disponibilidad de QEMU)
antos boot status
antos boot

# Compilar el kernel no_std y generar la imagen arrancable de disco
antos boot build

# Ejecutar prueba automatizada de arranque en QEMU headless con verificación por serial
antos boot test

# Lanzar la máquina virtual QEMU de forma interactiva
antos boot qemu
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
