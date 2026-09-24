# web/ · Página de presentación de antOS (prototipo)

Landing estática, sin build ni dependencias: `index.html` + `styles.css` + `app.js` + `assets/`.

## Identidad

Sale del concepto de icono y wallpaper del sistema: noche azul (`#070b24`), degradado
cian → violeta de las antenas y del «OS» del wordmark (`#1fb8ff → #5b6cff → #a45cff`),
Outfit para titulares, Manrope para texto y JetBrains Mono para terminal y datos.
El hero y la sección de descarga son siempre nocturnos; el resto sigue el tema claro/oscuro.

- `assets/wallpaper.webp`: wallpaper del sistema, fondo del hero.
- `assets/icon.webp`, `assets/favicon.png`: icono de la app (recortado del paquete de iconos).
- `assets/mascot.webp`: la hormiga asomando sobre la colina, en la sección de descarga.

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
