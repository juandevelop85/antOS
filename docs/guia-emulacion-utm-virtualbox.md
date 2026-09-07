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
  El kernel bare-metal utiliza el puerto serie UART estándar **PL011** (mapeado en `0x09000000`).  
  > ⚠️ **Punto Crítico en ARM64:**  
  > Por defecto, UTM y VirtualBox abren únicamente una ventana gráfica (*Display*). Si la máquina virtual no tiene configurado un **Puerto Serie (Terminal o archivo de log)**, la ventana gráfica permanecerá en negro o mostrará *"Guest has not initialized the display (yet)"*. **Debes habilitar siempre el puerto serie para ver el arranque de antOS en ARM64.**

* **En x86_64 (Intel / AMD):**  
  antOS incorpora tanto una **Consola Gráfica Framebuffer** en pantalla con fuente bitmap y secuencias ANSI (T23.3) como salida simultánea por puerto serie **COM1** (I/O `0x3F8`).  
  Verás la consola directamente en la pantalla de la máquina virtual y también a través de la terminal serie.

---

## 2. Catálogo de Artefactos de Arranque

antOS soporta dos arquitecturas bare-metal (`no_std`) y genera los siguientes artefactos:

| Arquitectura | Tipo de Archivo | Ruta Relativa | Compatibilidad Recomendada |
| :--- | :--- | :--- | :--- |
| **AArch64 (ARM64)** | Binario ELF | `kernel/target/aarch64-unknown-none/debug/kernel` | **UTM (Método A)** y **QEMU `-kernel`**. |
| **AArch64 (ARM64)** | Disco UEFI GPT | `kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img` | **UTM (Método B)** y **VirtualBox 7 ARM64** (convertido a `.vdi`). |
| **AArch64 (ARM64)** | Live ISO | `kernel/target/aarch64-unknown-none/debug/antos-aarch64.iso` | Medios USB y pruebas ópticas UEFI. |
| **x86_64** | Binario ELF | `kernel/target/x86_64-unknown-none/debug/kernel` | Kernel bare-metal x86_64. |
| **x86_64** | Imagen BIOS MBR | `kernel/target/x86_64-unknown-none/debug/antos-bios.img` | **VirtualBox 7 x86_64 (BIOS clásico)** y **QEMU x86_64**. |
| **x86_64** | Disco UEFI GPT | `kernel/target/x86_64-unknown-none/debug/antos-uefi-x86_64.img` | **UTM x86_64 (UEFI)** y **VirtualBox 7 (con EFI activado)**. |
| **x86_64** | Live ISO | `kernel/target/x86_64-unknown-none/debug/antos-x86_64.iso` | ISO híbrida UEFI/BIOS con Ramdisk live (T24.2). |

---

## 3. Comandos de Compilación y Generación de Imágenes

### A. Para Arquitectura ARM64 (AArch64 - Mac Apple Silicon)

```bash
# 1. Asegurar el target cruzado en Rust
rustup target add aarch64-unknown-none

# 2. Compilar el kernel AArch64 bare-metal
(cd kernel && cargo build --target aarch64-unknown-none)

# 3. Generar el disco GPT con partición ESP FAT32 (/EFI/BOOT/BOOTAA64.EFI) y la Live ISO
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64
```

### B. Para Arquitectura x86_64 (Intel / AMD)

```bash
# 1. Asegurar el target cruzado en Rust
rustup target add x86_64-unknown-none

# 2. Compilar el kernel x86_64 bare-metal
(cd kernel && cargo build)

# 3. Generar imagen BIOS MBR, disco UEFI GPT y Live ISO híbrida
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format all
```

---

## 4. Guía Paso a Paso para UTM 4.x (macOS Apple Silicon e Intel)

UTM en macOS soporta dos motores de virtualización: **Apple Virtualization (VZ)** y **QEMU**.  
Para ejecutar kernels bare-metal experimentales o utilizar arranque directo, **el motor QEMU es el indicado**.

---

### Método A: Arranque Directo del Kernel con QEMU (Recomendado en ARM64)

Este método es el más rápido y directo para desarrollo y pruebas en Macs con chip M1/M2/M3/M4:

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
       2. Valor / siguiente argumento: `/Users/juandevelop/Develop/antOS/kernel/target/aarch64-unknown-none/debug/kernel`
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

### Método B: Arranque con Imagen de Disco UEFI (GPT / Limine)

Si deseas probar la secuencia de arranque completa mediante firmware UEFI EDK2 y el cargador de arranque Limine:

1. **Crear la VM en UTM:**
   * Pulsa **+** -> **Emular** -> **Otro (Other)** -> **Omitir arranque ISO**.
   * Memoria: `1024 MB`, CPU: `2 núcleos`.
   * En almacenamiento: desmarca la creación de disco o crea uno efímero.
2. **Configurar Unidades y Firmware (`Editar VM`):**
   * **Pestaña QEMU:** Asegúrate de que **"UEFI Boot" esté MARCADO**.
   * **Pestaña Unidades (Drives):**
     * Si el asistente creó un disco duro vacío de 64 GB, elimínalo.
     * Pulsa **Nueva unidad...** -> **Importar unidad...** (*Import Drive*).
     * Selecciona el archivo:
       ```text
       <ruta-del-repo>/kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img
       ```
     * En los detalles de la unidad importada, verifica que la **Interfaz** sea `VirtIO` (o `NVMe`).
     * ⚠️ **No marques la unidad como extraíble ni como CD/DVD.**
   * **Pestaña Dispositivos:**
     * Pulsa **Nuevo...** -> **Puerto serie** -> Modo: **Terminal**.
3. **Iniciar y Secuencia de Arranque:**
   * Pulsa **Play**.
   * El firmware EDK2 buscará la partición EFI. Si entra en la **UEFI Interactive Shell**, el script `/startup.nsh` (T24.1) se ejecutará automáticamente tras 5 segundos.
   * Si necesitas ejecutarlo manualmente en la Shell UEFI:
     ```text
     FS0:
     startup.nsh
     ```
   * En la pestaña **Terminal** de UTM verás los mensajes del kernel y el cargador.

---

### Resolución de Problemas en UTM

* **La ventana se queda en negro o dice *"Guest has not initialized the display (yet)"*:**  
  Es el comportamiento esperado en ARM64 porque antOS aún no activa el framebuffer gráfico en ARM, usando la UART PL011. Asegúrate de haber añadido un **Puerto Serie** en modo **Terminal** en la configuración de la VM y haz clic en el icono de terminal de la barra superior.
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
VirtualBox no admite imágenes RAW `.img` directamente en su selector gráfico:
```bash
# 1. Asegurar la compilación y generación de la imagen
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64

# 2. Convertir el disco GPT a disco virtual VDI nativo de VirtualBox
VBoxManage convertfromraw kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img antos-arm64.vdi --format VDI
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

---

## 6. Comandos Rápidos para QEMU en Terminal

Si prefieres arrancar directamente desde la terminal sin configuraciones de hipervisor gráfico:

### A. QEMU ARM64 (AArch64) Bare Metal (Kernel Directo)
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/debug/kernel \
  -serial stdio -monitor none
```
*(Para salir de QEMU presiona `Ctrl+A` y luego `X`)*.

### B. QEMU ARM64 con Firmware UEFI (EDK2)
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -drive format=raw,file=kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img \
  -serial stdio
```

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
```
