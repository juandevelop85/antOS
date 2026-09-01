---
name: antos-ticket-runner
description: >-
  Protocolo estandarizado para la planificación, desarrollo, verificación y cierre
  de tickets técnicos ubicados en `docs/tickets/` para antOS. Activar cuando el usuario
  quiera resolver, continuar o auditar un ticket (ej. 'ejecuta el ticket T1.1', 'desarrolla T1.2').
---

# antOS · Ticket Runner Protocol

Esta skill establece el flujo de trabajo riguroso y automatizado para implementar cualquier ticket del backlog de antOS ubicado en [`docs/tickets/`](file:///Users/juandevelop/Develop/antOS/docs/tickets).

---

## 1. Verificación y Contexto Previo

Antes de escribir código:

1. **Lectura del Catálogo Global:**
   - Inspeccionar [`docs/tickets/README.md`](file:///Users/juandevelop/Develop/antOS/docs/tickets/README.md) para verificar el estado de los tickets predecesores.
   - Comprobar que no haya dependencias de fases previas bloqueantes.

2. **Lectura del Ticket Asignado:**
   - Leer el archivo del ticket (ej. `docs/tickets/T1.1-*.md`).
   - Identificar:
     - **Descripción y Objetivo.**
     - **Alcance Técnico:** crates afectados (`system/protocolo`, `system/sysod`, `kernel`, `builder`, etc.), tipos de datos y mensajes IPC.
     - **Criterios de Aceptación:** tests unitarios, compatibilidad, no regresiones.

---

## 2. Planificación y Diseño

1. **Determinación de Crates Afectados:**
   - Identificar si el cambio requiere modificaciones en:
     - `system/protocolo`: Estructuras tipadas, serialización `serde`, mensajes IPC.
     - `system/sysod`: Demonio del sistema, analizadores de fondo, despacho de capacidades.
     - `system/capabilities`: Módulos de aislamiento, Sandboxing (Landlock), Git semántico.
     - `system/barra`: UI en GTK4 / Relm4 / Wayland.
     - `kernel`: Bare metal x86_64 / aarch64 (si aplica).
2. **Definición de Pruebas (TDD):**
   - Planificar los tests unitarios e integrados requeridos según los criterios de aceptación del ticket.

---

## 3. Implementación y Compilación

1. **Desarrollo en Rust:**
   - Seguir las directrices de código de antOS:
     - Manejo exhaustivo de errores con `Result<T, E>`.
     - Evitar `unwrap()` o `expect()` en código de producción/IPC.
     - Tipado estricto y serialización segura con `serde`.
2. **Compilación y Chequeo:**
   - Ejecutar verificación estática:
     ```bash
     cargo check --workspace
     ```
   - Ejecutar tests específicos del crate modificado:
     ```bash
     cargo test -p antos-protocolo
     # o el crate correspondiente
     ```

---

## 4. Validación de Criterios de Aceptación

1. Revisar cada punto de los **Criterios de Aceptación** definidos en el archivo del ticket.
2. Asegurar que todas las pruebas pasen sin advertencias (`warnings`) críticas de clippy o del compilador.

---

## 5. Actualización y Cierre de Ticket

1. Actualizar el estado en [`docs/tickets/README.md`](file:///Users/juandevelop/Develop/antOS/docs/tickets/README.md):
   - Cambiar `⏳ Pendiente` o `🔄 En Progreso` a `✅ Completado`.
2. Notificar al usuario con un resumen conciso:
   - Archivos y crates modificados.
   - Tests ejecutados y su resultado.
   - Sugerencia del siguiente ticket en la hoja de ruta.
