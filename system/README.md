# syso · capa de sistema (Vía A)

Convierte una intención en capacidades **tipadas, aisladas y reversibles**.
Arquitectura completa: [`docs/arquitectura.html`](../docs/arquitectura.html).

## Probarlo

```bash
cargo build -p sysod

target/debug/syso caps                                        # catálogo
target/debug/syso -n "crea un proyecto rust llamado demo"     # planificar sin ejecutar
target/debug/syso "crea un proyecto rust llamado demo"        # diff + confirmación
target/debug/syso log                                         # bitácora
target/debug/syso undo                                        # revertir
```

Las capacidades de nivel «concesión» están denegadas por defecto:

```bash
target/debug/syso "borra demo"                # denegado
target/debug/syso grant fs.delete --minutos 5
target/debug/syso "borra demo"                # ahora sí
```

## Planificadores

| | |
|---|---|
| `local` | Reglas de palabras clave, determinista, sin red. Entiende un puñado de frases. |
| `claude` | Messages API con el catálogo como herramientas tipadas. Necesita `ANTHROPIC_API_KEY`. |

Por defecto usa `claude` si hay clave y `local` si no; la salida siempre dice cuál corrió.
Forzar uno: `--planificador local|claude`.

## Estado de M1

Verificado de punta a punta con el planificador local: plan, diff, niveles de
permiso, denegación por defecto, instantánea, ejecución, bitácora y `undo`.
Tres pruebas cubren la derivación del nivel y la contención.

Sin verificar todavía:

- **El planificador con Claude** compila, pero nunca ha hecho una petición
  real: no hubo credenciales durante el desarrollo.
- **El aislamiento es validación de rutas, no un recinto de verdad.** Una
  capacidad se comprueba contra el espacio de trabajo, pero nada le impide
  físicamente salirse. Eso es M2 (espacios de nombres).
- **Las instantáneas son copias de ficheros.** Correcto para efectos acotados,
  pero no escala. M2 lo sustituye por instantáneas del sistema de ficheros.
