# antOS 🐜⚡

> **El Sistema Operativo Personal para Desarrolladores impulsado por IA y Orquestación Multi-Agente Nativa.**

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-108%2F108%20Passed-brightgreen.svg)]()
[![Wayland](https://img.shields.io/badge/UI-Wayland%20GTK4-blue.svg?logo=gnome)]()
[![Security](https://img.shields.io/badge/Sandbox-Landlock%20%2F%20Seatbelt-purple.svg)]()
[![Tickets Backlog](https://img.shields.io/badge/Backlog-28%2F34%20Completados-blue.svg)](docs/tickets/README.md)
[![Manual de Comandos](https://img.shields.io/badge/Documentaci%C3%B3n-Manual%20de%20Comandos-blueviolet.svg)](docs/manual-de-comandos.md)

---

## 🌟 ¿Qué es antOS?

Los sistemas operativos convencionales (macOS, Windows, Linux) fueron diseñados bajo paradigmas de los años 70 y 90: solo entienden flujos de bytes planos, otorgan **autoridad ambiental total** a cualquier script o agente de IA, y carecen por completo de contexto sobre repositorios Git, puertos de red, pruebas o dependencias de software.

**antOS** es un sistema operativo diseñado desde sus cimientos para el **desarrollador de software**. Transforma la máquina en un entorno donde:
1. **El repositorio y el proyecto son ciudadanos de primera clase:** El sistema comprende ramas activas, diffs en tiempo real, linters, memoria semántica vectorial y grafos de contexto.
2. **Orquestación Multi-Agente Nativa (`antFlow`):** Un equipo de roles especializados (*Arquitecto 📐, Coder 💻, QA 🧪, Auditor 🛡️*) opera localmente como demonios del sistema operativo sobre *Git Worktrees* efímeros y aislados.
3. **Cero Autoridad Ambiental y Recinto de Seguridad (*Sandboxing*):** La IA no ejecuta comandos ciegos en Bash. Planifica contra un catálogo de **capacidades tipadas**, calcula su **radio de impacto (*Blast Radius*)** antes de tocar el disco, opera bajo restricciones del kernel (*Landlock* en Linux / *Seatbelt* en macOS) y protege credenciales `.env`/SSH con **concesiones temporales explícitas (`grants`)**.
4. **Cuotas y Límites de Recursos en Tiempo Real:** Supervisión mediante *Cgroups v2* en Linux y supervisor de procesos con límites duros de memoria RSS y timeout en macOS.
5. **Reversibilidad Nativa (`undo`):** Cada plan, commit o ciclo de desarrollo genera una instantánea atómica previa, permitiendo revertir cualquier cambio con `antos undo` o `antos undo --ticket <id>`.
6. **Superficie de Escritorio Moderna:** Shell Wayland GTK4 de latencia ultra-baja con barra de intenciones contextual, panel de control Kanban (`Super + A`), visor interactivo de diffs sintácticos, consola terminal VTE y bandeja de notificaciones asíncronas.

> 📖 **Para una referencia completa de comandos, banderas y opciones de arranque, consulta el [Manual Completo de Comandos y Métodos de Arranque](docs/manual-de-comandos.md).**

---

## 🏗️ Arquitectura del Sistema

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                         SUPERFICIE DE ESCRITORIO                            │
 │  HUD de Intenciones · Tablero Kanban (Super + A) · Diffs · Consola VTE      │
 │  Bandeja de Notificaciones y Aprobaciones Asíncronas · Insignias Git en Vivo│
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ IPC Tipado (antos-protocolo)
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                      MOTOR DE CONTEXTO Y MULTI-AGENTE                       │
 │  - Orquestador de Agentes antFlow (Arquitecto, Coder, QA, Auditor)          │
 │  - Spec Engine: Parser nativo de tickets Markdown (docs/tickets/)           │
 │  - Memoria Semántica (SQLite Vectorial) & Grafo de Contexto del Proyecto    │
 │  - Motor de Inferencia LLM Local con Ollama (Qwen2.5-Coder) & Fallback      │
 │  - Gestor Declarativo de Entornos y Toolchains (Nix / Devbox)               │
 │  - Analizador Git con caché mtime & Worktrees Efímeros Aislados             │
 │  - Gestor de Servicios Efímeros (Postgres, Redis, MariaDB) & Puertos        │
 │  - Bóveda de Secretos (vault.json) & Sistema de Concesiones (grants)        │
 │  - Bandeja de Notificaciones & Rollback Asíncrono por Ticket                │
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ Capacidades Tipadas & Sandboxing
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                   NÚCLEO DE EJECUCIÓN AISLADA (antosd)                      │
 │  Planificador IA (Local/Ollama/Claude) · Cálculo Blast Radius               │
 │  Recinto Sandbox (Landlock LSM / macOS Seatbelt) & Control de Cuotas        │
 │  Bitácora Inmutable (journal.jsonl) · Instantáneas & Reversión Atómica      │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### Estructura del Workspace (Crates de Rust)

* **[`system/protocolo`](system/protocolo):** Crate `antos-protocolo` con tipos puros de intercambio IPC, serializables con `serde`. Cero dependencias pesadas de I/O.
* **[`system/antosd`](system/antosd):** Demonio `antosd` y CLI `antos`. Contiene el planificador local/Ollama/Claude, orquestador `antFlow`, memoria semántica, gestor de cuotas, recinto sandbox y motor de ejecución.
* **[`system/capabilities`](system/capabilities):** Manifiestos TOML tipados que definen contratos, parámetros, efectos y niveles de riesgo de cada capacidad del desarrollador.
* **[`system/barra`](system/barra):** Shell de escritorio Wayland / GTK4 Layer Shell con barra flotante de intenciones, insignias en vivo, visor de diffs, consola VTE y centro de control Kanban.
* **[`kernel`](kernel):** Núcleo `no_std` en Rust para arranque en metal desnudo x86_64.
* **[`builder`](builder):** Ensamblador de imágenes de arranque.
* **[`docs/tickets`](docs/tickets):** Backlog y especificaciones técnicas maestro (*Spec-Driven Development*).

---

## 🚀 Guía de Inicio y Puesta en Marcha

### 1. Requisitos Previos y Herramientas

* **Rust Toolchain:** `rustc` y `cargo` (1.75 o superior).
* **Git:** 2.30 o superior.
* **Ollama (Opcional para inferencia LLM local offline):** [ollama.com](https://ollama.com) con modelos como `qwen2.5-coder:7b`.
* **Nix / Devbox (Opcional para entornos declarativos):** Para aprovisionar toolchains reproducibles.
* **QEMU / Podman / Docker (Opcional para VM y Kernel):** `qemu-system-x86_64`, `podman` o `docker`.
* **(Opcional para Claude):** Variable de entorno `ANTHROPIC_API_KEY` para planificación en la nube.
* **(Opcional para Desktop Wayland):** `gtk4` y `gtk4-layer-shell`.

### 2. Compilación del Workspace y Verificación

Compila todos los crates del workspace y verifica la suite de pruebas (**66/66 pruebas automatizadas en verde**):

```bash
# Compilar todo el workspace
cargo build --workspace

# Ejecutar la suite completa de pruebas unitarias y de integración
cargo test --workspace
```

### 3. Configurar el Comando `antos` en tu Terminal

Para usar el comando `antos` directamente desde cualquier directorio de tu sistema:

```bash
# Opción A: Instalar el binario directamente en tu PATH (~/.cargo/bin)
cargo install --path system/antosd

# Opción B: Crear un alias en tu shell (~/.zshrc o ~/.bashrc)
alias antos="$(pwd)/target/debug/antos"
```

#### Variables de Entorno de antOS

antOS autodescubre el contexto, pero puedes personalizar su comportamiento:

| Variable | Descripción | Valor por Defecto |
| :--- | :--- | :--- |
| `ANTOS_WORKSPACE` | Raíz del proyecto en el que opera antOS | Raíz del repositorio Git actual o `pwd` |
| `ANTOS_STATE` | Directorio de estado (bitácora, servicios, bóveda de secretos) | `.antos/` en el workspace o `~/.local/state/antos/` |
| `ANTOS_SOCKET` | Ruta del socket UNIX del demonio | `$ANTOS_STATE/antos.sock` |
| `ANTOS_CAPABILITIES` | Directorio con los manifiestos TOML de capacidades | `system/capabilities/` |
| `OLLAMA_HOST` | URL del servidor Ollama para inferencia local | `http://localhost:11434` |
| `ANTHROPIC_API_KEY` | Clave de API de Anthropic para el planificador Claude | `~/.config/antos/anthropic.key` |

---

## 🕹️ Modos de Ejecutar e Iniciar antOS

antOS cuenta con **6 métodos de arranque** adaptados a cada escenario:

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                       6 FORMAS DE INICIAR antOS                             │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 1. CLI y Centro de Control (Host): Desarrollo diario en macOS y Linux       │
 │ 2. Demonio IPC en Segundo Plano: Escucha en socket UNIX y atiende clientes  │
 │ 3. Shell Gráfico Wayland (GTK4): HUD contextual flotante y Kanban (Super+A) │
 │ 4. Contenedor Linux (Landlock LSM): Verificación de aislamiento kernel      │
 │ 5. Máquina Virtual NixOS en QEMU: Sistema operativo completo y servicios    │
 │ 6. Kernel Bare-Metal no_std en QEMU: Arranque x86_64 directo en firmware    │
 └─────────────────────────────────────────────────────────────────────────────┘
```

> 📖 **Para conocer todas las combinaciones de banderas y opciones avanzadas, consulta el [Manual Completo de Comandos y Métodos de Arranque](docs/manual-de-comandos.md).**

---

## 🛠️ Guía Rápida de Comandos por Subsistema

### 1. Planificación e Intenciones en Lenguaje Natural
```bash
# Ejecución interactiva con cálculo de radio de impacto
antos "crea un proyecto rust llamado api-service"

# Modo simulación (dry-run): inspecciona el diff y plan sin modificar nada
antos -n "actualiza las dependencias de cargo y compila"

# Usar el modelo LLM local Ollama
antos -p ollama "optimiza las consultas del módulo memory.rs"
```

### 2. Orquestador Multi-Agente (`antFlow`) y Tablero Kanban
```bash
# Consultar el equipo de agentes especializados del sistema operativo
antos agents

# Despachar un ticket técnico (Arquitecto -> Coder -> QA -> Auditor)
antos agent run T1.1 --auto

# Consultar el estado y worktree activo del ticket
antos agent status T1.1

# Abrir el Tablero Kanban y monitor de agentes en consola
antos panel
```

### 3. Visor de Diffs Interactivo y Consola VTE
```bash
# Inspeccionar diffs sintácticos coloreados con números de línea dobles
antos diff

# Comparar contra una rama o commit específico
antos diff main

# Consola terminal interactiva VTE embebida
antos terminal
antos terminal "cargo check"
```

### 4. Bandeja de Notificaciones y Aprobaciones Asíncronas
```bash
# Consultar la bandeja de alertas y revisiones pendientes
antos notify

# Aprobar cambios y fusionar el worktree del agente
antos notify approve notif-t8-2

# Rechazar y ejecutar rollback inmediato del worktree
antos notify reject notif-t8-2

# Limpiar notificaciones leídas
antos notify clear
```

### 5. Memoria Semántica y Grafo de Contexto
```bash
# Indexar el proyecto con vectores de términos y dependencias
antos memory index

# Búsqueda semántica por similitud coseno
antos memory search "orquestación multi-agente en worktrees"

# Visualizar el grafo de dependencias bidireccional
antos memory graph system/antosd/src/main.rs
```

### 6. Perfiles Declarativos Nix y Devbox
```bash
# Diagnosticar toolchains instaladas en el workspace
antos env status

# Inicializar un perfil de lenguaje (.antos/env.toml, devbox.json, flake.nix)
antos env init rust
antos env init python

# Sincronizar toolchains y dependencias
antos env sync
```

### 7. Cuotas y Límites de Recursos para Sandboxes
```bash
# Ver límites actuales de CPU, memoria RAM, PIDs y timeout
antos quota status

# Configurar límites estrictos para tareas de agentes
antos quota set --timeout 60 --memory 1024 --cpu 80 --pids 128

# Restablecer cuotas por defecto
antos quota reset
```

### 8. Bóveda de Secretos y Concesiones (Zero Environmental Authority)
```bash
# Guardar un secreto de forma segura (cifrado / 0600)
antos secret set GITHUB_TOKEN ghp_1122334455

# Listar secretos (lectura bloqueada sin concesión)
antos secrets

# Otorgar una concesión temporal con justificación
antos grant secret.GITHUB_TOKEN --minutos 15 --para "sincronizar releases"

# Leer el secreto concedido
antos secret get GITHUB_TOKEN

# Revocar el acceso
antos revoke secret.GITHUB_TOKEN
```

### 9. Servicios Locales Efímeros (PostgreSQL / Redis / MariaDB)
```bash
# Levantar PostgreSQL local efímero
antos service up postgres

# Consultar servicios activos y variables inyectadas (.env)
antos services

# Detener el servicio cuando termines
antos service down postgres
```

### 10. Reversión Atómica Instantánea (`undo`)
```bash
# Revertir la última transacción ejecutada restaurando el snapshot atómico
antos undo

# Revertir todos los commits y cambios asociados a un ticket técnico
antos undo --ticket T1.1
```

---

## 📋 Catálogo Completo de Capacidades (`system/capabilities/`)

| Capacidad | Descripción | Nivel de Riesgo |
| :--- | :--- | :--- |
| `git.status` | Introspección y estado estructurado del repositorio Git | `auto` (Lectura) |
| `git.commit_semantic` | Generación y aplicación de commits semánticos validados | `confirm` |
| `git.smart_branch` | Creación y cambio contextual de ramas | `confirm` |
| `git.worktree_create` | Creación de worktrees efímeros aislados para agentes | `confirm` |
| `git.worktree_cleanup` | Limpieza y eliminación de worktrees temporales | `confirm` |
| `git.worktree_merge` | Fusión de ramas de trabajo de agentes a la rama base | `confirm` |
| `diag.port_status` | Diagnóstico de puertos TCP de desarrollo y procesos en escucha | `auto` (Lectura) |
| `diag.port_kill` | Liberación controlada de puertos en conflicto (SIGTERM/SIGKILL) | `confirm` |
| `env.service_up` | Aprovisionamiento declarativo de servicios locales (Postgres, Redis) | `confirm` |
| `env.service_down` | Detención y limpieza de servicios locales efímeros | `confirm` |
| `env.service_status` | Consulta del estado de servicios locales aprovisionados | `auto` (Lectura) |
| `secret.set` | Almacenamiento seguro de secretos en la bóveda con cifrado/0600 | `confirm` |
| `secret.get` / `secret.read` | Lectura de credenciales protegida por el sistema de concesiones | `grant` |
| `secret.grant` | Concesión explícita temporal de acceso a un secreto o `.env` | `grant` |
| `secret.revoke` | Revocación inmediata de una concesión activa | `confirm` |
| `secret.list` | Listado de secretos y estado de concesiones activas | `auto` (Lectura) |
| `llm.status` | Diagnóstico del motor de inferencia LLM local Ollama | `auto` (Lectura) |
| `llm.list` | Listado de modelos LLM locales disponibles | `auto` (Lectura) |
| `memory.index` | Indexación sintáctica y vectorial del workspace en SQLite | `confirm` |
| `memory.search` | Búsqueda semántica por similitud coseno en memoria local | `auto` (Lectura) |
| `memory.graph` | Consulta del grafo de dependencias y contexto del proyecto | `auto` (Lectura) |
| `env.init` | Inicialización de perfiles de entorno declarativo (Nix/Devbox) | `confirm` |
| `env.sync` | Sincronización y validación de toolchains del proyecto | `confirm` |
| `env.profile_status` | Diagnóstico de perfiles y toolchains activas | `auto` (Lectura) |
| `quota.status` | Inspección de cuotas de CPU, memoria y timeouts de sandbox | `auto` (Lectura) |
| `quota.set` | Configuración de límites y cuotas de confinamiento | `confirm` |
| `ui.diff_viewer` | Visor interactivo de diffs estructurados y coloreados sintácticamente | `auto` (Lectura) |
| `ui.terminal` | Consola terminal interactiva VTE embebida | `auto` |
| `notify.list` | Consulta de la bandeja de notificaciones y aprobaciones de agentes | `auto` (Lectura) |
| `notify.action` | Ejecución de acciones de aprobación, rechazo o rollback | `confirm` |
| `mesh.status` | Muestra el estado del nodo local y los peers en la red P2P antMesh | `auto` (Lectura) |
| `mesh.connect` | Conecta a un nodo peer remoto mediante IP o multiaddr (QUIC) | `confirm` |
| `mesh.pair` | Genera un token seguro de emparejamiento con 15m de expiración | `auto` |
| `flow.swarm_status` | Inspección de nodos y matriz de distribución de agentes en el Swarm | `auto` (Lectura) |
| `flow.dispatch_remote` | Despacho distribuido de roles antFlow a nodos con sincronización de worktrees | `confirm` |
| `vfs.query` | Consulta símbolos, nodos del AST o diffs proyectados en /antfs | `auto` (Lectura) |
| `vfs.mount` | Monta la proyección del sistema de ficheros virtual /antfs en el workspace | `confirm` |
| `vfs.unmount` | Desmonta y limpia el punto de montaje virtual /antfs | `confirm` |
| `vfs.validate_write` | Intercepta y valida sintaxis de archivos en memoria previa a disco | `auto` |
| `vfs.guard_status` | Métricas y estado del interceptor de escrituras semánticas | `auto` (Lectura) |
| `ebpf.status` | Diagnóstico de compatibilidad y sondas eBPF LSM activas en el kernel | `auto` (Lectura) |
| `ebpf.audit_log` | Lectura del registro en vivo de syscalls y eventos de seguridad eBPF | `auto` (Lectura) |
| `profile.run` | Ejecución y perfilado en tiempo real de comandos (CPU, RSS, page faults) | `auto` |
| `profile.analyze` | Análisis de puntos calientes (hotspots) y sugerencias técnicas de optimización | `auto` (Lectura) |
| `lsp.start` | Inicia el servidor Language Server Protocol (LSP) sobre stdio para editores | `auto` |
| `lsp.status` | Diagnóstico de conexiones y símbolos indexados en el servidor LSP | `auto` (Lectura) |
| `collab.session` | Inicia o une una sesión interactiva de pair programming con el Coder mediante CRDT | `confirm` |
| `dap.attach` | Conecta una sesión de depuración supervisada DAP a un proceso dentro del sandbox | `auto` |
| `project.scaffold` | Creación y andamiaje inicial de proyectos (Rust, TS, Python) | `confirm` |
| `pkg.declare` | Declaración de dependencias en manifiestos de proyecto | `confirm` |
| `fs.write` / `fs.delete` | Modificación y eliminación controlada de archivos con instantánea | `confirm` / `grant` |
| `system.declare` | Modificación de la configuración declarativa del sistema | `grant` |

---

## 🗺️ Hoja de Ruta y Backlog de Desarrollo

El desarrollo de antOS se gestiona bajo la metodología **Spec-Driven Development** en [`docs/tickets/`](docs/tickets/):

| Fase | Título | Estado |
| :--- | :--- | :--- |
| **Fase 0** | [T0.1](docs/tickets/T0.1-renombrado-de-syso-a-antos.md) · Renombrado integral del sistema a `antOS` | ✅ Completado |
| **Fase 1** | [T1.1](docs/tickets/T1.1-protocolo-introspeccion-git.md) · Extensión de `antos-protocolo` para introspección Git | ✅ Completado |
| **Fase 1** | [T1.2](docs/tickets/T1.2-analizador-git-en-demonio.md) · Analizador de repositorios Git en segundo plano con caché `mtime` | ✅ Completado |
| **Fase 1** | [T1.3](docs/tickets/T1.3-spec-engine-tickets-parser.md) · Indexador y parser nativo de especificaciones y tickets Markdown | ✅ Completado |
| **Fase 2** | [T2.1](docs/tickets/T2.1-capacidades-git-semantico.md) · Capacidades tipadas para Git (`status`, `commit_semantic`, `branch`) | ✅ Completado |
| **Fase 2** | [T2.2](docs/tickets/T2.2-capacidades-worktrees-aislados.md) · Capacidad de gestión de *Git Worktrees* efímeros para agentes | ✅ Completado |
| **Fase 2** | [T2.3](docs/tickets/T2.3-diagnostico-de-puertos-y-procesos.md) · Capacidad de diagnóstico y liberación de puertos en colisión | ✅ Completado |
| **Fase 3** | [T3.1](docs/tickets/T3.1-roles-y-maquina-de-estados-de-agentes.md) · Orquestador de roles multi-agente (`antFlow` Core en Rust) | ✅ Completado |
| **Fase 3** | [T3.2](docs/tickets/T3.2-flujo-ejecucion-paralela-en-worktrees.md) · Ejecución paralela de tareas en worktrees con validación de tests | ✅ Completado |
| **Fase 3** | [T3.3](docs/tickets/T3.3-consolidador-de-diffs-y-rollback-por-ticket.md) · Consolidador de diffs y reversión granular por ticket (`undo --ticket`) | ✅ Completado |
| **Fase 4** | [T4.1](docs/tickets/T4.1-barra-intencion-actualizada.md) · Adaptación y enriquecimiento de la barra de intenciones Wayland/GTK4 | ✅ Completado |
| **Fase 4** | [T4.2](docs/tickets/T4.2-panel-centro-de-agentes-y-tickets.md) · Centro de control de agentes y tablero de tickets (`Super + A`) | ✅ Completado |
| **Fase 5** | [T5.1](docs/tickets/T5.1-servicios-locales-efimeros-nix.md) · Aprovisionamiento declarativo de servicios efímeros (Postgres, Redis) | ✅ Completado |
| **Fase 5** | [T5.2](docs/tickets/T5.2-boveda-segura-de-secretos-y-grants.md) · Bóveda de secretos y blindaje de `.env`/claves SSH con concesiones | ✅ Completado |
| **Fase 6** | [T6.1](docs/tickets/T6.1-motor-de-inferencia-llm-local-con-ollama-y-fallback-offline.md) · Motor de Inferencia LLM Local con Ollama y Fallback Offline | ✅ Completado |
| **Fase 6** | [T6.2](docs/tickets/T6.2-memoria-semántica-y-grafo-de-contexto-del-proyecto-con-sqlite-vectorial.md) · Memoria Semántica y Grafo de Contexto del Proyecto con SQLite Vectorial | ✅ Completado |
| **Fase 7** | [T7.1](docs/tickets/T7.1-gestor-de-perfiles-de-entorno-declarativo-nix-y-devbox-por-proyecto.md) · Gestor de Perfiles de Entorno Declarativo Nix y Devbox por Proyecto | ✅ Completado |
| **Fase 7** | [T7.2](docs/tickets/T7.2-control-de-cuotas-de-cpu-y-memoria-para-sandboxes-de-agentes.md) · Control de Cuotas de CPU y Memoria para Sandboxes de Agentes | ✅ Completado |
| **Fase 8** | [T8.1](docs/tickets/T8.1-visor-de-diffs-interactivo-y-terminal-embebido-en-wayland.md) · Visor de Diffs Interactivo y Terminal Embebido en Wayland | ✅ Completado |
| **Fase 8** | [T8.2](docs/tickets/T8.2-bandeja-de-notificaciones-y-aprobaciones-asíncronas-para-agentes.md) · Bandeja de Notificaciones y Aprobaciones Asíncronas para Agentes | ✅ Completado |
| **Fase 9** | [T9.1](docs/tickets/T9.1-protocolo-de-red-p2p-cifrado-antmesh-con-descubrimiento-mdns-y-quic.md) · Protocolo de Red P2P Cifrado (antMesh) con Descubrimiento mDNS y QUIC | ✅ Completado |
| **Fase 9** | [T9.2](docs/tickets/T9.2-despacho-distribuido-de-roles-antflow-a-nodos-de-gpu-y-sincronización-de-worktrees.md) · Despacho Distribuido de Roles antFlow a Nodos de GPU y Sincronización | ✅ Completado |
| **Fase 10** | [T10.1](docs/tickets/T10.1-sistema-de-ficheros-virtual-fuse-para-inspección-de-ast-símbolos-y-diffs-antfs.md) · Sistema de Ficheros Virtual FUSE para Inspección de AST, Símbolos y Diffs (/antfs) | ✅ Completado |
| **Fase 10** | [T10.2](docs/tickets/T10.2-interceptores-de-escritura-semántica-con-validación-tipada-previa-a-disco.md) · Interceptores de Escritura Semántica con Validación Tipada Previa a Disco | ✅ Completado |
| **Fase 11** | [T11.1](docs/tickets/T11.1-supervisor-kernel-ebpf-lsm-para-detección-de-fugas-de-sandbox-y-syscalls-anómalas.md) · Supervisor Kernel eBPF (LSM) para Detección de Fugas de Sandbox y Syscalls | ✅ Completado |
| **Fase 11** | [T11.2](docs/tickets/T11.2-profiler-continuo-de-cpu-y-memoria-en-runtime-con-sugerencias-de-optimización-para-agentes.md) · Profiler Continuo de CPU y Memoria en Runtime con Sugerencias de Optimización | ✅ Completado |
| **Fase 12** | [T12.1](docs/tickets/T12.1-servidor-language-server-protocol-lsp-unificado-alimentado-por-la-memoria-semántica.md) · Servidor LSP Unificado Alimentado por la Memoria Semántica | ✅ Completado |
| **Fase 12** | [T12.2](docs/tickets/T12.2-edición-colaborativa-en-vivo-humano-agente-y-protocolo-dap-de-depuración-aislada.md) · Edición Colaborativa en Vivo Humano-Agente y Protocolo DAP de Depuración Aislada | ✅ Completado |
| **Fase 13** | [T13.0](docs/tickets/T13.0-compositor-wayland-ultraligero-y-configuración-declarativa-de-sesión-de-escritorio.md) · Compositor Wayland Ultraligero y Configuración Declarativa de Sesión | ⏳ Pendiente |
| **Fase 13** | [T13.1](docs/tickets/T13.1-integracion-de-telemetria-ebpf-profiler-y-pair-programming-en-la-barra-wayland-gtk4.md) · Telemetría eBPF, Profiler y Pair Programming en Barra Wayland | ⏳ Pendiente |
| **Fase 13** | [T13.2](docs/tickets/T13.2-pipeline-de-arranque-bare-metal-compilacion-cruzada-de-kernel-y-disco-bios-uefi-en-qemu.md) · Pipeline de Arranque Bare Metal, Kernel y QEMU | ⏳ Pendiente |
| **Fase 14** | [T14.1](docs/tickets/T14.1-motor-de-capacidades-y-plugins-en-webassembly-wasi-con-aislamiento-de-memoria.md) · Motor de Capacidades y Plugins en WebAssembly (WASI) | ⏳ Pendiente |
| **Fase 14** | [T14.2](docs/tickets/T14.2-agente-multimodal-con-captura-de-pantalla-wayland-para-inspeccion-y-qa-visual.md) · Agente Multimodal con Captura Wayland e Inspección Visual | ⏳ Pendiente |
| **Fase 14** | [T14.3](docs/tickets/T14.3-generador-de-live-iso-autonoma-empaquetado-release-y-distribucion-v0.1.0.md) · Generador de Live ISO Autónoma y Empaquetado Release v0.1.0 | ⏳ Pendiente |

---

## 🛡️ Seguridad y Filosofía de Privacidad

* **Cero Filtración de Memoria o Variables Ambientales:** Los subprocesos y agentes nacen limpios; las credenciales no se heredan en variables globales.
* **Aislamiento por Kernel:** Restricciones de lectura/escritura mediante *Landlock LSM* en Linux y *Seatbelt* en macOS.
* **Control de Recursos:** Cgroups v2 y supervisor watchdog evitando procesos desbocados o fugas de memoria.
* **Privacidad y Soberanía Local:** Inferencia con modelos LLM locales mediante Ollama, memoria vectorial en SQLite local y ejecución offline por defecto.

---

## 📜 Licencia

antOS se distribuye bajo la licencia MIT. Consulta el archivo `LICENSE` para más información.
