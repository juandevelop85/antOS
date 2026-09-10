#!/usr/bin/env bash
# antOS · Arnés local para los tests no_std del kernel, x86_64 (T31.16).
#
# Compila el binario de tests (`cargo test --no-run`, arnés custom_test_frameworks
# — ver kernel/src/test_framework.rs), lo empaqueta en una imagen BIOS con el
# `builder` del workspace (el mismo mecanismo que usa `run.sh` para el kernel
# normal) y lo arranca en QEMU headless, vigilando la salida serie con el
# mismo arnés Python que ya usa T28.10 (`system/qemu-smoke.py`) hasta ver
# `KERNEL_TEST_RESULT: PASS` o `KERNEL_TEST_RESULT: FAIL`.
#
# Uso: kernel/run-tests-x86_64.sh
# Salida: 0 si todos los tests pasan, 1 en cualquier fallo o timeout.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo ">> antOS: compilando el arnés de tests del kernel (x86_64-unknown-none)..."
(cd "${RAIZ}/kernel" && cargo test --target x86_64-unknown-none --no-run)

TESTBIN="$(
  cd "${RAIZ}/kernel" && cargo test --target x86_64-unknown-none --no-run --message-format=json 2>/dev/null \
    | python3 -c "
import json, sys
for line in sys.stdin:
    try:
        d = json.loads(line)
    except ValueError:
        continue
    if d.get('reason') == 'compiler-artifact' and d.get('executable'):
        print(d['executable'])
" | tail -n1
)"

if [ -z "$TESTBIN" ]; then
  echo "Error: no se pudo determinar la ruta del binario de tests." >&2
  exit 1
fi
echo ">> antOS: binario de tests en ${TESTBIN}"

echo ">> antOS: empaquetando disco de arranque BIOS/MBR con builder..."
(cd "${RAIZ}" && cargo run -q -p builder -- "$TESTBIN" --format bios > /dev/null)
DISK_IMAGE="$(dirname "$TESTBIN")/antos-bios.img"

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "Error: 'qemu-system-x86_64' no está instalado en el sistema." >&2
  exit 1
fi

echo ">> antOS: arrancando el arnés de tests en QEMU (headless)..."
ANTOS_QEMU_BIN="qemu-system-x86_64" \
ANTOS_QEMU_ARGS="$(printf '%s\n' -m 256M -serial stdio -display none -drive "format=raw,file=${DISK_IMAGE}" -no-reboot)" \
ANTOS_BANNER="KERNEL_TEST_RESULT: PASS" \
ANTOS_BOOT_TIMEOUT="${ANTOS_KERNEL_TEST_TIMEOUT:-30}" \
  python3 "${RAIZ}/system/qemu-smoke.py"
