#!/usr/bin/env bash
# Compila el kernel, genera la imagen de disco y arranca QEMU.
set -euo pipefail

PROFILE_DIR="debug"
KERNEL="target/x86_64-unknown-none/${PROFILE_DIR}/kernel"

echo ">> compilando el kernel para x86_64-unknown-none"
cargo build -p kernel --target x86_64-unknown-none

echo ">> generando imagen de disco"
cargo run -q -p builder -- "$KERNEL" > /dev/null

QEMU_ARGS=(
  -m 256M
  # Redirige el puerto serie del invitado a esta terminal. Todavía no
  # escribimos nada por ahí, pero lo necesitaremos en la Fase 1.
  -serial stdio
)

QEMU_ARGS+=(-drive "format=raw,file=target/x86_64-unknown-none/${PROFILE_DIR}/syso-bios.img")

echo ">> arrancando QEMU (BIOS)"
exec qemu-system-x86_64 "${QEMU_ARGS[@]}"
