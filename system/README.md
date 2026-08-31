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

## El recinto

La ejecución no ocurre en `sysod`: ocurre en un proceso aparte confinado por el
kernel, con una política derivada de los efectos declarados. Esa separación es
necesaria — el confinamiento afecta al proceso entero, así que si el broker
ejecutara los cambios él mismo no podría escribir su propia bitácora.

```bash
target/debug/syso doctor    # ataca su propio recinto y comprueba que aguanta
```

| Plataforma | Motor | Garantiza |
|---|---|---|
| macOS | Seatbelt (`sandbox-exec`) | escrituras y red, por el kernel |
| Linux | — | sin escribir todavía; el destino son espacios de nombres |
| otras | ninguno | nada, y lo dice en cada ejecución |

Las lecturas **no** están confinadas en macOS: un perfil `(deny default)` pelea
con el enlazador dinámico y mata el proceso al arrancar. Con la red denegada,
una lectura no declarada no puede salir a ningún sitio, pero la garantía real
llegará con el motor de Linux.

## Estado

Verificado ejecutándolo, con el planificador local: plan, diff, niveles de
permiso, denegación por defecto, instantánea, ejecución confinada, bitácora y
`undo`. Siete pruebas cubren la derivación del nivel, la contención y la
generación de la política del recinto.

Las instantáneas usan `clonefile(2)` de APFS — clones copy-on-write, con caída
a copia byte a byte si el sistema de ficheros no lo permite. Cada entrada
registra cuál de las dos vías se usó.

Sin verificar todavía:

- **El planificador con Claude** compila, pero nunca ha hecho una petición
  real: no hubo credenciales durante el desarrollo.
- **Las lecturas no están confinadas**, y `sandbox-exec` lleva años deprecado.
- **No hay motor de Linux**, que es el destino real del producto.
