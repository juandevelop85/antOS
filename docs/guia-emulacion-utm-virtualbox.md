# antOS · Guía de Emulación y Ejecución en UTM, VirtualBox y QEMU 🐜⚡

Esta guía proporciona instrucciones detalladas, actualizadas y verificadas paso a paso para compilar, generar imágenes de disco arrancables e ISOs de **antOS**, y ejecutarlas con éxito en los hipervisores y emuladores gráficos más comunes (**UTM 4.x** en macOS y **VirtualBox 7.x**), así como en **QEMU** directo desde la terminal.

---

## 📑 Tabla de Contenidos
1. [Arquitectura Gráfica vs Serie (Comprender la Salida de Pantalla)](#1-arquitectura-gráfica-vs-serie-comprender-la-salida-de-pantalla)
2. [Catálogo de Artefactos de Arranque](#2-catálogo-de-artefactos-de-arranque)
3. [Comandos de Compilación y Generación de Imágenes](#3-comandos-de-compilación-y-generación-de-imágenes)
4. [Guía Paso a Paso para UTM 4.x (macOS Apple Silicon e Intel)](#4-guía-paso-a-paso-para-utm-4x-macos-apple-silicon-e-intel)
   - [Método A: Arranque Directo del Kernel con QEMU (Recomendado en ARM64)](#método-a-arranque-directo-del-kernel-con-qemu-recomendado-en-arm64)
   - [Método B: Arranque con Imagen de Disco UEFI (GPT / Limine)](#método-b-arranque-con-imagen-de-disco-uefi-gpt--limine)
   - [Resolución de Problemas en UTM](#resolución-de-problemas-en-utm)
5. [Guía Paso a Paso para VirtualBox 7.x](#5-guía-paso-a-paso-para-virtualbox-7x)
   - [5.1 VirtualBox 7 en macOS Apple Silicon (Host ARM64 / AArch64)](#51-virtualbox-7-en-macos-apple-silicon-host-arm64--aarch64)
   - [5.2 VirtualBox 7 en PCs y Macs Intel/AMD (x86_64)](#52-virtualbox-7-en-pcs-y-macs-intelamd-x86_64)
   - [Resolución de Problemas en VirtualBox](#resolución-de-problemas-en-virtualbox)
6. [Comandos Rápidos para QEMU en Terminal](#6-comandos-rápidos-para-qemu-en-terminal)

---

## 1. Arquitectura Gráfica vs Serie (Comprender la Salida de Pantalla)

Antes de configurar cualquier máquina virtual, es fundamental entender por dónde emite sus mensajes el kernel bare-metal de antOS según la arquitectura objetivo:

* **En AArch64 (ARM64 - Apple Silicon / QEMU virt):**  
  El kernel bare-metal siempre emite su telemetría por el puerto serie UART estándar **PL011**
  (mapeado en `0x09000000`), y desde la **Fase 26** también detecta un framebuffer gráfico —
  `simple-framebuffer` vía DTB o un dispositivo `virtio-gpu` MMIO (T26.1) — y, si lo encuentra,
  renderiza ahí el **Desktop Shell nativo** (compositor 2D, T26.2) además de la consola serie.
  > ⚠️ **Punto Crítico en ARM64:**  
  > Por defecto, UTM y VirtualBox abren únicamente una ventana gráfica (*Display*), y no toda
  > configuración de VM expone un dispositivo `virtio-gpu` que antOS pueda detectar. Si tu VM no
  > tiene GPU virtual configurada, la ventana gráfica quedará en negro o mostrará *"Guest has not
  > initialized the display (yet)"* — comportamiento esperado en modo headless/serie. **Añade
  > siempre un Puerto Serie (Terminal o archivo de log) para ver el arranque de antOS en ARM64**,
  > sea que tengas GPU virtual configurada o no.

* **En x86_64 (Intel / AMD):**  
  antOS incorpora tanto una **Consola Gráfica Framebuffer** en pantalla con fuente bitmap y secuencias ANSI (T23.3) como salida simultánea por puerto serie **COM1** (I/O `0x3F8`).  
  Verás la consola directamente en la pantalla de la máquina virtual y también a través de la terminal serie.

---

## 2. Catálogo de Artefactos de Arranque

antOS soporta dos arquitecturas bare-metal (`no_std`) y genera los siguientes artefactos:

> Las rutas usan `debug/`; con `--release` cambia el segmento a `release/`
> (idéntico árbol). **Para UTM y VirtualBox en Apple Silicon compila en
> `--release`** — emulan con TCG y el `debug` es demasiado lento.

| Arquitectura | Tipo de Archivo | Ruta Relativa | Compatibilidad Recomendada |
| :--- | :--- | :--- | :--- |
| **AArch64 (ARM64)** | Binario ELF | `kernel/target/aarch64-unknown-none/release/kernel` | **UTM (Método A)** y **QEMU `-kernel`**. |
| **AArch64 (ARM64)** | Disco UEFI GPT | `kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img` | **UTM (Método B)** y **VirtualBox 7 ARM64** (convertido a `.vdi`). |
| **AArch64 (ARM64)** | Live ISO | `kernel/target/aarch64-unknown-none/release/antos-aarch64.iso` | Medios USB y pruebas ópticas UEFI. |
| **x86_64** | Binario ELF | `kernel/target/x86_64-unknown-none/debug/kernel` | Kernel bare-metal x86_64. |
| **x86_64** | Imagen BIOS MBR | `kernel/target/x86_64-unknown-none/debug/antos-bios.img` | **VirtualBox 7 x86_64 (BIOS clásico)** y **QEMU x86_64**. |
| **x86_64** | Disco UEFI GPT | `kernel/target/x86_64-unknown-none/debug/antos-uefi-x86_64.img` | **UTM x86_64 (UEFI)** y **VirtualBox 7 (con EFI activado)**. |
| **x86_64** | Live ISO | `kernel/target/x86_64-unknown-none/debug/antos-x86_64.iso` | ISO híbrida UEFI/BIOS con Ramdisk live (T24.2). |

---

## 3. Comandos de Compilación y Generación de Imágenes

> ⚠️ **Toda imagen UEFI (disco GPT, ISO) exige compilar el kernel con `--features limine` (T27.1).**
> El binario por defecto se enlaza en la mitad baja del espacio de direcciones — le basta al
> arranque directo de QEMU, pero el Limine real embebido en las imágenes UEFI lo rechaza con
> `PANIC: elf: Lower half PHDRs are not allowed` (el pánico típico al arrancar en UTM Método B o
> en VirtualBox). Este flag no afecta ni hace falta para el arranque directo de kernel (Método A).

### A. Para Arquitectura ARM64 (AArch64 - Mac Apple Silicon)

La vía corta es `system/run-arm.sh`, que encadena los tres pasos:

```bash
system/run-arm.sh --release --build-only          # ELF optimizado (Método A)
system/run-arm.sh --uefi --release --build-only    # + imagen UEFI/ISO (Método B)
```

Paso a paso, si prefieres invocar cada herramienta:

```bash
# 1. Asegurar el target cruzado en Rust
rustup target add aarch64-unknown-none

# 2a. Compilar el kernel AArch64 para arranque directo de QEMU (Método A más abajo)
(cd kernel && cargo build --target aarch64-unknown-none --release)

# 2b. — o — compilarlo hablando el protocolo Limine, para generar una imagen UEFI (Método B)
(cd kernel && cargo build --target aarch64-unknown-none --release --features limine)

# 3. Generar el disco GPT con partición ESP FAT32 (/EFI/BOOT/BOOTAA64.EFI) y la Live ISO
# a partir del binario que hayas compilado en el paso 2a o 2b
cargo run -p builder -- kernel/target/aarch64-unknown-none/release/kernel --arch aarch64
```

### B. Para Arquitectura x86_64 (Intel / AMD)

```bash
# 1. Asegurar el target cruzado en Rust
rustup target add x86_64-unknown-none

# 2a. Compilar el kernel x86_64 para BIOS (crate `bootloader`, ./run.sh usa este mismo binario)
(cd kernel && cargo build)

# 2b. — o — compilarlo hablando el protocolo Limine, para generar una imagen UEFI
(cd kernel && cargo build --features limine)

# 3. Generar imagen BIOS MBR, disco UEFI GPT y Live ISO híbrida a partir de ese binario
# (--format all asume que ya elegiste 2a o 2b según qué imágenes necesitas realmente)
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format all
```

---

## 4. Guía Paso a Paso para UTM 4.x (macOS Apple Silicon e Intel)

UTM en macOS soporta dos motores de virtualización: **Apple Virtualization (VZ)** y **QEMU**.  
Para ejecutar kernels bare-metal experimentales o utilizar arranque directo, **el motor QEMU es el indicado**.

> ⚡ **Compila siempre en `--release` para UTM.**
> UTM en Apple Silicon sólo acelera por hardware (HVF) cuando la CPU invitada
> es `host`. antOS arranca con `-cpu cortex-a72`, así que UTM lo **emula con
> TCG**. El binario `debug` es entre 10× y 40× más lento y, en la práctica,
> parece colgado (no llega a imprimir el banner en 40 s). Genera los artefactos
> con `--release`:
>
> ```bash
> system/run-arm.sh --uefi --release --build-only   # imagen UEFI (Método B)
> system/run-arm.sh --release --build-only          # binario ELF   (Método A)
> ```
>
> Las rutas quedan en `kernel/target/aarch64-unknown-none/release/`.

> 🧭 **Qué método elegir:**
> | | Método A (`-kernel`) | Método B (imagen UEFI) |
> | :--- | :--- | :--- |
> | Arranque | ELF directo, sin firmware | EDK2 + Limine |
> | Salida | **Sólo serie** (PL011) | **Gráfica** (GOP de Limine) + serie |
> | GPU / USB PCIe | Se detectan pero se **omiten** (¹) | **Funcionan** (virtio-gpu-pci, xHCI) |
> | Entrada | Sólo consola serie | Teclado + ratón USB (xHCI) |
> | Iteración | La más rápida (recompila y relanza) | Requiere regenerar la `.img` |
> | Uso recomendado | Bucle de kernel / depuración de arranque | **Escritorio completo en UTM** |
>
> (¹) `-M virt -kernel` no ejecuta ningún asignador de recursos PCI, así que los
> BAR de `qemu-xhci` y `virtio-gpu-pci` quedan sin programar. El kernel lo
> detecta (`BAR0 sin asignar`) y salta esos controladores en lugar de fallar; la
> salida sigue por la UART. Para GPU y USB en UTM usa el **Método B**.

---

### Método A: Arranque Directo del Kernel con QEMU (bucle de desarrollo)

Este método es el más rápido para iterar sobre el arranque del kernel en Macs
con chip M1/M2/M3/M4. La salida es **exclusivamente por puerto serie**: no hay
firmware que programe los BAR PCIe, de modo que la GPU y el xHCI se omiten
(desde el commit de robustez de BAR ya no provocan pánico). Para escritorio
gráfico y USB, ve al **[Método B](#método-b-arranque-con-imagen-de-disco-uefi-gpt--limine)**.

1. **Crear la Máquina Virtual en el Asistente:**
   * Abre UTM y pulsa el botón **+** en la barra superior.
   * En la pantalla de inicio:
     * En Mac con Apple Silicon: Selecciona **Emular (Emulate)** o **Virtualizar (Virtualize)**.  
       *(Nota: Seleccionar **Emular -> Otro** garantiza el acceso completo al backend QEMU con opciones avanzadas de kernel directo y puerto serie)*.
     * Selecciona **Otro (Other)**.
     * En la pantalla de selección de medio: marca la casilla **Omitir arranque ISO** (*Skip ISO boot*).
     * Memoria y CPU: Asigna `1024 MB` de RAM y `2` núcleos de CPU.
     * Almacenamiento: Puedes dejar el disco sugerido o reducirlo a `1 GB` (no se usará en este método).
     * Finalizar el asistente guardando la VM con el nombre `antOS-Kernel-Direct`.

2. **Ajustar la Configuración de la VM (`Clic derecho -> Editar`):**
   * **Pestaña Sistema (System):**
     * **Arquitectura:** `ARM64 (aarch64)`.
     * **Sistema / Máquina:** `QEMU ARM Virtual Machine (virt)` (versión virt estándar o recomendada).
     * **Memoria RAM:** `1024 MB`.
   * **Pestaña QEMU:**
     * ⚠️ **Desmarcar la casilla "Arranque UEFI"** (en la sección *Retoques* de la pestaña QEMU).
   * **Carga del Kernel en UTM 4.5+ (QEMU 10):**
     En las versiones actuales de UTM (con QEMU 10), UTM ha retirado el selector gráfico de archivos de kernel. Para pasar el binario del kernel:
     * En la barra lateral izquierda, haz clic en **`[A] Argument...`** (justo debajo de **QEMU**).
     * Pulsa el botón **`+`** (o *Nuevo argumento*).
     * Introduce dos entradas (o el flag y el valor):
       1. Argumento: `-kernel`
       2. Valor / siguiente argumento: `<ruta-del-repo>/kernel/target/aarch64-unknown-none/release/kernel`
          (usa `release/`, no `debug/` — ver el aviso de `--release` arriba).
     * *(Opcional)* Para forzar GICv3 en el arranque directo (sin DTB), añade
       también los argumentos `-machine` y `virt,gic-version=3`. El kernel sondea
       el bloque GICC y, si no responde, conmuta a GICv3 por sí solo, así que
       esto sólo es necesario si quieres fijar la versión.
   * **Pestaña Dispositivos (Crucial para ver la salida de pantalla):**
     * En la barra lateral izquierda, bajo **Dispositivos**, pulsa **+ Nuevo...** -> **Puerto serie**.
     * Modo: **Terminal** (Consola integrada).
     * *(Opcional)*: Si eliminas el dispositivo **Monitor**, la VM abrirá automáticamente la terminal de texto serie al arrancar.

> 💡 **Recomendación (El Camino Más Sencillo en UTM):**  
> Debido a que UTM 4.5+ no tiene selector gráfico de kernel, el **[Método B (Disco UEFI)](#método-b-arranque-con-imagen-de-disco-uefi-gpt--limine)** es hoy el método 100% gráfico y más cómodo: mantienes *Arranque UEFI* activado e importas el archivo `.img` en *Unidades de disco*.

3. **Iniciar la Máquina Virtual:**
   * Pulsa **Play (▶️)**.
   * Si mantuviste la pantalla gráfica, pulsa el icono de la **Terminal** en la barra superior de la ventana de la VM.
   * Verás la ejecución en tiempo real del kernel antOS:
     ```text
     antOS · kernel AArch64
     ═══════════════════════
     arranque
       arquitectura AArch64 (ARM 64-bit)
       uart         pl011 inicializado en 0x09000000
     ...
     espacio de usuario (EL0) y llamadas al sistema (SVC)
       userspace    ¡saludo desde espacio de usuario (EL0) via svc #0!
       retorno      el programa EL0 finalizó limpiamente con código de salida 42
     ```

---

### Método B: Arranque con Imagen de Disco UEFI (GPT / Limine) — **recomendado para escritorio en UTM**

Con firmware UEFI (EDK2) el arranque es completo: EDK2 programa los BAR PCIe —el
mismo escenario que VirtualBox ARM64, donde el teclado USB ya está validado—, así
que **la GPU (`virtio-gpu-pci`) y la entrada USB (xHCI) quedan operativas** y el
compositor pinta el Desktop Shell sobre el framebuffer GOP de Limine.

0. **Generar la imagen (una vez por cambio de código):**
   ```bash
   system/run-arm.sh --uefi --release --build-only
   # -> kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img
   ```

1. **Crear la VM en UTM:**
   * Pulsa **+** -> **Emular** -> **Otro (Other)** -> **Omitir arranque ISO**.
   * Memoria: `1024 MB`, CPU: `2 núcleos`.
   * En almacenamiento: desmarca la creación de disco o crea uno efímero.
2. **Configurar Unidades y Firmware (`Editar VM`):**
   * **Pestaña Sistema:** Arquitectura `ARM64 (aarch64)`, máquina `QEMU ARM
     Virtual Machine (virt)`.
   * **Pestaña QEMU:** Asegúrate de que **"UEFI Boot" esté MARCADO**.
   * **Pestaña Unidades (Drives):**
     * Si el asistente creó un disco duro vacío de 64 GB, elimínalo.
     * Pulsa **Nueva unidad...** -> **Importar unidad...** (*Import Drive*).
     * Selecciona el archivo:
       ```text
       <ruta-del-repo>/kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img
       ```
     * En los detalles de la unidad importada, verifica que la **Interfaz** sea `VirtIO` (o `NVMe`).
     * ⚠️ **No marques la unidad como extraíble ni como CD/DVD.**
   * **Pestaña Pantalla (Display):**
     * Tarjeta emulada: **`virtio-gpu-pci`** (predeterminada en UTM para ARM).
       Es la que antOS trae por PCIe (T28.7) con *flush* explícito por
       rectángulo — cursor rápido y sin descuadre. `virtio-ramfb-gl` también
       sirve si tu build de UTM la ofrece.
   * **Pestaña Entrada (Input):**
     * Deja **USB** (UTM adjunta `usb-kbd` + `usb-tablet` sobre `qemu-xhci`).
       Es la pila HID xHCI del kernel (T28.3/T28.4); en el log verás
       `roothub …` y, al conectar, `usb-debug slot N … role=Keyboard|Mouse`.
   * **Pestaña Dispositivos:**
     * Pulsa **Nuevo...** -> **Puerto serie** -> Modo: **Terminal** (para ver el
       log de arranque; la ventana gráfica muestra el Desktop Shell).
3. **Iniciar y Secuencia de Arranque:**
   * Pulsa **Play**.
   * El firmware EDK2 buscará la partición EFI. Si entra en la **UEFI Interactive Shell**, el script `/startup.nsh` (T24.1) se ejecutará automáticamente tras 5 segundos.
   * Si necesitas ejecutarlo manualmente en la Shell UEFI:
     ```text
     FS0:
     startup.nsh
     ```
   * En la pestaña **Terminal** de UTM verás los mensajes del kernel y el
     cargador; con UEFI el GIC se autodetecta por DTB/ACPI (en Apple Silicon con
     TCG, `virt` expone GICv2).

---

### Resolución de Problemas en UTM

* **El arranque parece colgado (sin banner tras decenas de segundos):**  
  Casi siempre es el binario `debug` emulado con TCG. Recompila con `--release`
  (`system/run-arm.sh --uefi --release --build-only` o `--release --build-only`)
  y apunta la VM al artefacto de `release/`. Un arranque `release` bajo TCG en un
  M-series llega al `antos>` en pocos segundos.
* **En el Método A no aparecen la GPU ni el teclado USB:**  
  Es esperado: `-kernel` sin firmware no asigna ventanas a los BAR PCIe. El
  kernel lo avisa (`pcie-xhci … BAR0 sin asignar · omitido`) y la GPU cae por
  la cadena de framebuffer hasta `modo headless`; ninguno provoca pánico. Usa
  el **Método B (UEFI)** para escritorio y USB.
* **La ventana se queda en negro o dice *"Guest has not initialized the display (yet)"*:**  
  Comportamiento esperado si la VM no expone un `virtio-gpu` que antOS pueda detectar (T26.1): el
  kernel sigue arrancando normalmente, solo que en modo headless por la UART PL011. Asegúrate de
  haber añadido un **Puerto Serie** en modo **Terminal** en la configuración de la VM y haz clic
  en el icono de terminal de la barra superior — ahí verás el arranque completo y, al final, el
  prompt `antos>` del shell interactivo soberano (T26.5).
* **Error *"qemu-system-aarch64: -kernel: cannot load elf"***:  
  Verifica que compilaste para el target `aarch64-unknown-none` y no para el target por defecto del host (`x86_64` o `aarch64-apple-darwin`).
* **En la UEFI Shell no aparece `FS0:` al escribir `map -r`:**  
  La imagen de disco fue añadida con una interfaz no soportada por el driver UEFI o como lector de CD/DVD. Cambia la interfaz de la unidad a `VirtIO Drive` o `NVMe Drive`.

---

## 5. Guía Paso a Paso para VirtualBox 7.x

---

### 5.1 VirtualBox 7 en macOS Apple Silicon (Host ARM64 / AArch64)

VirtualBox 7 en procesadores Apple Silicon (M1/M2/M3/M4) **solo virtualiza máquinas ARM64**, y utiliza obligatoriamente **firmware UEFI ARM64**.

#### Paso 1: Convertir la Imagen RAW a Formato VDI
VirtualBox no admite imágenes RAW `.img` directamente en su selector gráfico.

> ⚡ **Compila en `--release`.** VirtualBox ARM64 no acelera el kernel de antOS;
> el binario `debug` es demasiado lento para usarlo (se queda minutos antes del
> banner). Todas las rutas de abajo usan `release/`.

```bash
# 1. Compilar (Limine) y generar la imagen UEFI optimizada
system/run-arm.sh --uefi --release --build-only

# 2. Convertir el disco GPT a disco virtual VDI nativo de VirtualBox
VBoxManage convertfromraw \
  kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img \
  antos-arm64.vdi --format VDI
```
*(Nota: Si ya habías ejecutado este comando antes, elimina el archivo anterior `rm -f antos-arm64.vdi` para evitar errores de UUID duplicado)*.

#### Paso 2: Configurar la Máquina Virtual en la Interfaz de VirtualBox 7
1. **Crear Máquina Virtual:**
   * Pulsa **Nueva**.
   * **Nombre:** `antOS-ARM64`.
   * **Imagen ISO:** Deja el campo vacío (o activa *"Omitir instalación desatendida"*).
   * **Tipo:** `Other` (u `Otro`).
   * **Versión:** `Other/Unknown (64-bit)` o `Linux (ARM 64-bit)`.
2. **Hardware:**
   * Memoria RAM: `1024 MB` o `2048 MB`.
   * Procesadores: `2 CPUs`.
   * Casilla **Habilitar EFI**: Viene marcada por defecto (en ARM64 es obligatoria).
3. **Disco Duro:**
   * Selecciona **"Usar un archivo de disco duro virtual existente"**.
   * Pulsa el icono de carpeta/búsqueda -> **Añadir** -> Selecciona el archivo `antos-arm64.vdi` generado.
   * Pulsa **Terminar**.
4. **Captura de Salida Serie UART (Imprescindible en ARM64):**
   * Selecciona la VM creada y pulsa **Configuración** (Settings).
   * Ve a **Puertos serie** -> pestaña **Puerto 1**:
     * Marcar: **Habilitar puerto serie**.
     * Número de puerto: `COM1`.
     * Modo de puerto: Selecciona **Archivo sin formato** (*Raw File*).
     * Ruta del archivo: Escribe `/tmp/antos-serial.log`.
     * *(VirtualBox volcará toda la salida del puerto serie UART PL011 en este archivo)*.

#### Paso 3: Iniciar y Monitorear en Terminal
1. En una terminal de macOS, abre el monitor en tiempo real:
   ```bash
   touch /tmp/antos-serial.log && tail -f /tmp/antos-serial.log
   ```
2. En VirtualBox, pulsa **Iniciar (▶️)**.
3. Si el firmware arranca en la **UEFI Interactive Shell**:
   ```text
   FS0:
   startup.nsh
   ```
4. El kernel arrancará y verás todos los mensajes del sistema fluyendo en tu ventana de terminal.

---

### 5.2 VirtualBox 7 en PCs y Macs Intel/AMD (x86_64)

En arquitectura x86_64 tradicional, la forma más robusta y visual de arrancar es mediante el modo **BIOS MBR clásico**, aprovechando la consola gráfica en pantalla:

#### Paso 1: Generar Imagen BIOS y Convertir a VDI
```bash
# 1. Compilar y generar imagen BIOS x86_64
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format bios

# 2. Convertir a disco VDI
VBoxManage convertfromraw kernel/target/x86_64-unknown-none/debug/antos-bios.img antos-x86.vdi --format VDI
```

#### Paso 2: Configurar la VM x86_64
1. **Crear Máquina Virtual:**
   * Nombre: `antOS-x86_64`.
   * Tipo: `Other` -> Versión: `Other/Unknown (64-bit)`.
   * Hardware: `1024 MB` de RAM, `1` o `2` CPUs.
   * Disco duro: Seleccionar `antos-x86.vdi`.
2. **Configuración de Placa Base:**
   * En **Configuración -> Sistema -> Placa base**:
     * ⚠️ **Asegúrate de que la casilla "Habilitar EFI" esté DESMARCADA**.
3. **Iniciar Máquina:**
   * Pulsa **Iniciar**.
   * El cargador de arranque inicializará el kernel inmediatamente y verás la consola gráfica en pantalla con el logo y registros de antOS.

---

### Resolución de Problemas en VirtualBox

* **Error `BdsDxe: No bootable option or device was found`:**  
  * *En Mac ARM64:* Has montado la imagen en la unidad óptica virtual (CD/DVD) en vez de en el controlador de Disco Duro SATA/SCSI como archivo `.vdi`. VirtualBox busca una estructura ISO9660 con El Torito en unidades ópticas; usa siempre el disco `.vdi`.
  * *En PC Intel x86_64:* Tienes marcada la opción *"Habilitar EFI"* teniendo un disco con imagen BIOS. Desmarca EFI en *Sistema -> Placa base*.
* **Error `UUID already exists` en `VBoxManage convertfromraw`:**  
  Si ya convertiste un disco anteriormente con el mismo nombre o UUID registrado, ejecuta:
  ```bash
  VBoxManage closemedium disk antos-arm64.vdi --delete 2>/dev/null || rm -f antos-arm64.vdi
  ```
  y vuelve a ejecutar la conversión.
* **VirtualBox muestra pantalla negra en Mac ARM64:**  
  El kernel no pinta sobre la pantalla virtual en ARM64; utiliza la UART PL011. Configura el **Puerto serie en modo Archivo sin formato** (`/tmp/antos-serial.log`) y visualízalo con `tail -f /tmp/antos-serial.log`.
* **Se abre un diálogo `DrvTCP#0` / el arranque se detiene en `verificando fuente de temporizador…`:**  
  Síntoma del bug de GIC ya corregido: el kernel se quedaba en GICv2 porque no
  encontraba el DTB de VirtualBox y el *timer* virtual nunca disparaba.
  Actualiza a un binario que parsee **ACPI MADT** (imprime
  `GICv3 … d=0xfcd3_0000` en el log). Si persiste, verifica que la VM expone
  ACPI (EFI habilitado) y recompila desde `master`.
* **El puntero deja un «escalón» de píxeles o va muy lento:**  
  Corregido: el compositor pinta líneas de barrido completas por bandas sobre
  el GOP crudo de VirtualBox. Si lo ves, tu binario es anterior al arreglo del
  compositor; recompila.
* **La pantalla se llena de líneas `input-rx: …`:**  
  Corregido: ese diagnóstico ahora sólo emite el primer evento y luego una
  línea cada 100. Recompila desde `master`.
* **El teclado USB no escribe en el shell:**  
  Corregido (alimentación de todos los puertos raíz + `Root Hub Port` en
  *Configure Endpoint* + clasificación de rol HID por colección `Application`).
  En el log debe aparecer `usb-debug slot N … role=Keyboard` y `roothub …`.
  Como alternativa, añade dispositivos **VirtIO-Input** a la VM.

---

## 6. Comandos Rápidos para QEMU en Terminal

Si prefieres arrancar directamente desde la terminal sin configuraciones de hipervisor gráfico:

> En Apple Silicon `-cpu cortex-a72` implica emulación TCG: compila en
> `--release` (`system/run-arm.sh --release --build-only`) y usa la ruta
> `release/` en los comandos siguientes. El `debug/` sólo es práctico en un
> host x86_64 rápido o en CI.

### A. QEMU ARM64 (AArch64) Bare Metal (Kernel Directo)
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/release/kernel \
  -serial stdio -monitor none
```
*(Para salir de QEMU presiona `Ctrl+A` y luego `X`)*.

Con teclado y ratón/tablet nativos por **VirtIO-Input MMIO** (T28.1) y ventana gráfica:
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 512M \
  -kernel kernel/target/aarch64-unknown-none/release/kernel \
  -device virtio-keyboard-device -device virtio-tablet-device \
  -serial mon:stdio -display cocoa
```
El sufijo `-device` (no `-pci`) coloca los dispositivos en el bus `virtio-mmio` de
`-M virt`, que es donde el kernel los busca (`0x0a00_0000..0x0a00_4000`). El
arranque debe listar `virtio-input N dispositivo(s) de entrada activos` con una
línea por dispositivo (`teclado` / `tablet`).

> ⚠️ En arranque directo (`-kernel`) los dispositivos **PCIe** (`virtio-gpu-pci`,
> `qemu-xhci` + `usb-kbd`/`usb-tablet`) se detectan pero se **omiten**: sin
> firmware nadie asigna sus BAR. Para GPU/USB por PCIe usa el arranque UEFI (6-B)
> o la entrada VirtIO-MMIO de arriba. Forzar GICv3 es opcional
> (`-M virt,gic-version=3`): el kernel también conmuta solo si el bloque GICC no
> responde.

### B. QEMU ARM64 con Firmware UEFI (EDK2)
```bash
system/run-arm.sh --uefi --release --build-only    # genera la .img optimizada

# Copia local escribible del almacén de variables EFI (una vez):
cp /opt/homebrew/share/qemu/edk2-arm-vars.fd /tmp/antos-efivars.fd

qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -drive if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -drive if=pflash,format=raw,file=/tmp/antos-efivars.fd \
  -drive format=raw,file=kernel/target/aarch64-unknown-none/release/antos-uefi-aarch64.img \
  -serial stdio
```
El par `-drive if=pflash` (código de sólo lectura + almacén de variables
escribible) es la forma habitual de cargar EDK2 en `-M virt`. `system/run-arm.sh
--uefi` usa el nombre `QEMU_EFI.fd` (el que instala `apt`); en macOS/Homebrew
ajusta la ruta al `edk2-aarch64-code.fd` de arriba.

> ℹ️ Tras `limine: Loading executable …`, con algunas versiones de QEMU/EDK2 en
> macOS el kernel entrega su salida al **framebuffer GOP** y la consola serie
> queda en silencio: mira la ventana gráfica (o la pestaña de pantalla de UTM),
> no la terminal. Para ver el log por serie de forma fiable usa el Método A.
> Bajo TCG en un M-series el arranque UEFI completo (EDK2 + Limine + kernel)
> tarda bastante; ten paciencia y compila en `--release`.

Añade `-device virtio-gpu-pci -device qemu-xhci -device usb-kbd -device
usb-tablet -display cocoa` para escritorio gráfico con entrada USB: con UEFI los
BAR sí quedan programados y ambos controladores se inicializan.

### C. QEMU x86_64 BIOS (Con Salida Gráfica y Serie)
```bash
qemu-system-x86_64 -m 256M \
  -drive format=raw,file=kernel/target/x86_64-unknown-none/debug/antos-bios.img \
  -serial stdio
```

### D. Pipeline Automatizado de Pruebas de antOS
```bash
# Compila el kernel, empaqueta el disco y ejecuta verificación desatendida en QEMU
./run.sh --test

# Inyecta teclado + ratón por el monitor de QEMU y exige que el kernel los
# acuse (línea "input-rx:" en la consola serie).
./run.sh --test-input --kbd usb            # x86_64
system/run-arm.sh --test-input --gic 3     # AArch64
```

## 7. Matriz de Periféricos: Qué Funciona Dónde (T28.10)

Comando exacto + resultado esperado por combinación. Los scripts parametrizados
(`run.sh`, `system/run-arm.sh`) los generan automáticamente con los *flags*
`--kbd` / `--gpu` / `--gic`.

| Entorno | Comando | Resultado esperado |
| :--- | :--- | :--- |
| **QEMU x86_64 · PS/2** | `./run.sh --kbd ps2 --gpu std` | `ps2-mouse IRQ12 activo`; teclas y ratón mueven el cursor y encolan eventos (`input_events` en `info`). |
| **QEMU x86_64 · USB** | `./run.sh --kbd usb --gpu std` | `usb-xhci controlador activo en PCI …`; `usb-kbd`/`usb-tablet` funcionan en el shell y el compositor. |
| **QEMU `-M virt` GICv2 · VirtIO-Input** | `system/run-arm.sh --gic 2 --kbd virtio --gpu virtio-mmio` | `GICv2`, `timer virtual (CNTV, PPI 27)`, `virtio-input N dispositivos`, framebuffer por VirtIO-GPU MMIO. |
| **QEMU `-M virt` GICv3 · VirtIO-Input** | `system/run-arm.sh --gic 3 --kbd virtio --gpu ramfb` | `GICv3` (bring-up de redistribuidor + `ICC_*_EL1`), framebuffer por `ramfb` (fw_cfg). |
| **QEMU `-M virt` UEFI · USB xHCI + GPU PCIe** | sección 6-B + `-device virtio-gpu-pci -device qemu-xhci -device usb-kbd -device usb-tablet` | `pcie-xhci controlador USB 3.0 activo`, `roothub …`, framebuffer por `virtio-gpu-pci`. Requiere firmware (BAR programados). |
| **QEMU `-M virt` `-kernel` · dispositivos PCIe** | `system/run-arm.sh --kbd usb --gpu virtio-pci` | `pcie-xhci … BAR0 sin asignar · omitido`; `virtio-gpu-pci` cae a `modo headless` (sin pánico); salida por serie. Usar VirtIO-MMIO o el arranque UEFI. |
| **UTM Método A (`-kernel`, `--release`)** | `system/run-arm.sh --release` (o el comando 4-A) | Serie PL011, MMU, timer 100 Hz, EL0 + syscalls. GICv2 o, con `gic-version=3` / autodetección, GICv3. GPU/USB PCIe omitidos. |
| **UTM Método B (imagen UEFI, `--release`)** | `system/run-arm.sh --uefi --release` | GOP de Limine pinta el Desktop Shell; `virtio-gpu-pci` + entrada USB xHCI operativos; sin regresión frente a T27–T28.7. |
| **VirtualBox ARM64** | Método 5.1 (VDI, `--release`) | Arranca a `antos>`; GICv3 por ACPI, teclado **USB xHCI** y ratón operativos, cursor rápido sobre GOP crudo. Ver notas abajo. |
| **VirtualBox x86_64** | Método 5.2 (VDI BIOS) | Teclado y ratón **PS/2**; xHCI opcional si añades un controlador USB 3.0 a la VM. |

### Resuelto en VirtualBox ARM64 (iteración de depuración runtime)

Tras probar antOS sobre VirtualBox 7 ARM64 se corrigieron, con confirmación en
la VM real:

* **GICv3 no se seleccionaba.** VirtualBox ARM64 no expone un DTB alcanzable
  pero sí ACPI. El kernel ahora parsea **MADT** (`RSDP → XSDT → MADT`,
  `parse_madt_gic`) y llama a `gic::set_version(V3, gicd, gicr)` con las bases
  reales (`d=0xfcd3_0000`, `r=0xfcd4_0000`) antes de `gic::init()`. Antes se
  quedaba en GICv2 y el timer virtual no llegaba nunca (colgado en
  `verificando fuente de temporizador…`).
* **El puntero subía unos píxeles / dejaba «descuadre».** El *scanout* del GOP
  crudo de VirtualBox (Non-Cacheable, sin canal de *flush*) no reflejaba
  escrituras parciales estrechas. El compositor pinta ahora **líneas de barrido
  completas** por bandas de daño (`present_rows`) en vez de rectángulos
  angostos, y el estado del cursor se refresca sólo cada ~25 *ticks*.
* **El puntero iba lento.** La ruta rápida `present_best` recompone sólo 3
  bandas (barra de estado + cursor previo + cursor actual), las fusiona y hace
  *full redraw* únicamente si tocan ≥ 2/3 de la pantalla.
* **El teclado USB no funcionaba.** Dos causas: (1) el *Configure Endpoint*
  ponía a cero el *Root Hub Port Number* del Slot Context — se restaura con
  `set_port_info(root_port)`; (2) el teclado colgaba de un puerto sin
  alimentar — ahora se activa `PP` en todos los puertos raíz antes de
  enumerar. Además el rol HID se clasifica por el *usage* de la colección
  `Application`, no por el primer *usage* suelto (un teclado se detectaba como
  ratón).
* **Spam de `input-rx:` en pantalla.** El diagnóstico de recepción de eventos
  sólo imprime el primer evento y luego, si el escritorio no está activo, una
  línea cada 100 eventos.
* **Deadlock tras `framebuffer UEFI (GOP) activo`.** El log `fb-geom`
  re-tomaba el lock de `CONSOLE` dentro de `println!`; ahora toma una
  instantánea de la geometría, suelta el lock y luego imprime.

### Limitaciones conocidas de VirtualBox ARM64 (permanente)

* **Timer:** la IRQ del *timer virtual* (PPI 27, `CNTV_*`) no se dispara. El
  kernel lo detecta al arrancar (`verify_and_fallback`, T28.6) y cae al **timer
  físico EL1** (PPI 30, `CNTP_*`); `uptime_ticks` incrementa a partir de ahí.
* **xHCI *event ring*:** el modelo xHCI de VirtualBox puede no volver a colocar
  *Transfer Events* tras el primero (`ev 0` en `info`, T28.3). `update_erdp`
  limpia `IMAN.IP` en cada avance del *dequeue pointer* y hay un barrido
  PORTSC de reserva para el *hotplug*; el teclado USB funciona, pero si ves
  eventos perdidos prueba VirtIO-Input.
* **Rendimiento:** VirtualBox ARM64 no acelera el kernel de antOS; **compila en
  `--release`** (`system/run-arm.sh --uefi --release`), el `debug` es
  inutilizable.

### Notas de UTM / QEMU en Apple Silicon

* **Rendimiento:** con `-cpu cortex-a72` no hay HVF; todo es TCG. Usa siempre
  `--release`. El `debug` puede tardar más de 40 s sólo en llegar al banner.
* **Arranque directo sin firmware (`-kernel`, UTM Método A):** QEMU `-M virt`
  no ejecuta un asignador de recursos PCI, así que los BAR de las funciones
  PCIe (`qemu-xhci`, `virtio-gpu-pci`) quedan a cero. El kernel los detecta
  (`BAR0 sin asignar`), los omite y continúa por serie. Para GPU y USB usa el
  **Método B (UEFI)**, donde EDK2 programa los BAR.
* **GIC en arranque directo:** sin DTB ni ACPI el kernel asume GICv2; si el
  bloque GICC no responde (máquina `gic-version=3`), sondea `GICC_IIDR` con
  recuperación de fallos y conmuta a GICv3 (`init_v3`) por sí mismo. Fijar
  `-M virt,gic-version=3` en los argumentos es opcional.
* **`system/run-arm.sh --test` / `--test-input`:** el arnés headless compila en
  `debug` por defecto; en un M-series conviene añadir `--release` para que el
  arranque quepa en el *timeout*. En CI (host x86_64) el `debug` es suficiente.
