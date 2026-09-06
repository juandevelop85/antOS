# Arquitectura de antOS

Este documento contiene los diagramas vivos de arquitectura sincronizados continuamente.

<!-- ANTOS_ARCH_START -->
> 📐 **antOS Living Architecture (T21.3)** · Generado automáticamente a partir del código fuente.
> *Crates: 6 | Módulos Demonio: 55 | Capacidades: 113*

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

  subgraph DAEMON["🐜 Demonio del Sistema (system/antosd - 55 Módulos)"]
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

---

## 4. Arquitectura de Despliegue en Hardware Real (Bare Metal) y Almacenamiento Físico (Fase 24)

A partir de la **Fase 24**, antOS incorpora una cadena de arranque y despliegue completa para hardware físico bare-metal (x86_64 y AArch64):

```mermaid
graph TD
  subgraph BOOT["1. Arranque Híbrido (T24.1)"]
    LimineUEFI["Limine UEFI (BOOTX64.EFI / BOOTAA64.EFI)"]
    LimineBIOS["Limine BIOS MBR (Stage 1 / Stage 2)"]
    LimineCfg["limine.cfg (Protocolo Limine)"]
  end

  subgraph RAM["2. Entorno Live en RAM (T24.2)"]
    KernelELF["Kernel no_std (HAL unificado)"]
    Initramfs["Initramfs / TarFS en Memoria RAM"]
    BaseBinaries["Binarios del Sistema (antos, shell, utilidades)"]
  end

  subgraph STORAGE["3. Drivers de Almacenamiento Físico (T24.3)"]
    AHCI["Driver AHCI / SATA (Controlador PCI, Puertos SATA 3.0)"]
    NVMe["Driver NVMe PCIe (Admin Submission/Completion Queues, 4K)"]
    BlockDev["Interfaz Genérica de Dispositivos de Bloque"]
  end

  subgraph DEPLOY["4. Instalación Guiada y Despliegue (T24.4 / T24.5)"]
    Installer["Asistente CLI antos install (Limpio o Dual Boot)"]
    Partitioner["Particionado GPT & Creación ESP FAT32 / Ext4"]
    DeployEngine["Copia de Sistema Base & Generación de /etc/fstab"]
    BootloaderEngine["Inscripción NVRAM UEFI (efibootmgr)"]
  end

  BOOT --> RAM
  RAM --> STORAGE
  STORAGE --> DEPLOY
```

### Componentes de la Fase 24:
1. **Bootloader Híbrido Limine (`builder::limine`):**
   - Integración nativa en el generador de imágenes `builder`.
   - Soporte para arranque dual: UEFI (GPT + ESP FAT32) y BIOS Legacy (MBR boot sectors).
   - Generación de imágenes ISO híbridas (`.iso`) e imágenes de disco crudo (`.img`).
2. **Empaquetador de Ramdisk Live (`builder::ramdisk`):**
   - Empaquetado del sistema de ficheros en memoria `initramfs` (formato TarFS compatible).
   - Carga en RAM durante el boot para permitir una sesión Live interactiva sin requerir almacenamiento persistente previo.
3. **Drivers de Almacenamiento Físico en Kernel (`kernel::storage`):**
   - **AHCI / SATA:** Detección de controladoras PCI clase almacenamiento, inicialización de puertos HBA y comandos FIS de lectura/escritura DMA.
   - **NVMe PCIe:** Descubrimiento de controladores NVMe sobre PCIe, mapeo de registros BAR0, creación de colas de envío y finalización (Admin Queue Pairs) y lectura/escritura de sectores LBA a alta velocidad.
4. **Instalador Guiado e Interactivo (`system/antosd/src/installer`):**
   - **`antos install`:** Asistente interactivo por pasos o desatendido por archivo TOML (`--config`).
   - Gestión de particiones GPT, formateo limpio o convivencia Dual Boot con Windows/Linux.
   - Copia de sistema base con telemetría visual, configuración de `/etc/fstab` y registro en firmware con `efibootmgr`.
5. **Generador y Grabador Seguro de Live USB (`system/antosd/src/installer/usb.rs`):**
   - **`antos usb build`:** Construcción automatizada de la ISO y generación de hash SHA-256.
   - **`antos usb flash`:** Detección segura de medios extraíbles, bloqueo automático de discos internos del sistema anfitrión, volcado bit a bit con barra de progreso y verificación criptográfica.

