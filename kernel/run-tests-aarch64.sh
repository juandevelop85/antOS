#!/usr/bin/env bash
# antOS · Arnés local para los tests no_std del kernel, AArch64 (T31.16).
#
# Mismo espíritu que `run-tests-x86_64.sh`, pero sin necesidad de empaquetar
# ninguna imagen de disco: QEMU `-M virt` puede arrancar el ELF crudo del
# kernel directamente vía `-kernel`, igual que ya hace `system/run-arm.sh`
# para el binario normal (T28.10).
#
# Uso: kernel/run-tests-aarch64.sh
# Salida: 0 si todos los tests pasan, 1 en cualquier fallo o timeout.
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo ">> antOS: compilando el arnés de tests del kernel (aarch64-unknown-none)..."
(cd "${RAIZ}/kernel" && cargo test --target aarch64-unknown-none --no-run)

TESTBIN="$(
  cd "${RAIZ}/kernel" && cargo test --target aarch64-unknown-none --no-run --message-format=json 2>/dev/null \
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

if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  echo "Error: 'qemu-system-aarch64' no está instalado en el sistema." >&2
  exit 1
fi

echo ">> antOS: arrancando el arnés de tests en QEMU (headless, -M virt)..."
ANTOS_QEMU_BIN="qemu-system-aarch64" \
ANTOS_QEMU_ARGS="$(printf '%s\n' -M "virt,gic-version=2" -cpu cortex-a72 -m 512M -serial stdio -display none -kernel "${TESTBIN}" -no-reboot)" \
ANTOS_BANNER="KERNEL_TEST_RESULT: PASS" \
ANTOS_BOOT_TIMEOUT="${ANTOS_KERNEL_TEST_TIMEOUT:-30}" \
  python3 "${RAIZ}/system/qemu-smoke.py"
