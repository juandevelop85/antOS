# antOS 🐜⚡

> **El Sistema Operativo Personal para Desarrolladores impulsado por IA y Orquestación Multi-Agente Nativa.**

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-46%2F46%20Passed-brightgreen.svg)]()
[![Wayland](https://img.shields.io/badge/UI-Wayland%20GTK4-blue.svg?logo=gnome)]()
[![Security](https://img.shields.io/badge/Sandbox-Landlock%20%2F%20Seatbelt-purple.svg)]()
[![Tickets Backlog](https://img.shields.io/badge/Backlog-14%2F14%20Completados-success.svg)](docs/tickets/README.md)

---

## 🌟 ¿Qué es antOS?

Los sistemas operativos convencionales (macOS, Windows, Linux) fueron diseñados bajo paradigmas de los años 70 y 90: solo entienden flujos de bytes planos, otorgan **autoridad ambiental total** a cualquier script o agente de IA, y carecen por completo de contexto sobre repositorios Git, puertos de red, pruebas o dependencias de software.

**antOS** es un sistema operativo diseñado desde sus cimientos para el **desarrollador de software**. Transforma la máquina en un entorno donde:
1. **El repositorio y el proyecto son ciudadanos de primera clase:** El sistema comprende ramas activas, diffs en tiempo real, linters y suites de prueba en segundo plano.
2. **Orquestación Multi-Agente Nativa (`antFlow`):** Un equipo de roles especializados (*Arquitecto 📐, Coder 💻, QA 🧪, Auditor 🛡️*) opera localmente como demonios del sistema operativo sobre *Git Worktrees* efímeros y aislados.
3. **Cero Autoridad Ambiental y Recinto de Seguridad (*Sandboxing*):** La IA no ejecuta comandos ciegos en Bash. Planifica contra un catálogo de **capacidades tipadas**, calcula su **radio de impacto (*Blast Radius*)** antes de tocar el disco, opera bajo restricciones del kernel (*Landlock* en Linux / *Seatbelt* en macOS) y protege credenciales `.env`/SSH con **concesiones temporales explícitas (`grants`)**.
4. **Reversibilidad Nativa (`undo`):** Cada plan, commit o ciclo de desarrollo genera una instantánea atómica previa, permitiendo revertir cualquier cambio con `antos undo` o `antos undo --ticket <id>`.
5. **Superficie de Escritorio Moderna:** Shell Wayland GTK4 de latencia ultra-baja con barra de intenciones contextual y panel de control Kanban (`Super + A`).

---

## 🏗️ Arquitectura del Sistema

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                         SUPERFICIE DE ESCRITORIO                            │
 │  HUD de Intenciones · Tablero Kanban (Super + A) · Diffs · Insignias Git    │
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ IPC Tipado (antos-protocolo)
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                      MOTOR DE CONTEXTO Y MULTI-AGENTE                       │
 │  - Orquestador de Agentes antFlow (Arquitecto, Coder, QA, Auditor)          │
 │  - Spec Engine: Parser nativo de tickets Markdown (docs/tickets/)           │
 │  - Analizador de Repositorios Git con caché mtime & Worktrees Efímeros      │
 │  - Gestor de Servicios Efímeros (Postgres, Redis, MariaDB) & Puertos        │
 │  - Bóveda de Secretos (vault.json) & Sistema de Concesiones (grants)        │
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ Capacidades Tipadas & Sandboxing
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                   NÚCLEO DE EJECUCIÓN AISLADA (antosd)                      │
 │  Planificador IA (Local/Claude) · Cálculo Blast Radius · Landlock/Seatbelt  │
 │  Bitácora Inmutable (journal.jsonl) · Instantáneas & Reversión Atómica      │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### Estructura del Workspace (Crates de Rust)

* **[`system/protocolo`](system/protocolo):** Crate `antos-protocolo` con tipos puros de intercambio IPC, serializables con `serde`. Cero dependencias pesadas de I/O.
* **[`system/antosd`](system/antosd):** Demonio `antosd` y CLI `antos`. Contiene el planificador local/remoto, orquestador `antFlow`, analizador Git, cálculo de radio de impacto, recinto sandbox y motor de ejecución.
* **[`system/capabilities`](system/capabilities):** Manifiestos TOML tipados que definen contratos, parámetros, efectos y niveles de riesgo de cada capacidad del desarrollador.
* **[`system/barra`](system/barra):** Shell de escritorio Wayland / GTK4 Layer Shell con barra flotante de intenciones, insignias en vivo y centro de control Kanban.
* **[`kernel`](kernel):** Núcleo `no_std` en Rust para arranque en metal desnudo.
* **[`builder`](builder):** Ensamblador de imágenes de arranque.
* **[`docs/tickets`](docs/tickets):** Backlog y especificaciones técnicas maestro (*Spec-Driven Development*).

---

## 🚀 Guía de Inicio y Puesta en Marcha

### 1. Requisitos Previos y Herramientas

* **Rust Toolchain:** `rustc` y `cargo` (1.75 o superior).
* **Git:** 2.30 o superior.
* **QEMU / Podman / Docker (Opcional para VM y Kernel):** `qemu-system-x86_64`, `podman` o `docker`.
* **(Opcional para Claude):** Variable de entorno `ANTHROPIC_API_KEY` para planificación avanzada con LLM.
* **(Opcional para Desktop Wayland):** `gtk4` y `gtk4-layer-shell`.

### 2. Compilación del Workspace y Verificación

Compila todos los crates del workspace y verifica la suite de pruebas (46+ pruebas automatizadas):

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

antOS autodescubre el contexto, pero puedes personalizar su comportamiento con variables de entorno:

| Variable | Descripción | Valor por Defecto |
| :--- | :--- | :--- |
| `ANTOS_WORKSPACE` | Raíz del proyecto en el que opera antOS | Directorio actual (`pwd`) o raíz del repositorio Git |
| `ANTOS_STATE` | Directorio de estado (bitácora, servicios, bóveda de secretos) | `.antos/` en el workspace o `~/.local/state/antos/` |
| `ANTOS_CAPABILITIES` | Directorio con los manifiestos TOML de capacidades | `system/capabilities/` del repositorio |
| `ANTHROPIC_API_KEY` | Clave de API de Anthropic para el planificador Claude | `~/.config/antos/anthropic.key` o vacía (usa planificador `local`) |

---

## 🕹️ Modos de Ejecutar e Iniciar antOS

antOS está diseñado para probarse y ejecutarse en múltiples niveles según tu objetivo:

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                       4 FORMAS DE INICIAR antOS                             │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 1. CLI y Demonio Host (macOS / Linux): Ejecución directa en desarrollo      │
 │ 2. Escritorio Wayland (antos-barra): Shell GTK4 con HUD y Tablero Kanban    │
 │ 3. Contenedor Linux (Landlock Sandbox): Verificación de aislamiento kernel  │
 │ 4. Máquina Virtual NixOS (QEMU): Sistema operativo completo con servicios   │
 │ 5. Kernel Bare-Metal (QEMU): Núcleo no_std x86_64 arrancable desde BIOS     │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### Modo 1: CLI Directo y Demonio Host (macOS / Linux)

El modo principal para trabajar en tu día a día como desarrollador:

```bash
# Ejecutar un comando o intención directamente
target/debug/antos "crea un proyecto rust llamado demo"

# Iniciar el demonio en segundo plano escuchando en socket IPC (/tmp/antos.sock)
target/debug/antos escucha
```

### Modo 2: Interfaz Gráfica de Escritorio Wayland (`antos-barra`)

Lanza la superficie de escritorio nativa en Linux/Wayland:

```bash
# Lanzar la barra flotante de intenciones contextual
cargo run -p antos-barra --bin antos-barra

# Lanzar directamente el Centro de Misión y Tablero Kanban (Super + A)
cargo run -p antos-barra --bin antos-barra -- --panel
```

### Modo 3: Verificación Confinada en Linux con Landlock (`verificar-linux.sh`)

Permite validar las políticas de seguridad del kernel Linux (*Landlock LSM*) atacando el recinto desde un contenedor:

```bash
# Requiere Podman o Docker
./system/verificar-linux.sh
```

Este script prueba automáticamente:
* Compilación y pruebas de `antosd` en Linux.
* Diagnóstico de `antos doctor` contra el recinto.
* Bloqueo estricto de lectura a `/root/.ssh/id_rsa`.
* Ciclo de vida completo: intención ➡️ diff ➡️ ejecución ➡️ undo.

### Modo 4: Máquina Virtual antOS Completa en QEMU (`arrancar-vm.sh`)

Construye una imagen NixOS con antOS integrado como demonio de sistema y la arranca en QEMU:

```bash
# Construye la VM y arranca QEMU (Ctrl-a x para salir)
./system/arrancar-vm.sh
```

### Modo 5: Núcleo Bare-Metal x86_64 en QEMU (`run.sh`)

Compila el kernel `no_std` en Rust, genera la imagen de disco con `builder` y la arranca en QEMU:

```bash
# Compilar kernel bare-metal y arrancar en QEMU
./run.sh
```

---

## 🛠️ Guía Práctica de 0 a 100: Cómo Usar `antos`

Sigue este recorrido interactivo para probar todas las capacidades del sistema:

### Paso 1: Autodiagnóstico del Sistema (`doctor`)
Verifica que el recinto sandbox del kernel esté activo y confinando lecturas/escrituras:
```bash
antos doctor
```

### Paso 2: Explorar el Catálogo de Capacidades (`caps`)
Lista todas las herramientas tipadas registradas en el sistema con sus niveles de riesgo (*Auto*, *Confirmación*, *Concesión*):
```bash
antos caps
```

### Paso 3: Planificar sin Ejecutar (`-n` o `--dry-run`)
Observa el plan de ejecución y el radio de impacto antes de tocar el disco:
```bash
antos -n "crea un proyecto rust llamado api-service"
```

### Paso 4: Ejecutar una Intención con Confirmación
```bash
antos "crea un proyecto rust llamado api-service"
```
*antOS calculará el diff, presentará la previsualización interactiva y solicitará tu confirmación (`s/N`).*

### Paso 5: Diagnóstico y Liberación de Puertos
```bash
# Ver puertos de desarrollo ocupados y PIDs asociados
antos ports

# Liberar un puerto específico ocupado
antos "libera el puerto 3000"
```

### Paso 6: Aprovisionamiento de Servicios Locales Efímeros
```bash
# Levantar PostgreSQL local efímero
antos service up postgres

# Consultar servicios activos y variables inyectadas (.env)
antos services

# Detener el servicio cuando termines
antos service down postgres
```

### Paso 7: Bóveda de Secretos y Concesiones Temporales (Zero Environmental Authority)
```bash
# Guardar un secreto de forma segura
antos secret set GITHUB_TOKEN ghp_1122334455

# Consultar la bóveda (aparecerá protegido con requerimiento de concesión)
antos secrets

# Intentar leerlo (bloqueado por Zero Environmental Authority)
antos secret get GITHUB_TOKEN

# Conceder acceso temporal con motivo de auditoría
antos grant secret.GITHUB_TOKEN --minutos 10 --para "sincronizar releases"

# Leer el secreto concedido
antos secret get GITHUB_TOKEN

# Revocar el acceso
antos revoke secret.GITHUB_TOKEN
```

### Paso 8: Orquestación Multi-Agente antFlow y Tablero Kanban
```bash
# Ver el equipo de agentes especializados del sistema operativo
antos agents

# Despachar un ticket técnico al equipo (Arquitecto -> Coder -> QA -> Auditor) en worktree aislado
antos agent run T1.1

# Ver el estado del ciclo de vida del ticket
antos agent status T1.1

# Visualizar el tablero Kanban completo de tickets
antos panel
```

### Paso 9: Consultar la Bitácora Transaccional (`log`)
Revisa el historial inmutable de intenciones, planes y snapshots generados:
```bash
antos log
```

### Paso 10: Reversión Atómica Instantánea (`undo`)
```bash
# Revertir la última acción ejecutada
antos undo

# O revertir todos los cambios asociados a un ticket específico
antos undo --ticket T1.1
```

---

## 💻 Referencia Rápida de Comandos CLI `antos`

### 2. Git Semántico e Introspección

```bash
# Crear commits convencionales automáticos analizando el contexto
antos "haz commit con los cambios de autenticación"

# Crear o cambiar de rama
antos "crea rama feature/login-oauth"
```

### 3. Diagnóstico y Liberación de Puertos de Red

```bash
# Diagnosticar puertos de desarrollo en escucha y procesos asociados
antos ports

# Diagnosticar o liberar un puerto específico en colisión
antos ports 3000
antos "libera el puerto 3000"
```

### 4. Servicios Locales Efímeros (Bases de Datos / Nix)

Aprovisiona servicios auxiliares aislados en `$STATE/services/<servicio>` con inyección automática de variables de conexión (`DATABASE_URL`, `REDIS_URL`) en el `.env` del workspace:

```bash
# Levantar PostgreSQL local efímero
antos service up postgres

# Levantar Redis en puerto específico
antos service up redis 6379

# Consultar tabla de servicios activos y variables de conexión
antos services

# Detener y limpiar un servicio
antos service down postgres
```

### 5. Bóveda de Secretos y Cero Autoridad Ambiental (Zero Environmental Authority)

Las credenciales (`.env*`, claves SSH, tokens de API) están blindadas a nivel de kernel y denegadas por defecto a sub-agentes y procesos no autorizados:

```bash
# Almacenar un secreto de forma segura en la bóveda ($STATE/vault.json con permisos 0600)
antos secret set STRIPE_API_KEY sk_live_9988776655

# Listar secretos y estado de concesiones
antos secrets

# Intentar leer un secreto (bloqueado por Zero Environmental Authority si no hay concesión)
antos secret get STRIPE_API_KEY

# Otorgar una concesión explícita temporal con motivo de auditoría
antos grant secret.STRIPE_API_KEY --minutos 10 --para "despliegue en staging"

# Leer el secreto concedido
antos secret get STRIPE_API_KEY

# Revocar la concesión inmediatamente
antos revoke secret.STRIPE_API_KEY
```

### 6. Orquestador Multi-Agente (`antFlow`)

Coordina el ciclo de vida de desarrollo delegando tareas a roles especializados en *Git Worktrees* aislados en segundo plano:

```bash
# Ver los roles especializados del sistema operativo y sus directivas
antos agents

# Ejecutar un ticket técnico con la cadena Arquitecto 📐 -> Coder 💻 -> QA 🧪 -> Auditor 🛡️
antos agent run T1.1

# Consultar estado, rol activo y traza de ejecución de los agentes
antos agent status T1.1
```

### 7. Centro de Control y Tablero Kanban (`antos panel`)

Visualiza el estado del backlog de tickets en 4 columnas (*Backlog, En Progreso, Revisión, Completado*) y el monitor de agentes en vivo:

```bash
# Mostrar el tablero Kanban y monitor de agentes en terminal
antos panel

# Despachar un ticket directamente al equipo de agentes desde el tablero
antos panel --dispatch T1.2
```

### 8. Reversión Granular e Instantánea (`antos undo`)

```bash
# Revertir el último plan ejecutado restaurando la instantánea atómica
antos undo

# Revertir todos los commits, cambios y transacciones originados por un ticket
antos undo --ticket T2.1
```

---

## 🖥️ Interfaz de Escritorio Wayland (`antos-barra`)

antOS incluye un shell de escritorio nativo acelerado por GPU construido sobre Wayland Layer Shell y GTK4:

```bash
# Lanzar la barra de intenciones contextual
cargo run -p antos-barra --bin antos-barra

# Lanzar directamente el Centro de Misión y Tablero Kanban
cargo run -p antos-barra --bin antos-barra -- --panel
```

### Atajos Globales de Teclado
* **`Super + Espacio`:** Abre el HUD flotante de intenciones rápidas con insignias de Git en vivo y diffs sintácticos coloreados.
* **`Super + A`:** Despliega el Centro de Misión con el monitor de roles activos `antFlow` y el Tablero Kanban de tickets sincronizado con `docs/tickets/`.
* **`Escape`:** Cierra la ventana activa.

---

## 📋 Catálogo de Capacidades del Sistema (`system/capabilities/`)

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
| `project.scaffold` | Creación y andamiaje inicial de proyectos (Rust, TS, Python) | `confirm` |
| `pkg.declare` | Declaración de dependencias en manifiestos de proyecto | `confirm` |
| `fs.write` / `fs.delete` | Modificación y eliminación controlada de archivos con instantánea | `confirm` / `grant` |
| `system.declare` | Modificación de la configuración declarativa del sistema | `grant` |

---

## 🗺️ Hoja de Ruta y Backlog de Desarrollo

El proyecto se desarrolla bajo la metodología **Spec-Driven Development**, donde cada hito se desglosa en especificaciones técnicas formales dentro de [`docs/tickets/`](docs/tickets/).

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

---

## 🛡️ Seguridad y Filosofía de Privacidad

* **Cero Filtración de Memoria o Variables Ambientales:** Los subprocesos y agentes nacen limpios; las claves de API no se heredan en variables de entorno globales.
* **Aislamiento por Hardware y Kernel:** Si una capacidad no declara que escribe en una ruta o contacta un dominio de red, el kernel deniega la operación devolviendo `EPERM`.
* **Privacidad Local:** Whisper corre localmente para voz; los analizadores semánticos y el motor de tickets operan 100% en local sin enviar código fuente a la nube a menos que el usuario active explícitamente un planificador externo.

---

## 📜 Licencia

antOS se distribuye bajo la licencia MIT. Consulta el archivo `LICENSE` para más información.
