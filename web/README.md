# web/ · Página de presentación de antOS

Landing en **Next.js 16** (App Router, TypeScript), pensada para desplegarse en Vercel.
La página es estática: se prerenderiza entera en el build y solo la demo del escritorio
y el cambio de tema se ejecutan en el navegador.

```sh
cd web
npm install
npm run dev        # → http://localhost:3000
npm run build      # build de producción (lo mismo que hace Vercel)
npm run typecheck
```

## Despliegue en Vercel

1. En Vercel: **Add New → Project** e importa `juandevelop85/antOS`.
2. En **Root Directory** elige `web`. Vercel detecta Next.js solo; no hace falta
   `vercel.json` ni variables de entorno.
3. **Deploy**. Cada push a la rama de producción vuelve a desplegar; las demás ramas
   generan despliegues de vista previa.

Las URLs absolutas de Open Graph salen de `VERCEL_PROJECT_PRODUCTION_URL`, que Vercel
define automáticamente.

## Estructura

```
app/
  layout.tsx       fuentes (next/font), metadatos, script de tema sin parpadeo
  page.tsx         secciones de la landing
  globals.css      tokens de color y estilos
  icon.png         favicon
components/
  DesktopDemo.tsx  demo interactiva del escritorio (cliente)
  ThemeToggle.tsx  cambio claro/oscuro (cliente)
lib/
  content.tsx      textos, tabla comparativa, crates y estado de cada módulo
  scenarios.ts     guiones de la demo
public/assets/     wallpaper, icono y mascota
```

La demo del hero es un **guion fijo** (`lib/scenarios.ts`): no habla con ningún
`antosd`, y la propia página lo dice.

## Identidad

Sale del concepto de icono y wallpaper del sistema: noche azul (`#070b24`), degradado
cian → violeta de las antenas y del «OS» del wordmark (`#1fb8ff → #5b6cff → #a45cff`),
Outfit para titulares, Manrope para texto y JetBrains Mono para terminal y datos.
El hero y la sección de descarga son siempre nocturnos; el resto sigue el tema claro/oscuro.

## Regla de honestidad (T31.14)

La sección «Estado» etiqueta cada funcionalidad como *Real / Parcial / Simulado /
En curso* a partir de las secciones `## Estado de implementación` de las cabeceras
de módulo (`ebpf.rs`, `collab.rs`, `profiler.rs`, `autopilot.rs`…). Al cambiar el
estado de un módulo, actualizar `STATUS` y `TICKETS` en `lib/content.tsx`.
