# syso · capa de sistema (Vía A)

Convierte una intención en capacidades **tipadas, aisladas y reversibles**.
Dos documentos:

- [`docs/arquitectura.html`](../docs/arquitectura.html) — el diseño y por qué es así.
- [`docs/construccion.html`](../docs/construccion.html) — qué se construyó, en qué
  orden, con qué herramientas, y los quince fallos que solo se vieron ejecutando.

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

## Arrancar como sistema

`flake.nix` define la máquina entera como un valor: el paquete, un módulo de
NixOS y un servicio que al arrancar ejecuta `syso doctor` — el sistema
comprueba su propio recinto antes de que nadie pueda pedirle nada.

```bash
# La máquina, arrancable en QEMU
nix build .#nixosConfigurations.syso-vm.config.system.build.vm
./result/bin/run-syso-vm
```

Arrancada, el sistema comprueba su propio recinto antes de dar un login:

```
Starting syso · comprobar que el recinto es real...
syso[577]: recinto de ejecución
syso[577]:   ✓ escritura fuera de lo declarado: la deniega el kernel
syso[577]:   ✓ escritura dentro de lo declarado: permitida
syso[577]:   ✓ lectura fuera de lo declarado: la deniega el kernel
syso[577]:   ? red: no concluyente — esta máquina no llega a internet
syso[577]: ✓ el recinto se comporta como dice
[  OK  ] Finished syso · comprobar que el recinto es real.
```

Ese «la deniega el kernel» en la lectura es **Landlock**, en el sistema para
el que se escribió. La red sale como no concluyente porque la VM no tiene
salida a internet: es la respuesta correcta de una prueba que se hace en los
dos sentidos para no confundir «bloqueada» con «sin conexión».

En Apple Silicon no hay virtualización anidada, así que un generador de
imágenes de disco (`qcow-efi` y compañía) **no sirve**: monta una VM para
ensamblar la imagen y necesita `/dev/kvm`. `system.build.vm` no: se construye
como una derivación normal y el resultado es un guion que lanza QEMU montando
el store. Por eso es la vía que funciona aquí.

La configuración declarativa es la **segunda raíz** que syso reconoce. No es
una fuga —tiene nombre, `$SYSTEM_CONFIG`— pero cualquier capacidad que la
toque exige concesión, siempre, sin importar lo que diga su manifiesto:

```bash
target/debug/syso grant system.declare --minutos 10
target/debug/syso "declara htop en el sistema"
```

El diff es de una línea en [`system/nixos/syso-paquetes.nix`](nixos/syso-paquetes.nix),
que `configuracion.nix` importa. Aplicarlo sigue siendo tuyo y explícito:
`sudo nixos-rebuild switch`.

## Voz

```bash
./system/instalar-voz.sh          # whisper-cpp + ffmpeg + modelo local
target/debug/syso escucha         # graba 5 s del micrófono
target/debug/syso escucha --desde grabacion.aiff
```

El audio no sale de la máquina: transcribe Whisper en local. Y **la voz no
salta ningún control** — tras transcribir sigue el mismo recorrido que una
orden tecleada, con su diff y su confirmación.

Probarlo sin micrófono, sintetizando la frase:

```bash
say -v Monica -o /tmp/orden.aiff "crea un proyecto python llamado servidor"
target/debug/syso escucha --desde /tmp/orden.aiff
```

El transcriptor se ceba con el vocabulario del catálogo (`--prompt`), que sale
de los valores enumerados de los manifiestos: si mañana una capacidad acepta un
lenguaje nuevo, el transcriptor lo aprende solo.

Elegir micrófono, porque el predeterminado del sistema suele ser un dispositivo
virtual de Teams o Zoom y grabarías silencio sin enterarte:

```bash
target/debug/syso escucha --dispositivos
target/debug/syso escucha --dispositivo 1
```

### El silencio no es una intención

Whisper ante ruido de fondo no dice «no he oído nada»: **se inventa una frase
bien formada**. Grabando una habitación vacía salió «La gente se puede hacer un
proyecto de trabajo» — una intención plausible con la que un planificador puede
construir un plan de verdad.

Por eso se mide el nivel antes de transcribir. Los números salen de medir en
esta máquina, no de suponer:

| fuente | nivel medio |
|---|---|
| silencio digital | −91 dB |
| habitación en silencio | −47 dB |
| alguien hablando | −18 dB |

El umbral está en −40 dB (`SYSO_UMBRAL_VOZ` lo cambia). Filtrar los marcadores
`[BLANK_AUDIO]` no bastaba: la alucinación no viene marcada.

## El demonio

`sysod` se llamaba así desde el principio sin serlo. Ahora lo es:

```bash
target/debug/syso demonio          # escucha en .syso/syso.sock (0600)
target/debug/syso "crea un proyecto rust llamado demo"
```

Si hay demonio, los comandos van por el socket; si no, se hacen en proceso.
La salida es **idéntica byte a byte** porque ambos caminos usan el mismo
recorrido (`sesion.rs`) y el mismo dibujado (`terminal.rs`). Forzar el modo
local: `SYSO_SIN_DEMONIO=1`.

