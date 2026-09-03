# antOS · Guía de Emulación y Ejecución en UTM, VirtualBox y QEMU 🐜⚡

Esta guía proporciona instrucciones detalladas paso a paso para compilar, generar imágenes de disco arrancables e ISOs de **antOS**, y ejecutarlas con éxito en hipervisores y emuladores gráficos (**UTM** en macOS y **VirtualBox**), así como en **QEMU** directo por terminal.

---

## 📑 Tabla de Contenidos
1. [Catálogo de Artefactos de Arranque](#1-catálogo-de-artefactos-de-arranque)
2. [Comandos de Generación de Imágenes](#2-comandos-de-generación-de-imágenes)
3. [Guía Paso a Paso para UTM (macOS Apple Silicon e Intel)](#3-guía-paso-a-paso-para-utm-macos-apple-silicon-e-intel)
   - [Método A: Arranque Directo del Kernel (Recomendado)](#método-a-arranque-directo-del-kernel-recomendado)
   - [Método B: Arranque con Imagen de Disco UEFI](#método-b-arranque-con-imagen-de-disco-uefi)
   - [Resolución de Problemas en UTM (Pantalla Negra o Shell UEFI)](#resolución-de-problemas-en-utm-pantalla-negra-o-shell-uefi)
4. [Guía Paso a Paso para VirtualBox](#4-guía-paso-a-paso-para-virtualbox)
   - [4.1 VirtualBox en macOS Apple Silicon (ARM64 / AArch64)](#41-virtualbox-en-macos-apple-silicon-arm64--aarch64)
   - [4.2 VirtualBox en PCs y Macs Intel (x86_64)](#42-virtualbox-en-pcs-y-macs-intel-x86_64)
   - [Resolución de Problemas en VirtualBox (`BdsDxe: No bootable option`)](#resolución-de-problemas-en-virtualbox-bdsdxe-no-bootable-option)
5. [Comandos Rápidos para QEMU en Terminal](#5-comandos-rápidos-para-qemu-en-terminal)

---

## 1. Catálogo de Artefactos de Arranque

antOS soporta dos arquitecturas bare-metal (`no_std`) y produce los siguientes artefactos:

| Arquitectura | Tipo de Archivo | Ruta Relativa | Descripción |
| :--- | :--- | :--- | :--- |
| **AArch64 (ARM64)** | Binario ELF | `kernel/target/aarch64-unknown-none/debug/kernel` | Kernel bare-metal para QEMU virt o arranque directo. |
| **AArch64 (ARM64)** | Disco UEFI | `kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img` | Disco GPT con partición ESP FAT32 (`/EFI/BOOT/BOOTAA64.EFI`). |
| **AArch64 (ARM64)** | Live ISO | `kernel/target/aarch64-unknown-none/debug/antos-aarch64.iso` | ISO híbrida UEFI GPT para grabación en USB o medios ópticos. |
| **x86_64** | Binario ELF | `kernel/target/x86_64-unknown-none/debug/kernel` | Kernel bare-metal x86_64. |
| **x86_64** | Imagen BIOS | `kernel/target/x86_64-unknown-none/debug/antos-bios.img` | Imagen de disco MBR para BIOS clásica (ideal para VirtualBox x86_64). |
| **x86_64** | Disco UEFI | `kernel/target/x86_64-unknown-none/debug/antos-uefi-x86_64.img` | Disco GPT con partición ESP FAT32 (`/EFI/BOOT/BOOTX64.EFI`). |

---

## 2. Comandos de Generación de Imágenes

### A. Para Arquitectura ARM64 (AArch64)

```bash
# 1. Asegurar el target de compilación
rustup target add aarch64-unknown-none

# 2. Compilar el kernel AArch64
cargo build --target aarch64-unknown-none --manifest-path kernel/Cargo.toml

# 3. Generar la imagen UEFI (.img) y la Live ISO híbrida (.iso)
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64

# O mediante el CLI de antOS:
cargo run --bin antos -- boot iso --arch aarch64
```

### B. Para Arquitectura x86_64

```bash
# 1. Asegurar el target de compilación
rustup target add x86_64-unknown-none

# 2. Compilar el kernel y generar imágenes BIOS y UEFI
cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format all
```

---

## 3. Guía Paso a Paso para UTM (macOS Apple Silicon e Intel)

> ⚠️ **Punto Crítico en UTM:**  
> El kernel bare-metal de antOS envía toda su telemetría y mensajes por el **puerto serie UART (PL011 en ARM / COM1 en x86)**.  
> En UTM, por defecto solo se crea una ventana gráfica (*Display*), por lo que verás la pantalla en negro a menos que añadas un **Puerto Serie (Terminal)**.

---

### Método A: Arranque Directo del Kernel (Recomendado)

Este método es el más rápido, directo y limpio para desarrollo:

1. **Crear Máquina Virtual:**
   * Abre UTM y pulsa el botón **+** (Crear nueva máquina virtual).
   * Selecciona **Virtualizar** (si tienes Mac con Apple Silicon M1/M2/M3/M4) o **Emular** (si deseas ejecutar ARM64 en Mac Intel).
   * Selecciona **Otro (Other)**.
2. **Configuración del Sistema (`Editar VM`):**
   * Pestaña **Sistema**:
     * **Arquitectura:** `ARM64 (aarch64)`.
     * **Sistema / Máquina:** `QEMU 7.x / 8.x / 9.x ARM Virtual Machine (virt)`.
     * **Memoria RAM:** `512 MB` o `1024 MB`.
   * Pestaña **QEMU**:
     * **Desmarcar** la casilla *"UEFI Boot"*.
   * Pestaña **Dispositivos**:
     * Pulsa **Nuevo...** -> **Puerto serie**.
     * Modo: **Terminal** (o Emulado).
     * *(Opcional)*: Puedes eliminar la pantalla ("Display") para que abra directamente la terminal serie.
   * Sección **Arranque / Kernel**:
     * En el campo **Kernel**, pulsa *Explorar...* y selecciona:
       ```text
       <ruta-del-repo>/kernel/target/aarch64-unknown-none/debug/kernel
       ```
3. **Iniciar Máquina Virtual:**
   * Pulsa **Play**.
   * Se abrirá la ventana de UTM con la pestaña **Terminal**, mostrando el arranque en tiempo real:
     ```text
     antOS · kernel AArch64
     ═══════════════════════
     arranque
       arquitectura AArch64 (ARM 64-bit)
     ...
     espacio de usuario (EL0) y llamadas al sistema (SVC)
       userspace    ¡saludo desde espacio de usuario (EL0) via svc #0!
       retorno      el programa EL0 finalizó limpiamente con código de salida 42
     ```

---

### Método B: Arranque con Imagen de Disco UEFI

Si deseas arrancar mediante la secuencia de firmware UEFI estándar:

1. **Configuración de Unidades:**
   * En UTM -> **Editar VM** -> **Unidades**:
   * Pulsa **Nuevo...** -> **Imagen de disco** (⚠️ **No** la agregues como CD/DVD).
   * **Interfaz:** Selecciona `VirtIO` o `NVMe`.
   * Pulsa **Importar...** y selecciona:
     ```text
     <ruta-del-repo>/kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img
     ```
2. **Habilitar Puerto Serie:**
   * En **Dispositivos**, asegúrate de tener añadido un **Puerto serie** (modo Terminal).
3. **Iniciar y Navegar en la UEFI Shell:**
   * Si la máquina entra a la **UEFI Interactive Shell**, escribe:
     ```text
     map -r
     FS0:
     cd EFI\BOOT
     BOOTAA64.EFI
     ```
   * En la pestaña **Terminal** verás la ejecución del kernel de antOS.

---

### Resolución de Problemas en UTM (Pantalla Negra o Shell UEFI)

* **¿Por qué la pantalla gráfica está en negro?**  
  El kernel bare-metal aún no inicializa el framebuffer gráfico; utiliza el puerto serie PL011. Asegúrate de añadir el dispositivo *Puerto Serie* en la configuración de UTM y cambiar a la pestaña *Terminal*.
* **¿Por qué dice `'antos' is not recognized` en la Shell UEFI?**  
  El comando `antos` es el binario del sistema operativo de usuario. Dentro de la Shell UEFI estás a nivel de firmware previo al SO. Debes ejecutar el archivo EFI: `FS0:\EFI\BOOT\BOOTAA64.EFI`.
* **¿Por qué no aparece `FS0:` en la Shell UEFI?**  
  Si la unidad fue montada como lector de CD/DVD en vez de disco duro VirtIO/NVMe, el firmware EDK II no reconocerá la tabla GPT. Cambia la interfaz de la unidad a `VirtIO Drive` o `NVMe`.

---

## 4. Guía Paso a Paso para VirtualBox

VirtualBox 7 en macOS con procesadores Apple Silicon (M1/M2/M3/M4) **solo permite crear máquinas virtuales ARM64**, y utiliza exclusivamente **firmware UEFI ARM64 (EDK2)** ya que en hardware ARM no existe la BIOS clásica x86.

---

### 4.1 VirtualBox en macOS Apple Silicon (ARM64 / AArch64)

Para arrancar antOS en VirtualBox 7 sobre macOS ARM:

#### Paso 1: Generar la Imagen UEFI ARM64 y Convertir a VDI
```bash
# 1. Compilar el kernel para AArch64
cargo build --target aarch64-unknown-none --manifest-path kernel/Cargo.toml

# 2. Generar el disco GPT con partición ESP FAT32 (/EFI/BOOT/BOOTAA64.EFI)
cargo run -p builder -- kernel/target/aarch64-unknown-none/debug/kernel --arch aarch64

# 3. Convertir la imagen RAW generada a disco virtual VDI nativo
VBoxManage convertfromraw kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img antos-arm64.vdi --format VDI
```

#### Paso 2: Configurar la Máquina Virtual en VirtualBox
1. **Crear Máquina:**
   * Nombre: `antOS-ARM64`.
   * Tipo: `Linux` o `Other`.
   * Versión: `Linux (ARM 64-bit)` o `Other (ARM 64-bit)`.
   * Memoria RAM: `1024 MB` o `2048 MB`.
   * Procesadores: `2 CPUs`.
2. **Sistema:**
   * En VirtualBox para Mac, el firmware **EFI viene activado por defecto y es obligatorio** para ARM64.
3. **Almacenamiento (Crucial):**
   * En el controlador de almacenamiento (SATA o VirtIO-SCSI):
   * Pulsa **Añadir Disco Duro** -> Selecciona el archivo convertido `antos-arm64.vdi`.
   * ⚠️ **Importante:** **No** utilices la unidad óptica virtual (lector de CD/DVD) para cargar la imagen `.iso`, ya que VirtualBox busca firmas ISO9660 en CD-ROMs y no reconocerá la tabla GPT.
4. **Puertos Serie (Para capturar los mensajes del kernel):**
   * Ve a **Puertos serie** -> **Puerto 1**:
   * Marcar *"Habilitar puerto serie"*.
   * Número de puerto: `COM1`.
   * Modo: *"Archivo sin formato (Raw File)"* y elige una ruta (ej. `/tmp/antos-serial.log`) o *"Tubería de host"*.
   * *(El kernel de antOS en ARM64 envía su salida por el puerto serie UART PL011)*.

#### Paso 3: Iniciar y Arrancar
1. Inicia la máquina virtual.
2. Si el firmware arranca en la **UEFI Interactive Shell**, escribe:
   ```text
   map -r
   FS0:
   cd EFI\BOOT
   BOOTAA64.EFI
   ```
   *(También puedes escribir directamente `startup.nsh` en la Shell)*.
3. El kernel arrancará y todos sus mensajes quedarán registrados en el archivo de log serie configurado.

---

### 4.2 VirtualBox en PCs y Macs Intel (x86_64)

En computadores con arquitectura x86_64, VirtualBox funciona de manera óptima mediante arranque **BIOS clásico (MBR)**:

1. **Compilar y convertir a VDI:**
   ```bash
   # Generar imagen BIOS x86_64
   cargo run -p builder -- kernel/target/x86_64-unknown-none/debug/kernel --format bios

   # Convertir a VDI
   VBoxManage convertfromraw kernel/target/x86_64-unknown-none/debug/antos-bios.img antos-x86.vdi --format VDI
   ```
2. **Configuración de la VM:**
   * Tipo: `Other` -> `Other/Unknown (64-bit)`.
   * **Sistema -> Placa base:** **Desmarcar** la casilla *"Habilitar EFI (solo SO especiales)"*.
   * **Almacenamiento:** Añadir `antos-x86.vdi` como disco duro SATA/IDE.
   * Iniciar la VM: el cargador MBR cargará el kernel inmediatamente en pantalla.

---

### Resolución de Problemas en VirtualBox (`BdsDxe: No bootable option`)

Si ves este error en VirtualBox:
```text
BdsDxe: failed to load Boot0001 "UEFI VBOX HARDDISK " : Not Found
BdsDxe: No bootable option or device was found.
```
* **Causa 1 (En Mac ARM64):** El disco fue montado como lector de CD/DVD en lugar de disco duro VDI, o no se encontró el archivo EFI en la ruta esperada.  
  👉 **Solución:** Asigna el archivo convertido `antos-arm64.vdi` como **Disco Duro** en el controlador SATA/SCSI, o entra a la UEFI Shell y escribe `FS0:` y luego `\EFI\BOOT\BOOTAA64.EFI`.
* **Causa 2 (En Mac Intel / PC x86_64):** Tienes la casilla *"Habilitar EFI"* activada con una imagen BIOS.  
  👉 **Solución:** Desactiva *"Habilitar EFI"* en *Configuración -> Sistema -> Placa base* para arrancar en modo BIOS clásico.

---

## 5. Comandos Rápidos para QEMU en Terminal

Si prefieres emular directamente desde la terminal sin instalar aplicaciones gráficas adicionales:

### A. QEMU ARM64 (AArch64) Bare Metal
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -kernel kernel/target/aarch64-unknown-none/debug/kernel \
  -serial stdio -monitor none
```
*(Para salir de QEMU presiona `Ctrl+A` y luego `X`)*.

### B. QEMU ARM64 con Firmware UEFI (EDK2)
```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic \
  -bios QEMU_EFI.fd \
  -drive format=raw,file=kernel/target/aarch64-unknown-none/debug/antos-uefi-aarch64.img \
  -serial stdio
```

### C. QEMU x86_64 BIOS
```bash
qemu-system-x86_64 -m 256M \
  -drive format=raw,file=kernel/target/x86_64-unknown-none/debug/antos-bios.img \
  -serial stdio
```

### D. Script Automático de antOS
```bash
# Ejecuta compilación y prueba desatendida
./run.sh --test
```
