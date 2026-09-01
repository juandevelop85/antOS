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

## 🚀 Guía de Inicio Rápido

### 1. Requisitos Previos

* **Rust Toolchain:** `rustc` y `cargo` (1.75 o superior).
* **Git:** 2.30 o superior.
* **(Opcional para Claude):** Variable de entorno `ANTHROPIC_API_KEY` para planificación avanzada con LLM.
* **(Opcional para Desktop Wayland):** `gtk4` y `gtk4-layer-shell`.

### 2. Compilación y Suite de Pruebas

Compila el workspace completo y verifica que todas las pruebas unitarias y de integración pasen en verde:

```bash
# Compilar todos los crates del workspace
cargo build --workspace

# Ejecutar la suite completa de pruebas (46+ tests)
cargo test --workspace
```

---

## 💻 Uso del CLI `antos`

El ejecutable principal se encuentra en `target/debug/antos` (o `cargo run --bin antos -- <comando>`).

### 1. Intenciones en Lenguaje Natural

Expresa lo que necesitas; antOS planifica la secuencia de capacidades, calcula el radio de impacto, muestra el diff y solicita confirmación:

```bash
# Crear un nuevo proyecto con toolchain y tests configurados
antos "crea un proyecto rust llamado api-service"

# Planificar y previsualizar en seco (dry-run) sin aplicar cambios
antos -n "declara serde en el proyecto api-service"

# Aprobar automáticamente la ejecución
antos -s "escribe en fichero src/main.rs: fn main() { println!(\"Hola antOS\"); }"
```

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
