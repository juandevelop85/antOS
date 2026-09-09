# antOS · Objetivo Funcional
## El Sistema Operativo para Desarrolladores Impulsado por IA

> **Versión del Documento:** 1.1  
> **Fecha:** Septiembre 2026  
> **Estado:** Aprobado para desarrollo  

---

## 1. Visión General y Filosofía

Los sistemas operativos convencionales (macOS, Windows, distribuciones Linux estándar) fueron diseñados en las décadas de 1970 a 1990 bajo dos premisas que hoy resultan obsoletas para un programador moderno:

1. **El sistema solo entiende "archivos y flujos de bytes":** Un directorio con código fuente es tratado exactamente igual que una carpeta de descargas o de música. El sistema operativo desconoce qué es un repositorio Git, qué dependencias utiliza, qué puertos necesita para correr o si sus pruebas unitarias están pasando.
2. **Autoridad ambiental total y comandos en texto plano:** Cuando un agente de IA o un script de terminal se ejecuta, hereda todos los permisos del usuario (pudiendo leer claves SSH o borrar archivos críticos) y se comunica a través de cadenas de texto desestructuradas y propensas a errores de interpretación.
3. **Herramientas de IA desconectadas del sistema:** Los entornos de desarrollo y orquestadores de agentes (como tableros web, chats o extensiones de navegador) viven en islas separadas, sin acceso de bajo nivel al hardware, sin capacidad de crear recintos de seguridad eficientes y consumiendo recursos excesivos.

### La Misión de antOS

**antOS** es un sistema operativo personal diseñado desde sus cimientos para el **desarrollador de software**. Transforma la máquina en un entorno donde:
* **El Proyecto / Repositorio es un ciudadano de primera clase:** El sistema operativo entiende intrínsecamente el contexto de tus proyectos (árbol de trabajo Git, estado de compilación, linters, contenedores y servicios asociados).
* **Orquestación Multi-Agente Nativa (Integración antFlow):** El ciclo de vida del software (especificación, arquitectura, desarrollo, pruebas, auditoría) es coordinado por un equipo de agentes especializados que corren como demonios del sistema operativo, con espacios de trabajo efímeros (*Git Worktrees*) y sin depender de plataformas web externas.
* **La IA opera con capacidades tipadas y aisladas:** La inteligencia artificial no ejecuta comandos ciegos en Bash; genera planes con herramientas tipadas, calcula su radio de impacto antes de tocar el disco, opera en *sandboxes* estrictos (Landlock / Seatbelt) y permite **deshacer cualquier cambio (`undo`) como una primitiva nativa del sistema**.
* **La interfaz de usuario es contextual y proactiva:** Las ventanas, barras de herramientas y terminales exponen directamente la salud del código, los diffs en tiempo real, el estado del flujo de agentes y atajos de intención por teclado o voz.

---

## 2. Pilares Funcionales

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                         SUPERFICIE DE ESCRITORIO                            │
 │  HUD de Intenciones · Centro de Agentes (antFlow UI) · Diffs · Ventanas Git │
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ IPC Tipado (antos-protocolo)
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                      MOTOR DE CONTEXTO Y MULTI-AGENTE                       │
 │  - Orquestador de Agentes (Arquitecto, Coder, QA, Auditor)                  │
 │  - Gestor de Tickets & Especificaciones (Spec-Driven Engine)                │
 │  - Introspección Git & Worktrees Efímeros                                   │
 │  - Runtimes, Bases de Datos & Detección de Puertos (Nix)                    │
 └──────────────────────────────────────┬──────────────────────────────────────┘
                                        │ Capacidades Tipadas & Sandboxing
 ┌──────────────────────────────────────▼──────────────────────────────────────┐
 │                   NÚCLEO DE EJECUCIÓN AISLADA (antosd)                      │
 │  Planificador IA · Cálculo de Radio de Impacto · Landlock · Bitácora & Undo │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### Pilar I: Orquestación Multi-Agente Nativa (antFlow OS Core)
