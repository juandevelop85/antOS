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
