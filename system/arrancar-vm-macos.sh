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
#   arrancar-vm-macos.sh --fullscreen ventana a pantalla completa (Retina nítido)
#
# Resolución del invitado. La ventana Cocoa de QEMU informa al invitado
# (EDID de `virtio-gpu`) del tamaño de la ventana en puntos, y el escritorio
# adopta ese modo: la resolución del invitado ES el tamaño de la ventana.
#   ANTOS_VM_RES=1440x900   tamaño inicial de la ventana (por defecto)
# En un Mac Retina la ventana se dibuja a 2× (imagen escalada). Con
# `--fullscreen`, Cocoa entrega la resolución nativa (p. ej. 3024×1900) y el
# escritorio pasa solo a escala 2 (`services.antos.desktop.outputScale`):
# es la única forma de ver el escritorio nítido a densidad Retina.
# `zoom-to-fit=on` NO sirve: la ventana arranca pequeña y el invitado la
# sigue (comprobado: se queda en 640×400).
#
# Diagnóstico sin adivinar (modo gráfico): la consola serie del invitado
# queda en el socket `target/antos-vm-serial.sock` (arranque completo + un
# shell autologueado como `nixos`; `nc -U` para entrar) y el monitor de
# QEMU en `target/antos-vm-monitor.sock` (`screendump`, etc.). Sockets
# locales al usuario; los crea QEMU y se borran al relanzar.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO="${RAIZ}/target/antos-linux-aarch64.iso"
VARS="${RAIZ}/target/edk2-aarch64-vars.fd"
export PATH="$PATH:/opt/podman/bin"

MODO="grafica"
REBUILD=0
BUILD_ONLY=0
FULLSCREEN=0
RES="${ANTOS_VM_RES:-1440x900}"
while [ $# -gt 0 ]; do
  case "$1" in
    --rebuild)    REBUILD=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    --headless)   MODO="headless" ;;
    --fullscreen) FULLSCREEN=1 ;;
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
  # Copia a un temporal y `mv` atómico: si hay una VM arrancada desde la ISO
  # anterior, conserva su inodo y sigue funcionando; sobrescribir en sitio
  # le corrompería el disco bajo los pies.
  podman run --rm -v antos-nix-store:/nix -v "${RAIZ}/target:/out" docker.io/nixos/nix:latest \
    sh -c 'cp -L /nix/antos-iso/iso/*.iso /out/antos-linux-aarch64.iso.tmp && chmod 644 /out/antos-linux-aarch64.iso.tmp'
  mv -f "${ISO}.tmp" "$ISO"
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

case "$RES" in
  *x*) XRES="${RES%x*}"; YRES="${RES#*x}" ;;
  *) echo "ANTOS_VM_RES debe ser ANCHOxALTO (p. ej. 1440x900), no '$RES'" >&2; exit 2 ;;
esac
DISPLAY_OPTS="cocoa,show-cursor=on"
[ "$FULLSCREEN" = 1 ] && DISPLAY_OPTS="$DISPLAY_OPTS,full-screen=on"

SERIAL_SOCK="${RAIZ}/target/antos-vm-serial.sock"
MON_SOCK="${RAIZ}/target/antos-vm-monitor.sock"
rm -f "$SERIAL_SOCK" "$MON_SOCK"

echo ">> arrancando antOS Linux en QEMU (HVF) · ${RES}$([ "$FULLSCREEN" = 1 ] && echo ' → pantalla completa') · cierra la ventana para parar"
echo "   consola serie: nc -U ${SERIAL_SOCK}   ·   monitor QEMU: nc -U ${MON_SOCK}"
exec qemu-system-aarch64 "${BASE_ARGS[@]}" \
  -device "virtio-gpu-pci,xres=${XRES},yres=${YRES}" -display "$DISPLAY_OPTS" \
  -device qemu-xhci -device usb-kbd -device usb-tablet \
  -serial "unix:${SERIAL_SOCK},server,nowait" \
  -monitor "unix:${MON_SOCK},server,nowait"
