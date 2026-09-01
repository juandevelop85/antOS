#!/usr/bin/env bash
# Compila la barra de intención dentro de Linux.
#
# GTK4 y layer-shell no existen en macOS —layer-shell es un protocolo de
# Wayland— así que este crate no se puede ni compilar en el anfitrión.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$PATH:/opt/podman/bin"

exec podman run --rm -v "$RAIZ:/src" -v syso-nix-store:/nix docker.io/nixos/nix:latest \
  nix --extra-experimental-features "nix-command flakes" \
      develop "path:/src#barra" --command \
      env CARGO_BUILD_JOBS=1 \
      cargo build --manifest-path /src/system/barra/Cargo.toml \
                  --target-dir /nix/barra-target
