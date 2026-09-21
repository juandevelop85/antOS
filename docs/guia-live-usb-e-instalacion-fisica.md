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

### Método A: Utilidad Nativa de antOS (`antos usb`) [Recomendado]

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

> **Estado real (T36.1, septiembre 2026): `antos install` todavía no instala.**
> Sin `--apply`, simula: genera la configuración de la tabla de abajo en
> `<workspace>/target/installer-staging/etc/nixos/` y marca cada paso
> `○ simulado`; el disco no se toca. Con `--apply`, comprueba las
> precondiciones (Linux, `root`, `parted`/`mkfs.*`/`blkid`/`nixos-install`
> en `PATH`, `/mnt/target` montado) y se detiene con un error explícito: el
> particionado, el formateo, `nixos-generate-config` y `nixos-install`
> reales son **T36.2**; la ISO con la *closure* del sistema y el árbol
> fuente, **T36.3**. Nada de lo que sigue se anuncia como «completado» sin
> haberse ejecutado.

Desde la **ISO gráfica** (`nix build .#iso`), `antos install` sigue el mismo
asistente (inspección de hardware, selección de disco, Disco Completo / Dual-Boot
seguro, hostname/usuario/timezone/**keymap**) y el despliegue es NixOS:

| Paso | Qué hace | Estado |
| :--- | :--- | :--- |
| Particionado | GPT limpio (512 MiB ESP + raíz) en Disco Completo; en **Dual-Boot** preserva la ESP y las particiones ajenas | simulado (T36.2) |
| `/etc/nixos` | Genera `flake.nix` (nixpkgs fijado al `flake.lock` del árbol, `antos.nixosModules.{default,desktop,llm}`, `antos.overlays.default`, `system` detectado), `flake.lock`, `configuration.nix` con `services.antos.desktop.enable = true`, `autologinUser`, `networking.hostName`, `time.timeZone`, `console.keyMap` y `systemd-boot` (en Dual-Boot, `systemd-boot` encadena Windows/otros Linux **sin tocar sus entradas**), y copia el árbol de antOS a `antos/` | **real** (la CI lo evalúa con `nix eval`) |
| `hardware-configuration.nix` | De relleno (raíz `antos-root` y ESP `ANTOS_ESP` por etiqueta); lo escribe de verdad `nixos-generate-config --root /mnt/target` | simulado (T36.2) |
| Instalación | `nixos-install --root /mnt/target --flake /mnt/target/etc/nixos#<hostname> --no-root-passwd` (copia el *closure* del escritorio antOS) | simulado (T36.2) |
| NVRAM UEFI | Entrada `antOS Linux` registrada por `systemd-boot` desde `nixos-install`; `antos install` **no** escribe ningún binario EFI ni llama a `efibootmgr` en esta vía | simulado (T36.2) |

La ESP se monta en `/boot` (lo que NixOS y `systemd-boot` esperan), no en
`/boot/efi`.

Cuando T36.2 cierre, tras `reboot` el equipo entrará **directo al escritorio
antOS** (autologin Wayland, barra anclada, `antosd` vivo). A partir de ahí se
evoluciona el sistema de forma declarativa:

```bash
sudo nixos-rebuild switch --flake /etc/nixos#<hostname>
antos doctor            # verifica el recinto y que antosd/antos-barra levantan
```
