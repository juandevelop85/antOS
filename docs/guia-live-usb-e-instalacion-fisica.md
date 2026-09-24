# Guía de Creación de Live USB e Instalación Física en Hardware Real

Esta guía describe el procedimiento paso a paso para generar un medio de instalación extraíble (Live USB) de **antOS**, arrancarlo en una computadora física (PC de escritorio o laptop) y desplegar el sistema operativo mediante el instalador nativo.

---

## 1. Requisitos Previos

* **Memoria USB (Pendrive):** Capacidad mínima recomendada de **4 GB** (idealmente USB 3.0 o superior para mayor velocidad).
* **Equipo Destino:**
  * Procesador x86_64 de 64 bits (o AArch64 para placas compatibles).
  * Memoria RAM mínima de **2 GB** (recomendado 4 GB o más).
  * Firmware UEFI con soporte de arranque de 64 bits.
  * Disco de destino (SSD NVMe o SATA) con al menos **8 GB** de espacio disponible.

---

## 2. Métodos para Crear el Live USB

### Método 0: Descargar la ISO de antOS Linux de la release (T36.3)

Para instalar **antOS Linux** (NixOS + escritorio antOS, el sistema de uso
diario) en una máquina nueva no hace falta compilar nada: cada tag `v*`
publica en [GitHub Releases](https://github.com/juandevelop85/antOS/releases)
`antos-linux-<versión>-x86_64.iso` y `…-aarch64.iso`, con `SHA256SUMS` (y
`SHA256SUMS.sig` cuando la release va firmada). La ISO es **autosuficiente**:
lleva dentro la closure del sistema que instala, el árbol fuente de antOS y
la fuente de `nixpkgs`, así que `antos install --apply` funciona sin red.

La ISO viaja **troceada**: GitHub limita cada fichero de una release a
2 GiB y la ISO pasa de 4 GiB. Se descargan las partes (`.part-aa`,
`.part-ab`, …) y se recomponen con `cat`, que es exactamente lo que hizo
`split` al trocearla:

```bash
cat antos-linux-<versión>-<arch>.iso.part-* > antos-linux-<versión>-<arch>.iso
sha256sum -c SHA256SUMS   # comprueba la ISO recompuesta y cada parte
# Firma (clave pública en docs/release-signing-key.pub del repositorio):
printf 'antos-release %s\n' "$(grep -v '^#' release-signing-key.pub)" > allowed
ssh-keygen -Y verify -f allowed -I antos-release -n antos-release -s SHA256SUMS.sig < SHA256SUMS
```

El orden importa y `*` lo da bien: las partes van con sufijo alfabético
(`aa`, `ab`, `ac`). Si `sha256sum -c` falla en una parte, se vuelve a
descargar solo esa.

Después se graba como cualquier ISO: `antos usb flash --image <iso> --target
/dev/sdX --apply` (Método A, paso 3), Ventoy, Rufus, Etcher o `dd` (Métodos
B–D). Requisitos de esta ISO: firmware UEFI, **≥ 4 GiB de RAM** para la
sesión en vivo (escritorio Plasma) y un disco con **≥ 20 GiB libres** para
la raíz (Disco Completo o hueco sin particionar en Dual-Boot). Tamaño
medido en la primera release (`v0.2.0`, aarch64): **4,11 GiB** (4218 MiB),
en tres partes de 1,9 GiB, 1,9 GiB y 417 MiB.

La descarga no necesita cuenta de GitHub: el repositorio es público desde
el 2026-09-24. Comprobado sin credenciales sobre la release `v0.2.0` —
`SHA256SUMS`, su firma y las partes de la ISO responden `200`, y
`ssh-keygen -Y verify` da `Good "antos-release" signature`.

Sin release a mano, la ISO se construye con Nix en cualquier Linux:
`nix build .#iso` (Método 5 del [manual](manual-de-comandos.md)).

### Método A: Utilidad Nativa de antOS (`antos usb`) [Recomendado para el Live bare-metal]

Si ya dispones de una instalación de antOS o del entorno de desarrollo:

1. **Construir la imagen híbrida autoarrancable:**
   ```bash
   antos usb build --arch x86_64 --out antos-live.iso
   ```
   *Esto compila el kernel `no_std`, empaqueta el initramfs y ensambla la ISO híbrida Limine (UEFI + MBR), generando automáticamente el archivo de suma de control `antos-live.iso.sha256`.*

2. **Listar memorias USB elegibles conectadas:**
   ```bash
   antos usb list
   ```
   *La utilidad filtra automáticamente los discos internos del sistema y muestra únicamente pendrives extraíbles seguros.*

3. **Grabar la memoria USB de forma segura:**
   ```bash
   antos usb flash --image antos-live.iso --target /dev/sdb --apply
   ```
   *El comando solicita confirmación explícita escribiendo `SI`, desmonta las particiones previas, vuelca la imagen en bloques de 4 MiB mostrando una barra de progreso en vivo con MB/s transferidos y verifica la integridad bit a bit con SHA-256.*

---

### Método B: Ventoy (Arranque Directo Multiboot)

antOS genera imágenes ISO 100% compatibles con **Ventoy**:
1. Conecta tu pendrive configurado con Ventoy.
2. Copia directamente el archivo `antos-live.iso` a la partición de datos de Ventoy (ej. mediante arrastrar y soltar en el explorador de archivos o `cp antos-live.iso /media/ventoy/`).
3. Al arrancar el equipo, selecciona `antos-live.iso` en el menú interactivo de Ventoy en modo normal (o modo grub2).

---

### Método C: Rufus (Windows)

1. Descarga e inicia **Rufus** (versión 3.x o 4.x).
2. En **Dispositivo**, selecciona tu pendrive USB.
3. En **Elección de arranque**, pulsa *Seleccionar* y elige `antos-live.iso`.
4. En **Esquema de partición**, selecciona **GPT** (o MBR para arranque híbrido).
5. En **Sistema de destino**, selecciona **UEFI (no CSM)**.
6. Pulsa **Empezar**. Si Rufus pregunta el modo de escritura, puedes elegir **Modo Imagen ISO** o **Modo Imagen DD** (ambos son totalmente compatibles).

---

### Método D: BalenaEtcher o `dd` (Linux / macOS)

* **Con BalenaEtcher:**
  1. Selecciona `Flash from file` y escoge `antos-live.iso`.
  2. Selecciona la memoria USB.
  3. Pulsa `Flash!`.

* **Con `dd` (Avanzado en Linux/macOS):**
  ```bash
  # En Linux:
  sudo dd if=antos-live.iso of=/dev/sdX bs=4M status=progress oflag=sync
  
  # En macOS:
  sudo dd if=antos-live.iso of=/dev/rdiskN bs=4m status=progress
  ```

---

## 3. Configuración del Firmware UEFI / BIOS

Antes de arrancar desde el pendrive USB:
1. Conecta la memoria USB en un puerto directo del equipo (preferiblemente USB 3.0 azul o USB-C).
2. Enciende el equipo y presiona repetidamente la tecla para acceder a la configuración del firmware o menú de arranque:
   * **Menú de Arranque (Boot Menu):** Generalmente `F12`, `F11`, `F8` o `F9` según el fabricante (Dell, Lenovo, HP, Asus).
   * **Configuración UEFI/BIOS:** `F2` o `Supr` (`Delete`).
3. **Ajustes clave:**
   * **Secure Boot:** Desactivar temporalmente (o enrolar la clave MOK de antOS si está configurado).
   * **Modo de almacenamiento:** Configurar los controladores SATA en modo **AHCI** (no RAID/Intel RST).
   * **Prioridad de arranque:** Seleccionar la entrada UEFI correspondiente a la memoria USB (ej. *UEFI: SanDisk Ultra*).

---

> 🧭 **Dos productos, dos ISOs.** Este documento y su `antos usb build` producen
> el **Live del kernel bare-metal** (`no_std`, Limine; Método 6). El **escritorio
> antOS Linux** de uso diario (NixOS + Wayland + `antos-barra` + Neovim) se
> instala desde la **ISO gráfica** de la Fase 30: `nix build .#iso` →
> `antos-linux-*.iso`, que se graba y arranca igual (secciones 2 y 3 de esta
> guía) pero cuyo `antos install` genera una configuración **NixOS**
> (`/etc/nixos/{flake.nix,configuration.nix}` con `services.antos.desktop.enable
> = true`) y ejecuta `nixos-install`. Ver §4-bis.

## 4. Sesión Live y Asistente de Instalación

1. **Menú de Arranque Limine:**
   Al iniciar, verás la pantalla de presentación de Limine con las opciones de arranque de antOS:
   * `antOS (Live Session & Installer)` (predeterminada).
   * `antOS (Safe Graphics / Framebuffer Fallback)`.

2. **Inicio del Sistema:**
   El kernel `no_std` se carga en memoria, inicializa los drivers PCI, AHCI/NVMe y monta el initramfs en memoria RAM. A continuación se despliega el entorno de usuario con la terminal de comandos y el entorno gráfico Wayland (`barra`).

3. **Ejecutar el Asistente de Instalación:**
   Abre una terminal y ejecuta:
   ```bash
   antos install
   ```
   El asistente guiado te llevará a través de los pasos:
   * **Inspección de Hardware:** Verificación de procesador, memoria RAM y arquitectura.
   * **Selección de Disco:** Identificación de SSDs internos (`/dev/nvme0n1`, `/dev/sda`).
   * **Modo de Instalación:**
     * *Opción A:* Disco completo (Instalación limpia, requiere escribir `SI`).
     * *Opción B:* Dual-Boot seguro (Preserva Windows/Linux y el cargador EFI existente).
   * **Configuración:** Hostname, usuario y zona horaria.
   * **Despliegue y Bootloader:** Creación de particiones GPT, copia del sistema base y registro en la NVRAM UEFI con `efibootmgr` (`antOS Linux`).

4. **Finalización:**
   Al terminar la instalación:
   ```bash
   reboot
   ```
   Retira la memoria USB cuando la pantalla se apague. El equipo iniciará directamente en tu nueva instalación de **antOS**.

---

## 4-bis. Instalar antOS Linux (NixOS + escritorio antOS) — T30.5

> **Estado real (T36.2, septiembre 2026):** `antos install` sin `--apply`
> simula (cada paso `○ simulado` con el comando literal; la configuración
> queda en `<workspace>/target/installer-staging/etc/nixos/`; el disco no se
> toca). Con `--apply`, desde la ISO en vivo como root, ejecuta la tabla de
> abajo de verdad; si algo falla, aborta con el error del comando y deja el
> disco desmontado. Lo que aún falta para que funcione **sin red desde la
> ISO** es que la ISO lleve la *closure* del sistema y el árbol fuente de
> antOS (**T36.3**); la verificación end-to-end en QEMU
> (`system/nixos/install-smoke.sh`) está escrita y se pondrá en verde con
> ella.

Desde la **ISO gráfica** (`nix build .#iso`), `antos install` sigue el mismo
asistente —inspección de hardware, selección de disco, Disco Completo /
Dual-Boot seguro— y pide (T36.4): **hostname**, **zona horaria**, **teclado**
(`us`, `es`, `latam`, `de`, `fr`, `gb`, `pt-br`, `it` u otro layout XKB),
**idioma** (`locale`, por defecto el del live), **usuario** y su
**contraseña** (dos veces, sin eco, mínimo 8 caracteres; se convierte en
hash con `mkpasswd -m yescrypt` y solo el hash llega al disco). Todo se
valida antes de tocar nada. El despliegue es NixOS:

| Paso | Qué hace (`--apply`) |
| :--- | :--- |
| Particionado | **Disco Completo:** `parted -s <disco> mklabel gpt mkpart ESP fat32 1MiB 513MiB set 1 esp on mkpart antos-root ext4 513MiB 100%`. **Dual-Boot:** se reutiliza la primera partición con flag `esp` y la raíz se crea en el mayor hueco libre (`parted … print free`; ≥ 20 GiB o se dice cuánto falta; nada se redimensiona). Después `partprobe` + `udevadm settle`. |
| Formateo | `mkfs.vfat -F32 -n ANTOS_ESP` **solo** sobre una ESP nueva (nunca sobre una ajena); `mkfs.ext4 -F -L antos-root`; UUIDs reales por `blkid`. |
| Montaje | Raíz en `/mnt/target`, ESP en `/mnt/target/boot` (lo que NixOS y `systemd-boot` esperan). |
| `/etc/nixos` | `nixos-generate-config --root /mnt/target` (el `hardware-configuration.nix` real) y después `flake.nix` (nixpkgs fijado al `flake.lock` del árbol, `antos.nixosModules.{default,desktop,llm,machine}`, `antos.overlays.default`, `nix.registry` con ambas fuentes para que `nixos-rebuild` funcione sin red, `system` detectado), `flake.lock`, `configuration.nix` con `services.antos.desktop.enable = true`, `autologinUser`, hostname, `services.antos.machine` (teclado, locale, zona horaria; y con él red, audio, bluetooth, firmware, sudo, energía, zram y `systemd-boot` — en Dual-Boot encadena Windows/otros Linux **sin tocar sus entradas**) y `initialHashedPassword` con el hash de la contraseña elegida; copia del árbol de antOS a `antos/`. |
| Instalación | `nixos-install --root /mnt/target --flake /mnt/target/etc/nixos#<hostname> --no-root-passwd --no-channel-copy --override-input antos path:/mnt/target/etc/nixos/antos --no-write-lock-file`, con su salida en directo. Si falla, el error son sus últimas 50 líneas. |
| Gestor de arranque | Lo instala y registra `systemd-boot` desde `nixos-install`; `antos install` **no** escribe ningún binario EFI ni llama a `efibootmgr` en esta vía. |
| Cierre | `sync`, `umount -R /mnt/target`, informe con cada paso `✓ ejecutado` (o `○ simulado`). |

La ESP se monta en `/boot` (lo que NixOS y `systemd-boot` esperan), no en
`/boot/efi`.

Tras `reboot`, el equipo entra **directo al escritorio antOS** (autologin
Wayland, barra anclada, `antosd` vivo; `antos ping` lo confirma desde una
terminal). A partir de ahí se evoluciona el sistema de forma declarativa:

```bash
sudo nixos-rebuild switch --flake /etc/nixos#<hostname>
antos doctor --desktop  # recinto, sesión Wayland, demonio por el socket, antos-barra
```

## 5. Primer arranque: `antos setup` (T36.5)

La primera sesión abre una terminal con `antos setup`. En pocos minutos deja
lo que un puesto de desarrollo necesita, y se puede repetir cuando se
quiera (cada paso se salta solo si ya está hecho):

1. **Identidad git** (nombre y correo de los commits).
2. **Clave SSH** ed25519; te muestra la pública para pegarla en GitHub/GitLab.
3. **Flathub**, para `antos app install <flatpak>`.
4. **Modelos**: perfil `local` (solo Ollama), `hybrid` (Ollama + nube) o
   `cloud`; la descarga del modelo local (varios GB) solo si la confirmas
   (`antos llm setup` después, si prefieres).
5. **Claves API** (Anthropic / OpenAI) a la bóveda cifrada, tecleadas sin
   eco. Nunca a un fichero en claro.
6. **`gh auth login`**, si quieres.
7. **`antos doctor --desktop`**, el resumen: recinto del usuario, sesión
   Wayland, demonio respondiendo por el socket, barra viva, Ollama,
   Flathub, identidad git.

`antos setup --status` muestra qué quedó hecho; `antos setup --yes --config
setup.toml` lo hace sin preguntas (las claves API nunca van en el TOML:
`antos secrets set ANTHROPIC_API_KEY`). El marcador es
`$ANTOS_STATE/setup.toml`.

> antOS Linux es **monousuario** en esta versión: el usuario del escritorio
> es el dueño del recinto y del demonio. Otro usuario del sistema no tiene
> barra ni socket; `antos doctor --desktop` lo dice.

## 6. Mantener el sistema: `antos system update` (T36.6)

```bash
antos system update          # diff de lo que cambia → confirmación → sudo nixos-rebuild switch
antos system rollback        # vuelve a la generación anterior (también `antos undo`)
antos system generations     # qué generaciones hay y cuál está activa
```

`update` trabaja sobre una copia de `/etc/nixos` y solo la devuelve al
sitio si la activación fue bien; si algo falla, `/etc/nixos` y su
`flake.lock` quedan como estaban. Un chequeo diario (`antos system update
--check`) evalúa sin construir ni descargar; nada se aplica sin que lo
apruebes viendo el diff. La fuente de antOS es `path:/etc/nixos/antos` (la
copia que dejó la instalación); con red puedes pasar a una release con
`--source github:juandevelop85/antOS/<tag>`.