Lo que un cliente puede decidir es exactamente una cosa: contestar sí o no a
una propuesta. El nivel de permiso y las concesiones los decide el demonio,
así que un cliente no puede fabricar un nivel más bajo ni saltarse la puerta.

Es el requisito de cualquier escritorio: la interfaz gráfica será otro
cliente del mismo demonio, no otra copia del recorrido.

## La barra de intención

La primera superficie del escritorio. GTK4 sobre `wlr-layer-shell`, hablando
por el socket con el mismo demonio que atiende al terminal.

```bash
./system/compilar-barra.sh        # solo compila en Linux
syso demonio &
syso-barra
```

No es un lanzador de aplicaciones: escribes una intención y la superficie
crece hasta convertirse en la hoja del diff. **El botón de aprobar vive
debajo del diff**, así que para llegar a él hay que haberlo tenido delante —
en el terminal escribes «s», pero en una GUI un clic es barato y el documento
ya avisaba de que la confirmación se degrada con el uso. El nivel de permiso
no es una etiqueta: tiñe el borde de la hoja dentro de la que lees.

Escape cierra sin contestar, y el demonio lo trata como un no.

Los tipos que viajan por el socket viven en `system/protocolo`, un crate que
usan los dos lados. Un protocolo duplicado es un protocolo que diverge.

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
| Linux | Landlock (ABI ≥ 1) | escrituras, **lecturas** y TCP, por el kernel |
| macOS | Seatbelt (`sandbox-exec`) | escrituras y TCP; las lecturas no |
| otras | ninguno | nada, y lo dice en cada ejecución |

Cada plataforma se encierra a su manera: en macOS el padre envuelve al hijo con
`sandbox-exec`; en Linux el hijo se encierra a sí mismo con Landlock leyendo la
política del entorno.

Las lecturas solo se confinan en Linux. En macOS un perfil `(deny default)`
pelea con el enlazador dinámico y mata el proceso al arrancar; con la red
denegada, una lectura no declarada no puede salir a ningún sitio, pero la
garantía de verdad solo la da Landlock.

Verificarlo en Linux, desde macOS:

```bash
./system/verificar-linux.sh    # requiere podman con una máquina arrancada
```

## Una barra final significa "directorio"

```toml
[effects]
writes = ["$WORKSPACE/{name}/"]   # directorio: la capacidad escribe todo el árbol
writes = ["{path}"]               # fichero
```

No es cosmético. Landlock engancha sus reglas a un descriptor, así que la ruta
tiene que existir antes de encerrarse: syso crea de antemano los directorios
declarados. Deducirlo del nombre —¿tiene extensión?— sería exactamente el tipo
de suposición que este diseño existe para eliminar.

## Estado

Verificado ejecutándolo en **las dos plataformas**, con el planificador local:
plan, diff, niveles de permiso, denegación por defecto, instantánea, ejecución
confinada, bitácora y `undo`. Ocho pruebas en macOS y nueve en Linux cubren la
derivación del nivel, la contención y la generación de la política del recinto.

En Linux, con un kernel 7.1: `doctor` pasa sus cuatro comprobaciones, y el
ejecutor confinado recibe `EACCES` al intentar leer una clave privada que sin
confinar sí lee.

Las instantáneas usan `clonefile(2)` de APFS — clones copy-on-write, con caída
a copia byte a byte si el sistema de ficheros no lo permite. Cada entrada
registra cuál de las dos vías se usó.

Sin verificar todavía:

- **El planificador con Claude** compila, pero nunca ha hecho una petición
  real: no hubo credenciales durante el desarrollo.
- **Las lecturas no están confinadas en macOS**, y `sandbox-exec` lleva años
  deprecado. En Linux sí lo están.
- **Landlock solo cubre TCP**: UDP y los sockets unix quedan fuera de su
  alcance, así que la denegación de red no es total.
- **La captura por micrófono está probada con voz real** y funciona. Elige
  el dispositivo con `--dispositivo N`: el predeterminado del sistema suele
  ser un dispositivo virtual de Teams o Zoom.
- **La imagen de DISCO (qcow2, raw) no se puede construir en esta máquina**:
  esos formatos ensamblan la imagen dentro de una VM y Apple Silicon no da
  virtualización anidada. En cualquier Linux con `/dev/kvm` sí. Lo que sí
  funciona aquí es `system.build.vm`, que arranca la misma configuración.
- **syso declara pero no aplica.** `nixos-rebuild switch` sigue siendo manual:
  aplicar toca todo el sistema y tendría que correr fuera del recinto.
- **Los términos técnicos en español se transcriben mal.** «rust» sale como
  «rastre» con el modelo `base` y como «rastriamado» con `small`; el tamaño
  del modelo no lo arregla. Las frases sin anglicismos salen perfectas.
  Con el planificador de Claude el daño se amortigua —entendió «pites» como
  «pytest» y lo dijo en la nota del plan— pero puede corregir hacia el sitio
  equivocado con la misma seguridad. Lee el diff.
