#!/usr/bin/env bash
# antOS · Pipeline de arranque bare metal x86_64, compilación cruzada y QEMU.
#
# Uso:
#   ./run.sh                       arranque normal (serie + display)
#   ./run.sh --headless            sin display
#   ./run.sh --build-only          solo genera la imagen
#   ./run.sh --test                humo de arranque (verifica el banner)
#   ./run.sh --test-input          humo de arranque + inyección de teclado/ratón
#
# Matriz de periféricos (T28.10):
#   --kbd  ps2|usb                 teclado/ratón: i8042 PS/2 (por defecto) o USB xHCI
#   --gpu  std|virtio-pci|ramfb    salida gráfica (por defecto: std / VGA)
#
# Cada combinación mapea a un comando QEMU canónico documentado en
# docs/manual-de-comandos.md.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE_DIR="debug"
KERNEL="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/kernel"
DISK_IMAGE="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/antos-bios.img"
INITRD_TAR="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/initrd.tar"

MODO="normal"
KBD="ps2"
GPU="std"

while [ $# -gt 0 ]; do
  case "$1" in
    --headless)   MODO="headless" ;;
    --test)       MODO="test" ;;
    --test-input) MODO="test-input" ;;
    --build-only) MODO="build-only" ;;
    --kbd)        KBD="$2"; shift ;;
    --gpu)        GPU="$2"; shift ;;
    --kbd=*)      KBD="${1#*=}" ;;
    --gpu=*)      GPU="${1#*=}" ;;
    *) echo "arg desconocido: $1" >&2; exit 2 ;;
  esac
  shift
done

# ── Device matrix ─────────────────────────────────────────────────────────
DEV_ARGS=()
case "$KBD" in
  ps2) : ;; # i8042 keyboard + PS/2 aux mouse are implicit on -machine pc
  usb) DEV_ARGS+=(-device qemu-xhci -device usb-kbd -device usb-tablet) ;;
  *)   echo "kbd no soportado: $KBD (ps2|usb)" >&2; exit 2 ;;
esac
case "$GPU" in
  std)        DEV_ARGS+=(-vga std) ;;
  virtio-pci) DEV_ARGS+=(-vga none -device virtio-gpu-pci) ;;
  ramfb)      DEV_ARGS+=(-vga none -device ramfb) ;;
  *)          echo "gpu no soportado: $GPU (std|virtio-pci|ramfb)" >&2; exit 2 ;;
esac

echo ">> antOS: compilando kernel no_std para target x86_64-unknown-none..."
(cd "${RAIZ}/kernel" && cargo build)

echo ">> antOS: empaquetando disco de arranque BIOS/MBR con builder..."
(cd "${RAIZ}" && cargo run -q -p builder -- "$KERNEL" > /dev/null)

if [ "$MODO" = "build-only" ]; then
  echo "✓ Imagen generada con éxito: $DISK_IMAGE"
  exit 0
fi

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "Error: 'qemu-system-x86_64' no está instalado en el sistema."
  echo "Instálalo con: brew install qemu (macOS) o sudo apt install qemu-system-x86 (Linux)"
  exit 1
fi

BASE_ARGS=(-m 256M -serial stdio -drive "format=raw,file=${DISK_IMAGE}")
if [ -f "${INITRD_TAR}" ]; then
  BASE_ARGS+=(-drive "file=${INITRD_TAR},format=raw,if=virtio")
fi

if [ "$MODO" = "test" ] || [ "$MODO" = "test-input" ]; then
  echo ">> antOS: prueba automatizada headless (kbd=${KBD} gpu=${GPU} modo=${MODO})..."
  INJECT_INPUT=0
  [ "$MODO" = "test-input" ] && INJECT_INPUT=1
  ANTOS_QEMU_BIN="qemu-system-x86_64" \
  ANTOS_QEMU_ARGS="$(printf '%s\n' "${BASE_ARGS[@]}" -display none "${DEV_ARGS[@]}")" \
  ANTOS_INJECT_INPUT="$INJECT_INPUT" \
  ANTOS_BANNER="antOS · kernel x86_64" \
    python3 "${RAIZ}/system/qemu-smoke.py"
  exit $?
fi

QEMU_ARGS=("${BASE_ARGS[@]}" "${DEV_ARGS[@]}")
[ "$MODO" = "headless" ] && QEMU_ARGS+=(-display none)

echo ">> antOS: arrancando QEMU x86_64 (BIOS legacy · kbd=${KBD} gpu=${GPU})..."
exec qemu-system-x86_64 "${QEMU_ARGS[@]}"