El sistema operativo sustituye las aplicaciones web de agentes por un subsistema nativo de alta eficiencia:
* **Roles Especializados del Ciclo de Vida:**
  * **Agente Arquitecto / Planificador:** Desglosa especificaciones y tickets en planes de implementación estructurados.
  * **Agente Desarrollador (Coder):** Ejecuta modificaciones de código respetando las guías de estilo del repositorio.
  * **Agente QA / Tester:** Diseña y ejecuta suites de pruebas de regresión, fuzzing y análisis estático.
  * **Agente de Seguridad y Auditoría:** Verifica que no existan vulnerabilidades en dependencias, fugas de secretos ni operaciones no permitidas.
* **Espacios de Trabajo Aislados (*Git Worktrees* automáticos):** Cuando un agente toma una tarea, el sistema genera automáticamente un *worktree* efímero. El agente trabaja, compila y prueba en paralelo sin interferir con tu código activo. Al finalizar con éxito, el sistema te presenta una notificación nativa con el diff consolidado listo para aprobación o merge.

### Pilar II: Desarrollo Basado en Especificaciones y Tickets (*Spec-Driven Development*)
* **Gestor de Flujo Nativo:** El sistema operativo indexa automáticamente las especificaciones y tickets Markdown del proyecto (ej. `docs/tickets/` o `specs/`).
* **Tablero de Estado Integrado:** Visualización nativa del estado de los tickets (*Backlog*, *En Progreso*, *Revisión*, *Completado*) accesible por atajo global (`Super + A`) sin necesidad de Jira, Trello o herramientas web pesadas.
* **Trazabilidad Absoluta:** Cada diff, commit y registro de bitácora queda vinculado al ticket que lo originó, permitiendo revertir funcionalidades completas con `antos undo --ticket <id>`.

### Pilar III: Conciencia Nativa de Repositorios (Git-Aware Workspace)
El sistema operativo monitoriza continuamente los directorios declarados como `$WORKSPACE`:
* **Metadatos en tiempo real en la interfaz:** Al navegar o enfocar cualquier ventana o terminal en un directorio de proyecto, el entorno refleja instantáneamente:
  * Rama activa y estado de sincronización con el remoto (commits por delante / por detrás).
  * Lista de archivos modificados, en *staging* o no rastreados con sus correspondientes diffs sintácticos.
  * Estado de la suite de pruebas y linters en segundo plano.
* **Contexto inyectado a la IA:** Cuando el desarrollador expresa una intención (ej. *"haz commit de lo que arreglé en la autenticación"*), la IA recibe directamente el grafo semántico del repositorio y los diffs concretos, eliminando la necesidad de comandos exploratorios manuales.

### Pilar IV: Motor de Intenciones Tipadas y Seguras
La interacción humano-IA en el sistema operativo se rige por un contrato estricto:
1. **Captura:** Entrada de texto mediante un HUD global (*shortcut* configurable) o comandos de voz locales (Whisper).
2. **Planificación Tipada:** El modelo de lenguaje (Claude u orquestadores locales) traduce la intención en una secuencia ordenada de capacidades registradas en el catálogo del sistema.
3. **Cálculo de Radio de Impacto (*Blast Radius*):** El sistema determina con precisión qué rutas se escribirán, qué puertos o dominios de red se contactarán y qué nivel de permiso requiere la acción (*Auto*, *Confirmación*, *Concesión Temporal*).
4. **Previsualización de Diffs:** Si la acción modifica código o configuración del sistema, se presenta un diff limpio e interactivo antes de ejecutar.
5. **Ejecución en Recinto (*Sandbox*):** Cada paso se ejecuta bajo restricciones estrictas del kernel (Landlock en Linux / Seatbelt en macOS), bloqueando cualquier acceso fuera de lo declarado.
6. **Bitácora y Reversibilidad:** Todos los cambios son registrados en una bitácora transaccional, permitiendo revertir la acción al instante con `antos undo`.

### Pilar V: Entornos Declarativos, Servicios y Diagnóstico de Puertos
* **Cero instalación global contaminante:** El sistema no ensucia `/usr` ni el home del usuario con múltiples versiones globales de Node, Python, Rust o bases de datos.
* **Servicios efímeros locales:** Si un proyecto requiere PostgreSQL, Redis o un mock server, el sistema los provisiona y aísla bajo demanda a través de módulos declarativos (Nix / contenedores) asociados al ciclo de vida del workspace.
* **Monitor Inteligente de Puertos y Procesos:** Detección proactiva de colisiones (ej. *"El puerto 3000 está ocupado por un proceso huérfano"*), permitiendo liberar o reasignar puertos con un solo atajo.

