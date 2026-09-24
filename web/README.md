# web/ · Página de presentación de antOS (prototipo)

Landing estática, sin build ni dependencias: `index.html` + `styles.css` + `app.js`.

```sh
# abrir directamente o servir en local
python3 -m http.server -d web 8080   # → http://localhost:8080
```

## Estructura

1. **Hero** con una demo interactiva del escritorio (barra de intenciones + tablero
   de agentes + log + aprobación / `antos undo`). Es un **guion fijo** en `app.js`:
   no habla con ningún `antosd`, y la propia página lo dice.
2. Problema → Cómo funciona (tres capas) → Agentes antFlow → Seguridad y
   reversibilidad → Crates → **Estado honesto** → Descarga.

## Regla de honestidad (T31.14)

La sección «Estado» etiqueta cada funcionalidad como *Real / Parcial / Simulado /
En curso* a partir de las secciones `## Estado de implementación` de las cabeceras
de módulo (`ebpf.rs`, `collab.rs`, `profiler.rs`, `autopilot.rs`…). Al cambiar el
estado de un módulo, actualizar también esta lista y el contador de tickets.
