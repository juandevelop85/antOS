#!/usr/bin/env bash
# Compila y verifica antosd dentro de Linux, contra un kernel de verdad.
#
# El recinto de Linux (Landlock) no se puede probar desde macOS: hay que
# ejecutarlo donde vive. Este guion levanta un contenedor, compila dentro y
# ataca el recinto desde fuera y desde dentro.
#
#   ./system/verificar-linux.sh
#
# Requiere podman con una máquina arrancada (`podman machine start`).
set -euo pipefail

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGEN="docker.io/library/rust:slim"

exec podman run --rm -i \
  -v "$RAIZ:/src" \
  -v antos-cargo-registry:/usr/local/cargo/registry \
  -v antos-target-linux:/tmp/tl \
  -w /src "$IMAGEN" bash -s <<'DENTRO'
set -euo pipefail
export NO_COLOR=1
export ANTOS_WORKSPACE=/tmp/ws
export ANTOS_STATE=/tmp/estado
export ANTOS_CAPABILITIES=/src/system/capabilities
S=/tmp/tl/debug/antos

echo "════════ entorno ════════"
echo "  kernel: $(uname -r) $(uname -m)"

echo
echo "════════ compilar y probar ════════"
cargo build -p antosd --target-dir /tmp/tl 2>&1 | tail -3
cargo test -p antosd --target-dir /tmp/tl 2>&1 | tail -4

# Un secreto del usuario, del tipo que una capacidad jamás debería poder leer.
mkdir -p /root/.ssh
echo "-----BEGIN OPENSSH PRIVATE KEY-----FALSA-----" > /root/.ssh/id_rsa

echo
echo "════════ doctor: el recinto se ataca a sí mismo ════════"
$S doctor

echo
echo "════════ leer una clave privada, confinado y sin confinar ════════"
ORDEN='{"changes":[{"tipo":"read","path":"/root/.ssh/id_rsa"}]}'
RECINTO='{"writes":["/tmp/ws"],"reads":[],"network":false}'

echo -n "  sin confinar: "
echo "$ORDEN" | $S __ejecutar | head -c 120; echo
echo -n "  confinado:    "
echo "$ORDEN" | ANTOS_RECINTO="$RECINTO" $S __ejecutar | head -c 200; echo

echo
echo "════════ el bucle completo ════════"
$S -p local -s "crea un proyecto rust llamado demo" 2>&1 | tail -4
$S -p local -s "escribe en notas/diario.txt: probado en Linux" 2>&1 | tail -4
find /tmp/ws | sort | sed 's|^|  |'

echo
echo "════════ deshacer ════════"
$S undo 2>&1 | tail -4
find /tmp/ws | sort | sed 's|^|  |'

echo
echo "════════ bitácora ════════"
$S log
DENTRO