### Pilar VI: Gestión Segura de Secretos (Zero Environmental Authority)
* **Protección de archivos `.env`, tokens y claves SSH:** Los archivos que contienen credenciales sensibles no son legibles por defecto para agentes o scripts de terceros.
* **Concesiones explícitas (`grants`):** Si una capacidad requiere una clave de API para desplegar o un token para hacer *push*, el sistema solicita una concesión temporal de acceso que expira automáticamente tras un tiempo configurado.

---

## 3. Catálogo de Capacidades del Desarrollador (Core Capabilities)

El sistema expone sus operaciones mediante manifiestos tipados en `system/capabilities/`:

| Dominio | Capacidad | Descripción | Nivel de Riesgo |
| :--- | :--- | :--- | :--- |
| **Agentes** | `agent.spawn_task` | Despacha una tarea a un rol específico en un worktree efímero. | *Auto* |
| **Agentes** | `agent.review_diff` | El agente auditor evalúa el diff generado antes de fusionar. | *Auto (Lectura)* |
| **Tickets** | `spec.create_ticket` | Crea un ticket estructurado a partir de una intención o bug report. | *Confirmación* |
| **Tickets** | `spec.list_tickets` | Lista tickets y su estado de avance actual. | *Auto (Lectura)* |
| **Git** | `git.status` | Obtiene el estado estructurado del repo (ramas, archivos sucios, diffs). | *Auto (Lectura)* |
| **Git** | `git.commit_semantic` | Genera un commit convencional analizando los cambios en staging. | *Confirmación* |
| **Git** | `git.smart_branch` | Crea o cambia de rama vinculando contexto o tickets. | *Confirmación* |
| **Git** | `git.worktree_create` | Crea un entorno de trabajo paralelo y aislado para un agente. | *Confirmación* |
| **Workspace** | `project.scaffold` | Inicializa un proyecto con su toolchain, linter y tests listos. | *Confirmación* |
| **Workspace** | `project.inspect` | Identifica stack tecnológico, dependencias y scripts de compilación. | *Auto (Lectura)* |
| **Workspace** | `fs.write_diff` | Modifica archivos aplicando diffs validados contra el original. | *Confirmación* |
| **Workspace** | `fs.delete` | Elimina rutas de forma segura (con copia en la instantánea). | *Concesión* |
| **Servicios** | `env.service_up` | Levanta un servicio auxiliar (Postgres, Redis) para el proyecto. | *Confirmación* |
| **Diagnóstico** | `diag.port_status` | Lista puertos ocupados y procesos vinculados a cada workspace. | *Auto (Lectura)* |
| **Diagnóstico** | `test.diagnose` | Ejecuta la suite de pruebas e identifica la causa raíz de fallos. | *Auto (Lectura)* |
| **Sistema** | `system.declare` | Modifica la configuración declarativa del SO (paquetes Nix/sistema). | *Concesión* |

---

## 4. Experiencia de Usuario (UI & Desktop Shell)

### 4.1. HUD de Intención Rápida (`antos barra`)
* Una superficie flotante y minimalista accesible mediante atajo global (ej. `Super + Espacio`).
* Permite redactar o dictar una orden en lenguaje natural.
* Muestra el plan resultante, el diff de código a aplicar y el botón/tecla de confirmación en una sola vista cohesiva.

### 4.2. Centro de Control de Agentes y Tickets (antFlow Desktop Panel)
* Panel lateral o vista dedicada (`Super + A`) que muestra:
  * El grafo de agentes trabajando activamente en segundo plano.
  * El tablero visual de tickets del proyecto actual.
  * Notificaciones de tareas completadas con botón de previsualización de diff y *Merge a rama principal*.

