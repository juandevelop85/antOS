# Arquitectura de antOS

Este documento contiene los diagramas vivos de arquitectura sincronizados continuamente.

<!-- ANTOS_ARCH_START -->
> 📐 **antOS Living Architecture (T21.3)** · Generado automáticamente a partir del código fuente.
> *Crates: 6 | Módulos Demonio: 53 | Capacidades: 113*

### 1. Topología de Componentes y Límites de Seguridad

```mermaid
graph TD
  subgraph UI["🖥️ Shell & Escritorio"]
    Barra["system/barra (Wayland GTK4 Shell)"]
    VTE["Consola Terminal VTE & HUD"]
    Kanban["Panel Kanban de Agentes (Super + A)"]
    DiffViewer["Visor de Diffs Sintácticos"]
  end

  subgraph PROTO["⚡ Protocolo IPC Tipado"]
    Protocolo["system/protocolo (97 Peticiones, 91 Eventos)"]
  end

  subgraph DAEMON["🐜 Demonio del Sistema (system/antosd - 53 Módulos)"]
    subgraph MultiAgente["Orquestación antFlow"]
      Architect["Arquitecto (Specs & Tickets)"]
      Coder["Coder (Worktree Patch)"]
      QA["QA (TDD & Regresiones)"]
      Auditor["Auditor (Certificación & Perf Diff)"]
    end

    subgraph Seguridad["Aislamiento & Blindaje"]
      Seatbelt["Seatbelt / Landlock LSM"]
      Vault["Bóveda de Secretos & Grants"]
      Quota["Watchdog & Cuotas de Memoria"]
    end

    subgraph Motores["Subsistemas de Desarrollo"]
      TimeMachine["Time Machine (Snapshots Atómicos)"]
      BenchEngine["Benchmarking Continuo (Perf Diff)"]
      ForgeEngine["Sincronización Git Forge (GitHub/GitLab)"]
      DocArch["Documentación Viva & Mermaid"]
    end
  end

  subgraph CAPS["📋 Catálogo de Capacidades Declarativas"]
    Capabilities["system/capabilities (113 capacidades: 70 auto, 39 confirm, 4 grant)"]
  end

  subgraph BAREMETAL["⚙️ Núcleo & Arranque Bare-Metal"]
    Kernel["kernel (Rust no_std Bare-Metal)"]
    Builder["builder (Generador de Imágenes UEFI)"]
  end

  UI -->|Unix Stream IPC| Protocolo
  Protocolo -->|Despacho Serde| DAEMON
  DAEMON -->|Radio de Impacto| CAPS
  DAEMON -->|Ejecución Enjaulada| Seguridad
  DAEMON -->|Syscalls / Init| BAREMETAL
```

### 2. Flujo de Datos IPC y Ciclo de Ejecución de Intenciones

```mermaid
sequenceDiagram
  autonumber
  actor User as Desarrollador / UI (Barra)
  participant IPC as Socket IPC Unix (antos-protocolo)
  participant Daemon as Demonio antosd
  participant Planner as Planificador IA (Local/Ollama)
  participant Blast as Blast Radius & Cuotas
  participant Sandbox as Recinto Seatbelt/Landlock
  participant TM as Time Machine / Journal

  User->>IPC: Request::Intent { text }
  IPC->>Daemon: Despacho asíncrono serializado
  Daemon->>Planner: plan(text, catalog)
  Planner-->>Daemon: Propuesta de Pasos y Capacidades
  Daemon->>Blast: Evaluar radio de impacto (Auto/Confirm/Grant)
  Daemon->>TM: Instantánea atómica pre-ejecución
  Daemon->>Sandbox: Iniciar proceso enjaulado con cuota estricta
  Sandbox-->>Daemon: Resultado verificado y salida
  Daemon->>TM: Registrar en bitácora inmutable (journal.jsonl)
  Daemon-->>IPC: Event::Done / Event::Milestone
  IPC-->>User: Actualización reactiva en HUD/Barra
```

### 3. Ciclo de Vida Multi-Agente antFlow

```mermaid
stateDiagram-v2
  [*] --> IssueImportado: antos issue import / docs/tickets/
  IssueImportado --> Arquitecto: Análisis y Criterios de Aceptación
  Arquitecto --> Coder: Despacho a Worktree Efímero Aislado
  Coder --> Tester: Código implementado (AST validado por VFS Guard)
  Tester --> Auditor: TDD Green (0 Panics, Tests 100% pasando)
  Auditor --> BenchDiff: Evaluación de Rendimiento Continuo
  BenchDiff --> Coder: Regresión Detectada (>15% latencia o RSS)
  BenchDiff --> PullRequest: Rendimiento y Calidad Certificados
  PullRequest --> [*]: antos pr create / Publicado en GitHub o GitLab
```
<!-- ANTOS_ARCH_END -->
