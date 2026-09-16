#!/usr/bin/env bash
# Arranca la máquina virtual de antOS en QEMU.
#
# Construye dentro de un contenedor Linux (no hace falta Nix en macOS) y
# lanza la VM. Sin KVM la emulación es LENTA: el arranque completo tarda
# bastantes minutos. Con /dev/kvm, segundos.
#
#   arrancar-vm.sh              headless: consola serie (CI, arranque rápido)
#   arrancar-vm.sh --grafica    escritorio antOS (Labwc + antos-barra) por VNC
#
# El modo --grafica arranca dentro del contenedor con `-display vnc`: conéctate
# con un cliente VNC a localhost:5901. Para una ventana nativa en el host,
# construye la imagen y ejecuta QEMU en macOS (ver docs/manual-de-comandos.md).
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$PATH:/opt/podman/bin"

MODO="headless"
[ "${1:-}" = "--grafica" ] && MODO="grafica"

if [ "$MODO" = "grafica" ]; then
  TARGET="antos-desktop-vm"
  echo ">> construyendo la VM GRÁFICA de antOS (la primera vez descarga el cierre entero)"
else
  TARGET="antos-vm"
  echo ">> construyendo la VM de antOS (la primera vez descarga el cierre entero)"
fi

podman run --rm -v "$RAIZ:/src" -v antos-nix-store:/nix docker.io/nixos/nix:latest \
  nix --extra-experimental-features "nix-command flakes" \
      build "git+file:///src#nixosConfigurations.${TARGET}.config.system.build.vm" \
      --out-link /nix/vm

if [ "$MODO" = "grafica" ]; then
  echo ">> arrancando el escritorio antOS · VNC en localhost:5901 · Ctrl-c aquí para parar"
  podman run --rm -it -p 5901:5901 -v antos-nix-store:/nix docker.io/nixos/nix:latest \
    sh -c 'cd /tmp && exec /nix/vm/bin/run-*-vm -display vnc=0.0.0.0:1 -vga none'
else
  echo ">> arrancando antOS VM · Ctrl-a x para salir de QEMU"
  podman run --rm -it -v antos-nix-store:/nix docker.io/nixos/nix:latest \
    sh -c 'cd /tmp && exec /nix/vm/bin/run-*-vm'
fi
