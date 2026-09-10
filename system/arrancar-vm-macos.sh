#!/usr/bin/env bash
# antOS · Construye la ISO en vivo de antOS Linux en un contenedor y la
# arranca en QEMU sobre el host macOS con aceleración HVF.
#
# `arrancar-vm.sh --grafica` corre QEMU DENTRO del contenedor podman, sin
# KVM ni HVF: en Apple Silicon eso es TCG puro y la sesión Wayland no se
# sostiene (greetd entra en bucle de reinicio — ver el hallazgo de T30.6).
# Aquí QEMU corre nativo en macOS y el invitado AArch64 va casi a velocidad
# de host, así que labwc arranca en segundos y aguanta.
#
#   arrancar-vm-macos.sh              construye la ISO si falta y arranca
#   arrancar-vm-macos.sh --rebuild    fuerza reconstruir la ISO
#   arrancar-vm-macos.sh --build-only solo construye y copia la ISO
#   arrancar-vm-macos.sh --headless   serie a stdio, sin ventana (CI/debug)
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO="${RAIZ}/target/antos-linux-aarch64.iso"
VARS="${RAIZ}/target/edk2-aarch64-vars.fd"
export PATH="$PATH:/opt/podman/bin"

MODO="grafica"
REBUILD=0
BUILD_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --rebuild)    REBUILD=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    --headless)   MODO="headless" ;;
    *) echo "arg desconocido: $1" >&2; exit 2 ;;
  esac
  shift
done

# ── requisitos del host ───────────────────────────────────────────────────
if [ "$(uname -s)" != "Darwin" ]; then
  echo "Este guion es para macOS (arranque con HVF). En Linux usa system/arrancar-vm.sh." >&2
  exit 1
fi
if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  echo "Falta 'qemu-system-aarch64'. Instálalo con: brew install qemu" >&2
  exit 1
fi
if ! command -v podman >/dev/null 2>&1; then
  echo "Falta 'podman' (para construir la ISO sin Nix en macOS)." >&2
  exit 1
fi

FW=""
for c in /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
         /usr/local/share/qemu/edk2-aarch64-code.fd; do
  [ -f "$c" ] && { FW="$c"; break; }
done
if [ -z "$FW" ]; then
  echo "No encuentro 'edk2-aarch64-code.fd' de QEMU (¿brew install qemu?)." >&2
  exit 1
fi

# ── construir la ISO en el contenedor ────────────────────────────────────
if [ ! -f "$ISO" ] || [ "$REBUILD" = 1 ]; then
  echo ">> construyendo la ISO de antOS Linux (la primera vez descarga el cierre entero)"
  podman run --rm -v "$RAIZ:/src" -v antos-nix-store:/nix docker.io/nixos/nix:latest \
    nix --extra-experimental-features "nix-command flakes" \
        build "path:/src#iso" --out-link /nix/antos-iso --print-build-logs
  mkdir -p "${RAIZ}/target"
  podman run --rm -v antos-nix-store:/nix -v "${RAIZ}/target:/out" docker.io/nixos/nix:latest \
    sh -c 'cp -L /nix/antos-iso/iso/*.iso /out/antos-linux-aarch64.iso && chmod 644 /out/antos-linux-aarch64.iso'
  echo "✓ ISO: $ISO ($(du -h "$ISO" | cut -f1))"
fi

[ "$BUILD_ONLY" = 1 ] && exit 0

# ── firmware UEFI con variables escribibles (se crea la primera vez) ─────
if [ ! -f "$VARS" ]; then
  # El code.fd de EDK2 para AArch64 es una flash de 64 MiB; el fichero de
  # variables tiene que tener el mismo tamaño.
  dd if=/dev/zero of="$VARS" bs=1m count=64 2>/dev/null
fi

BASE_ARGS=(
  -accel hvf -cpu host -M virt -smp 4 -m 4096
  -drive "if=pflash,format=raw,readonly=on,file=${FW}"
  -drive "if=pflash,format=raw,file=${VARS}"
  -device virtio-net-pci,netdev=net0 -netdev user,id=net0
  -drive "file=${ISO},format=raw,if=virtio,media=cdrom"
)

if [ "$MODO" = "headless" ]; then
  echo ">> arrancando antOS Linux (HVF, headless) · Ctrl-a x para salir"
  exec qemu-system-aarch64 "${BASE_ARGS[@]}" -nographic
fi

echo ">> arrancando antOS Linux en QEMU (HVF) · cierra la ventana para parar"
exec qemu-system-aarch64 "${BASE_ARGS[@]}" \
  -device virtio-gpu-pci -display cocoa,show-cursor=on \
  -device qemu-xhci -device usb-kbd -device usb-tablet
