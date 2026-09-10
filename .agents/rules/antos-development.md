---
trigger: always_on
---

# Reglas de Desarrollo de antOS

## 1. Arquitectura y Convenciones de Código (Rust)
- **Crates del Workspace:**
  - `system/protocolo` (`antos-protocolo`): Definición de tipos de datos puros, IPC y mensajes serializables con `serde`. No debe contener lógica de I/O pesada ni dependencias del demonio.
  - `system/antosd` (`antosd`): Demonio del sistema operativo encargado de servicios de fondo, despacho IPC, sandbox y coordinación de capacidades.
  - `system/capabilities`: Módulos de capacidades tipadas (Git semántico, Worktrees, puertos, nix services, secretos).
  - `kernel`: Crate `no_std` para bare metal. No mezclar dependencias de `std` con el kernel.
- **Calidad y Robustez:**
  - **Nomenclatura en Inglés:** Todos los identificadores (funciones, variables, structs, enums, métodos, traits y módulos) deben escribirse exclusivamente en inglés (ej. `FlowEngine`, `start_task`, `diagnose_ports`).
  - **Comentarios y documentación en español (T31.12):** la regla de arriba es
    sobre *identificadores*, no sobre prosa. Los comentarios, los doc-comments
    (`///`, `//!`) y el texto que el sistema le muestra al usuario (mensajes
    de error, salida de CLI, contenido de tickets) siguen en español,
    deliberadamente — es una de las señas de identidad del repositorio: el
    código explica *por qué* está hecho cada cosa en el idioma en el que se
    piensa el proyecto. Un método puede llamarse `name_es()` y devolver
    `"Arquitecto"`: el identificador va en inglés, el contenido que produce
    no tiene por qué. Ningún futuro ticket de nomenclatura debe traducir
    comentarios o strings de cara al usuario creyendo que así completa este
    tipo de trabajo — eso sería deshacer una decisión de proyecto, no
    aplicar la regla.
  - Cero `unwrap()` o `expect()` en rutas de ejecución de IPC o demonio; utilizar siempre propagación de errores (`?`) o manejo explícito.
  - Documentar structs y mensajes públicos expuestos a través del protocolo IPC.
  - **`sh -c` / `bash -c` (T31.4):** solo se usa cuando el intérprete de comandos
    es la funcionalidad que se pide (p. ej. `antos vm exec`, una etapa de CI
    declarada por el usuario) — nunca para tareas que no lo necesitan (buscar
    un binario en `PATH`, comprobar una versión, etc., que deben resolverse
    sin invocar un intérprete). Y aun cuando el intérprete es la
    funcionalidad buscada, la cadena de comando nunca se construye
    interpolando un valor que proceda de configuración externa (un fichero de
    perfil, un argumento de CLI, un campo de protocolo) sin pasar antes por
    una validación explícita de caracteres permitidos; y la ejecución debe
    pasar por el recinto de `sandbox::` siempre que sea razonable hacerlo.

## 2. Flujo de Tickets (`docs/tickets/`)
- Cada desarrollo debe corresponder a un ticket estructurado en `docs/tickets/T*.md`.
- No alterar los objetivos funcionales sin actualizar el ticket correspondiente.
- Al completar un ticket:
  1. Verificar todos sus criterios de aceptación con `cargo test --workspace`.
  2. Marcar el estado en `docs/tickets/README.md` a `✅ Completado`.
  3. **Realizar un commit de Git detallado** con el formato `<tipo>(<área>): <TID> - <título>` y desglose en el cuerpo de los cambios, archivos modificados y pruebas realizadas.
