#!/usr/bin/env bash
# Arranca la máquina syso en QEMU.
#
# Construye dentro de un contenedor Linux (no hace falta Nix en macOS) y
# lanza la VM. Sin KVM la emulación es LENTA: el arranque completo tarda
# bastantes minutos. Con /dev/kvm, segundos.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$PATH:/opt/podman/bin"

echo ">> construyendo la VM (la primera vez descarga el cierre entero)"
podman run --rm -v "$RAIZ:/src" -v syso-nix-store:/nix docker.io/nixos/nix:latest \
  nix --extra-experimental-features "nix-command flakes" \
      build "path:/src#nixosConfigurations.antos-vm.config.system.build.vm" \
      --out-link /nix/vm

echo ">> arrancando · Ctrl-a x para salir de QEMU"
podman run --rm -it -v syso-nix-store:/nix docker.io/nixos/nix:latest \
  sh -c 'cd /tmp && exec /nix/vm/bin/run-*-vm'
