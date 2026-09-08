#!/usr/bin/env bash
# antOS · Arranque bare-metal AArch64 en QEMU `-M virt` (T28.10).
#
# Uso:
#   system/run-arm.sh                     arranque directo (-kernel), serie
#   system/run-arm.sh --uefi              arranque por imagen UEFI (Limine)
#   system/run-arm.sh --release           compila optimizado (mucho más rápido
#                                         bajo emulación sin aceleración, p.ej.
#                                         VirtualBox ARM)
#   system/run-arm.sh --build-only        solo compila
#   system/run-arm.sh --test              humo de arranque
#   system/run-arm.sh --test-input        humo de arranque + inyección de entrada
#
# Matriz de periféricos:
#   --gic  2|3                            GICv2 (por defecto) o GICv3
#   --kbd  virtio|usb                     VirtIO-Input MMIO (por defecto) o USB xHCI
#   --gpu  none|virtio-mmio|virtio-pci|ramfb   salida gráfica (por defecto: none)
#
# Cada combinación mapea a un comando QEMU canónico documentado en
# docs/manual-de-comandos.md y docs/guia-emulacion-utm-virtualbox.md.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MODO="normal"
BOOT="kernel"
GIC="2"
KBD="virtio"
GPU="none"
PROFILE="debug"
CARGO_PROFILE_FLAG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --uefi)        BOOT="uefi" ;;
    --release)     PROFILE="release"; CARGO_PROFILE_FLAG="--release" ;;
    --build-only)  MODO="build-only" ;;
    --test)        MODO="test" ;;
    --test-input)  MODO="test-input" ;;
    --gic)  GIC="$2"; shift ;;
    --kbd)  KBD="$2"; shift ;;
    --gpu)  GPU="$2"; shift ;;
    --gic=*) GIC="${1#*=}" ;;
    --kbd=*) KBD="${1#*=}" ;;
    --gpu=*) GPU="${1#*=}" ;;
    *) echo "arg desconocido: $1" >&2; exit 2 ;;
  esac
  shift
done

TARGET_DIR="${RAIZ}/kernel/target/aarch64-unknown-none/${PROFILE}"
KERNEL_ELF="${TARGET_DIR}/kernel"
INITRD_TAR="${TARGET_DIR}/initrd.tar"
UEFI_IMAGE="${TARGET_DIR}/antos-uefi-aarch64.img"

echo ">> antOS: compilando kernel para aarch64-unknown-none (${PROFILE})..."
if [ "$BOOT" = "uefi" ]; then
  (cd "${RAIZ}/kernel" && cargo build --target aarch64-unknown-none --features limine ${CARGO_PROFILE_FLAG})
  echo ">> antOS: generando imagen UEFI AArch64..."
  (cd "${RAIZ}" && cargo run -q -p builder -- "$KERNEL_ELF" aarch64 > /dev/null)
else
  (cd "${RAIZ}/kernel" && cargo build --target aarch64-unknown-none ${CARGO_PROFILE_FLAG})
fi

if [ "$MODO" = "build-only" ]; then
  echo "✓ kernel: $KERNEL_ELF"
  exit 0
fi

if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  echo "Error: 'qemu-system-aarch64' no está instalado (brew install qemu / apt install qemu-system-arm)."
  exit 1
fi

# ── Machine + device matrix ──────────────────────────────────────────────
MACHINE="virt,gic-version=${GIC}"
DEV_ARGS=()
case "$KBD" in
  virtio) DEV_ARGS+=(-device virtio-keyboard-device -device virtio-tablet-device) ;;
  usb)    DEV_ARGS+=(-device qemu-xhci -device usb-kbd -device usb-tablet) ;;
  *)      echo "kbd no soportado: $KBD (virtio|usb)" >&2; exit 2 ;;
esac
case "$GPU" in
  none)        : ;;
  virtio-mmio) DEV_ARGS+=(-device virtio-gpu-device) ;;
  virtio-pci)  DEV_ARGS+=(-device virtio-gpu-pci) ;;
  ramfb)       DEV_ARGS+=(-device ramfb) ;;
  *)           echo "gpu no soportado: $GPU (none|virtio-mmio|virtio-pci|ramfb)" >&2; exit 2 ;;
esac

BASE_ARGS=(-M "$MACHINE" -cpu cortex-a72 -m 512M -serial stdio)
if [ "$BOOT" = "uefi" ]; then
  BASE_ARGS+=(-bios QEMU_EFI.fd -drive "format=raw,file=${UEFI_IMAGE}")
else
  BASE_ARGS+=(-kernel "$KERNEL_ELF")
  [ -f "$INITRD_TAR" ] && BASE_ARGS+=(-initrd "$INITRD_TAR")
fi

if [ "$MODO" = "test" ] || [ "$MODO" = "test-input" ]; then
  echo ">> antOS: prueba headless AArch64 (gic=${GIC} kbd=${KBD} gpu=${GPU} modo=${MODO})..."
  INJECT_INPUT=0
  [ "$MODO" = "test-input" ] && INJECT_INPUT=1
  ANTOS_QEMU_BIN="qemu-system-aarch64" \
  ANTOS_QEMU_ARGS="$(printf '%s\n' "${BASE_ARGS[@]}" -display none "${DEV_ARGS[@]}")" \
  ANTOS_INJECT_INPUT="$INJECT_INPUT" \
  ANTOS_BANNER="antOS · kernel AArch64" \
  ANTOS_BOOT_TIMEOUT="40" \
    python3 "${RAIZ}/system/qemu-smoke.py"
  exit $?
fi

echo ">> antOS: arrancando QEMU AArch64 (${MACHINE} · kbd=${KBD} gpu=${GPU} · ${PROFILE})..."
if [ "$GPU" = "none" ]; then
  # Solo serie.
  exec qemu-system-aarch64 "${BASE_ARGS[@]}" -nographic "${DEV_ARGS[@]}"
else
  # Con GPU: ventana gráfica + serie/monitor multiplexados en la terminal.
  # -serial ya está en BASE_ARGS como 'stdio'; se sustituye por 'mon:stdio'.
  GRAPHIC_ARGS=()
  for a in "${BASE_ARGS[@]}"; do
    if [ "$a" = "stdio" ]; then GRAPHIC_ARGS+=("mon:stdio"); else GRAPHIC_ARGS+=("$a"); fi
  done
  exec qemu-system-aarch64 "${GRAPHIC_ARGS[@]}" -display default,show-cursor=on "${DEV_ARGS[@]}"
fi
