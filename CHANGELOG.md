# Registro de Cambios de antOS (CHANGELOG)

Todos los cambios notables de este proyecto están documentados en este archivo.
El formato se basa en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/) y este proyecto se adhiere a [Semantic Versioning](https://semver.org/lang/es/).

---

## [0.1.0] - 2026-09-02

¡Primer lanzamiento oficial de **antOS**: el sistema operativo personal para desarrolladores impulsado por IA y orquestación multi-agente nativa!

Esta versión culmina las **14 Fases** de desarrollo del plan maestro con **34 tickets técnicos completados al 100%**, más de 129 pruebas automatizadas pasando en verde y soporte multi-plataforma tanto en Linux (con confinamiento estricto por Landlock / eBPF) como en macOS (con Seatbelt).

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

#### 🔹 Fase 10: VFS Semántico (`antfs`), Guardián de Código y Grafo de Contexto
- **[T10.1] Sistema de Archivos Virtual por Símbolos (`antfs`):** Proyección semántica de ficheros basada en funciones, structs, traits y tickets técnicos.
- **[T10.2] Guardián de Código (VFS Guard):** Intercepción en tiempo real de escrituras con errores de sintaxis (llaves huérfanas, JSON inválido) previniendo corrupción del código fuente en disco.
- **[T10.3] Grafo de Memoria Contextual:** Indexación vectorial con búsqueda por similitud de cosenos para enriquecer el contexto provisto a los agentes de IA.

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

### 📦 Artefactos de Distribución v0.1.0
- `antos-v0.1.0-<os>-<arch>.tar.gz`: Binarios de antOS (`antos`, `antosd`, `builder`), capacidades declarativas y documentación.
- `antos-live-x86_64.iso`: Imagen de disco arrancable híbrida para pruebas en bare metal o máquinas virtuales.
- `SHA256SUMS`: Sumas de comprobación criptográficas para verificación de integridad.
