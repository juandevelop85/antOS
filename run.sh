#!/usr/bin/env bash
# antOS · Pipeline de arranque bare metal, compilación cruzada y QEMU (T13.2).
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE_DIR="debug"
KERNEL="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/kernel"
DISK_IMAGE="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/antos-bios.img"
INITRD_TAR="${RAIZ}/kernel/target/x86_64-unknown-none/${PROFILE_DIR}/initrd.tar"

MODO="normal"

for arg in "$@"; do
  case "$arg" in
    --headless)
      MODO="headless"
      ;;
    --test)
      MODO="test"
      ;;
    --build-only)
      MODO="build-only"
      ;;
  esac
done

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

if [ "$MODO" = "test" ]; then
  echo ">> antOS: ejecutando prueba automatizada de arranque en QEMU (headless)..."
  python3 -c "
import subprocess, sys, re, os

cmd = [
    'qemu-system-x86_64',
    '-m', '256M',
    '-serial', 'stdio',
    '-display', 'none',
    '-drive', 'format=raw,file=${DISK_IMAGE}'
]

if os.path.exists('${INITRD_TAR}'):
    cmd.extend(['-drive', 'file=${INITRD_TAR},format=raw,if=virtio'])

proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
try:
    stdout, stderr = proc.communicate(timeout=7)
except subprocess.TimeoutExpired:
    proc.kill()
    stdout, stderr = proc.communicate()

clean_stdout = re.sub(r'\x1b\[[0-9;]*[a-zA-Z]', '', stdout)

if 'antOS · kernel x86_64' in clean_stdout:
    print('✓ Verificación de arranque exitosa:')
    for line in stdout.splitlines():
        clean_line = re.sub(r'\x1b\[[0-9;]*[a-zA-Z]', '', line)
        if any(marker in clean_line for marker in ['antOS · kernel', 'utilizable', 'interrupciones', 'gdt', 'idt', 'memoria virtual', 'asignador', 'anillo 3', 'consola', 'virtio-blk', 'tarfs', 'vfs', 'cargador', 'el ejecutor toma el control']):
            print('  ' + line)
    sys.exit(0)
else:
    print('Error: no se detectó el banner de arranque de antOS.')
    print('Salida capturada:', stdout[:500])
    sys.exit(1)
"
  exit $?
fi

QEMU_ARGS=(
  -m 256M
  -serial stdio
  -drive "format=raw,file=${DISK_IMAGE}"
)

if [ -f "${INITRD_TAR}" ]; then
  QEMU_ARGS+=(-drive "file=${INITRD_TAR},format=raw,if=virtio")
fi

if [ "$MODO" = "headless" ]; then
  QEMU_ARGS+=(-display none)
fi

echo ">> antOS: arrancando QEMU (BIOS legacy)..."
exec qemu-system-x86_64 "${QEMU_ARGS[@]}"
