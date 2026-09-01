---
trigger: always_on
---

# Reglas de Desarrollo de antOS

## 1. Arquitectura y Convenciones de Código (Rust)
- **Crates del Workspace:**
  - `system/protocolo` (`antos-protocolo`): Definición de tipos de datos puros, IPC y mensajes serializables con `serde`. No debe contener lógica de I/O pesada ni dependencias del demonio.
  - `system/sysod`: Demonio del sistema operativo encargado de servicios de fondo, despacho IPC y coordinación de capacidades.
  - `system/capabilities`: Módulos de capacidades tipadas (Git semántico, Worktrees, puertos, nix services).
  - `kernel`: Crate `no_std` para bare metal. No mezclar dependencias de `std` con el kernel.
- **Calidad y Robustez:**
  - Cero `unwrap()` o `expect()` en rutas de ejecución de IPC o demonio; utilizar siempre propagación de errores (`?`) o manejo explícito.
  - Documentar structs y mensajes públicos expuestos a través del protocolo IPC.

## 2. Flujo de Tickets (`docs/tickets/`)
- Cada desarrollo debe corresponder a un ticket estructurado en `docs/tickets/T*.md`.
- No alterar los objetivos funcionales sin actualizar el ticket correspondiente.
- Al completar un ticket:
  1. Verificar todos sus criterios de aceptación con `cargo test --workspace`.
  2. Marcar el estado en `docs/tickets/README.md` a `✅ Completado`.
  3. **Realizar un commit de Git detallado** con el formato `<tipo>(<área>): <TID> - <título>` y desglose en el cuerpo de los cambios, archivos modificados y pruebas realizadas.
