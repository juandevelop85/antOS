#!/usr/bin/env bash
# antOS Desktop · Wayland Session Launcher
# Ticket: T13.0
# Sets up user environment, populates configuration and starts the Wayland compositor.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Source Wayland desktop environment
if [ -f "${SCRIPT_DIR}/environment" ]; then
    # shellcheck disable=SC1091
    source "${SCRIPT_DIR}/environment"
fi

export ANTOS_WORKSPACE="${WORKSPACE_ROOT}"
export XDG_CONFIG_HOME="${HOME}/.config"

# Prepare Labwc configuration directory
LABWC_CONFIG_DIR="${XDG_CONFIG_HOME}/labwc"
mkdir -p "${LABWC_CONFIG_DIR}"

# Install or sync antOS desktop configuration
cp -f "${SCRIPT_DIR}/rc.xml" "${LABWC_CONFIG_DIR}/rc.xml"
cp -f "${SCRIPT_DIR}/autostart" "${LABWC_CONFIG_DIR}/autostart"
chmod +x "${LABWC_CONFIG_DIR}/autostart"

echo "════════ antOS Desktop (Wayland) ════════"
echo "  Compositor:      Labwc (wlroots)"
echo "  Workspace:       ${ANTOS_WORKSPACE}"
echo "  Config:          ${LABWC_CONFIG_DIR}"
echo "  Atajos clave:    Super+Space (Barra) · Super+A (Agentes) · Super+Return (Terminal)"
echo "─────────────────────────────────────────"

# Check if labwc is available
if ! command -v labwc >/dev/null 2>&1; then
    echo "Aviso: 'labwc' no está instalado en el PATH del sistema."
    echo "Para instalarlo en Linux: sudo apt install labwc (Debian/Ubuntu) o sudo pacman -S labwc (Arch)."
    echo "Entorno y configuraciones generadas en ${LABWC_CONFIG_DIR} exitosamente."
    exit 0
fi

# Launch compositor
if [ "${1:-}" = "--nested" ] || [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; then
    echo ">> Iniciando antOS Desktop en ventana anidada (nested)..."
    exec labwc -s "${LABWC_CONFIG_DIR}/autostart"
else
    echo ">> Iniciando antOS Desktop como sesión Wayland principal..."
    exec labwc -s "${LABWC_CONFIG_DIR}/autostart"
fi