### 4.3. Ventanas y Explorador Git-Aware
* Los marcos de ventana y el explorador de archivos integran insignias de estado:
  * Verde/Rojo: Estado de la suite de pruebas local.
  * Icono de rama Git con contador de cambios pendientes (`+12 -3`).
  * Al hacer clic en un archivo marcado como modificado, despliega el visor de diff instantáneo sin necesidad de abrir un IDE pesado.

### 4.4. Notificaciones Proactivas y Monitor de Salud
* Alertas contextuales no invasivas:
  * *"El Agente Coder ha terminado el Ticket T1.3 en worktree aislado; los tests pasaron al 100%. ¿Deseas revisar el diff?"*
  * *"El puerto 3000 fue liberado tras detener el servidor de desarrollo."*
  * *"La rama actual está 5 commits por detrás de main; ¿quieres rebasear?"*

---

## 5. Requerimientos No Funcionales

1. **Rendimiento Nativo:** La orquestación de agentes y el motor de contexto corren en Rust con huella de memoria inferior a 60 MB en reposo (frente a los cientos de MB de las plataformas web basadas en Electron/Node).
2. **Seguridad y Aislamiento Estricto:** Ningún agente puede escribir fuera de su *worktree* concedido ni acceder a la red salvo que esté explícitamente declarado en los efectos de la capacidad.
3. **Privacidad Total:** Transcripción de voz (Whisper) y análisis de código 100% locales por defecto; soporte de LLMs locales (Ollama/Llama.cpp) además de APIs en la nube.
4. **Resiliencia y Rollback:** Reversibilidad granular por intención, por ticket o por sesión mediante la bitácora inmutable.

---

## 6. Fases de Trabajo e Implementación

El plan de trabajo original se estructuró en los siguientes cinco paquetes fundacionales de desarrollo:

* **Fase 1: Motor de Contexto Git & Spec Engine** (Protocolo IPC, analizador de Git y parser de tickets en `antosd`).
* **Fase 2: Expansión del Catálogo de Capacidades** (Capacidades de Git, Worktrees, Tests y Diagnóstico de Puertos).
* **Fase 3: Orquestación Multi-Agente Nativa (antFlow Core en Rust)** (Roles de agentes, colas de ejecución y worktrees efímeros).
* **Fase 4: Superficie de Usuario y Centro de Agentes** (Evolución de `system/barra` y panel de control Wayland/GTK4).
* **Fase 5: Servicios Declarativos & Bóveda de Secretos** (Nix services y grants granulares para credenciales).

El plan se extendió sustancialmente más allá de esta visión fundacional: antOS ya no solo orquesta el
desarrollo *sobre* macOS/Linux, sino que — a partir de la **Fase 18** (portado del kernel a AArch64) y,
sobre todo, la **Fase 26** — es capaz de arrancar como **sistema operativo bare-metal soberano**, con su
propio driver gráfico, compositor de escritorio nativo y un runtime/shell de espacio de usuario
(`libantos` / `antos-init`) sin dependencia alguna de `glibc`/`musl` ni de un SO anfitrión. Las
**Fases 27-28** añaden arranque Limine real por UEFI, pila PCIe/USB xHCI y HID bare-metal, y
periféricos nativos (VirtIO-Input, GIC v2/v3, `virtio-gpu-pci`/`ramfb`, descubrimiento por DTB/ACPI)
a paridad entre x86_64 y AArch64; el kernel arranca hasta el shell interactivo en **QEMU**,
**VirtualBox ARM64** y **UTM**.

A partir de aquí el desarrollo se bifurca en **dos vías**: la **Fase 29** endurece el shell
del **kernel bare-metal** (framework de comandos, ejecución de ELF, coreutils `no_std`,
edición de línea) como vía de I+D de soberanía; y la **Fase 30** entrega el **escritorio
antOS Linux** —imagen NixOS con sesión Wayland, `antos-barra` y userland de desarrollo
(Neovim, Git, `antos dev`)— como sistema de uso diario. El backlog íntegro y actualizado
(28 fases completadas + Fases 29-30 planificadas, 107 tickets) vive en
[`docs/tickets/README.md`](tickets/README.md);
el resumen navegable por fase está en el [`README.md`](../README.md) raíz del repositorio, y el registro
cronológico de lo entregado en cada fase, en [`CHANGELOG.md`](../CHANGELOG.md).
