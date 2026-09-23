#!/usr/bin/env bash
# Verificación end-to-end de `antos install` en QEMU (T36.2).
#
#   system/nixos/install-smoke.sh <antos-linux.iso> [clean|dual]
#
# Fase A — arranca la ISO en vivo con un disco virtual vacío y le pasa por
#          `fw_cfg` `install-smoke-guest-install.sh` + un `install.toml`; el
#          servicio `antos-smoke` del live (services.antos.smoke) lo ejecuta:
#          en `dual` prepara antes una ESP ajena (con un `bootmgfw.efi` de
#          relleno) y otra partición; luego `antos install --config … --apply`;
#          activa el gancho de smoke en el sistema instalado (segundo
#          `nixos-install`, misma closure); en `dual` comprueba que la ESP
#          ajena sigue byte a byte y que `bootctl list` ve a Windows.
# Fase B — arranca desde el disco, SIN la ISO, con
#          `install-smoke-guest-verify.sh`: sesión de `greetd` abierta,
#          `antos-barra` viva, `antos ping` (QueryGitStatus por el socket) y
#          `nixos-rebuild build` sin red.
#
# Cada fase termina cuando el invitado escribe `ANTOS-SMOKE-<FASE>: OK|FAIL`
# por la consola serie y se apaga. Sin red en ninguna fase: si la
# instalación necesita descargar algo, falla, y eso es lo que se quiere
# saber (T36.3 mete la closure y la fuente en la ISO).
#
# La fase B lleva una GPU virtual (`virtio-gpu-pci`): sin un dispositivo DRM
# el compositor de la sesión no arranca y `antos-barra` no llega a existir.
# `-display none` mantiene la VM sin ventana en el anfitrión igualmente.
#
# Variables: SKIP_INSTALL=1 (repite solo la fase B sobre el disco ya
# instalado), ARCH (x86_64|aarch64), WORK (directorio de trabajo), DISK_GIB
# (32), MEM_MIB (4096), TIMEOUT_INSTALL (3600 s), TIMEOUT_VERIFY (900 s),
# OVMF_CODE / OVMF_VARS (firmware UEFI; se buscan en las rutas habituales).
set -euo pipefail

ISO="${1:-${ISO:-}}"
MODE="${2:-${MODE:-clean}}"
ARCH="${ARCH:-x86_64}"
WORK="${WORK:-$(mktemp -d -t antos-smoke.XXXXXX)}"
DISK_GIB="${DISK_GIB:-32}"
MEM_MIB="${MEM_MIB:-4096}"
TIMEOUT_INSTALL="${TIMEOUT_INSTALL:-3600}"
TIMEOUT_VERIFY="${TIMEOUT_VERIFY:-900}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

die() { echo "SMOKE FALLO: $*" >&2; exit 1; }

[ -n "$ISO" ] && [ -r "$ISO" ] || die "uso: $0 <antos-linux.iso> [clean|dual] (ISO no legible: '$ISO')"
case "$MODE" in clean|dual) ;; *) die "modo desconocido '$MODE' (clean|dual)";; esac
case "$ARCH" in x86_64|aarch64) ;; *) die "arquitectura no soportada '$ARCH'";; esac

QEMU="qemu-system-$ARCH"
command -v "$QEMU" >/dev/null || die "$QEMU no está en el PATH"
command -v qemu-img >/dev/null || die "qemu-img no está en el PATH"

# ── Firmware UEFI ──────────────────────────────────────────────────────────
find_first() { for f in "$@"; do [ -r "$f" ] && { echo "$f"; return 0; }; done; return 1; }
if [ "$ARCH" = x86_64 ]; then
  OVMF_CODE="${OVMF_CODE:-$(find_first \
    /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/edk2/x64/OVMF_CODE.4m.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd \
    /opt/homebrew/share/qemu/edk2-x86_64-code.fd /usr/local/share/qemu/edk2-x86_64-code.fd || true)}"
  OVMF_VARS="${OVMF_VARS:-$(find_first \
    /usr/share/OVMF/OVMF_VARS_4M.fd /usr/share/OVMF/OVMF_VARS.fd \
    /usr/share/edk2/ovmf/OVMF_VARS.fd /usr/share/edk2/x64/OVMF_VARS.4m.fd \
    /usr/share/edk2-ovmf/x64/OVMF_VARS.fd \
    /opt/homebrew/share/qemu/edk2-i386-vars.fd /usr/local/share/qemu/edk2-i386-vars.fd || true)}"
else
  OVMF_CODE="${OVMF_CODE:-$(find_first \
    /usr/share/AAVMF/AAVMF_CODE.fd /usr/share/edk2/aarch64/QEMU_EFI-pflash.raw \
    /opt/homebrew/share/qemu/edk2-aarch64-code.fd /usr/local/share/qemu/edk2-aarch64-code.fd || true)}"
  OVMF_VARS="${OVMF_VARS:-$(find_first \
    /usr/share/AAVMF/AAVMF_VARS.fd /usr/share/edk2/aarch64/vars-template-pflash.raw \
    /opt/homebrew/share/qemu/edk2-arm-vars.fd /usr/local/share/qemu/edk2-arm-vars.fd || true)}"
fi
[ -n "$OVMF_CODE" ] && [ -n "$OVMF_VARS" ] || die "no encuentro el firmware UEFI (OVMF_CODE / OVMF_VARS)"

# ── Aceleración ────────────────────────────────────────────────────────────
ACCEL=()
HOST_ARCH="$(uname -m)"
[ "$HOST_ARCH" = arm64 ] && HOST_ARCH=aarch64
if [ "$HOST_ARCH" = "$ARCH" ]; then
  if [ -w /dev/kvm ]; then ACCEL=(-enable-kvm -cpu host)
  elif [ "$(uname -s)" = Darwin ]; then ACCEL=(-accel hvf -cpu host)
  else ACCEL=(-cpu max); fi
else
  ACCEL=(-cpu max)
fi
MACHINE=()
[ "$ARCH" = aarch64 ] && MACHINE=(-M virt)

mkdir -p "$WORK"
DISK="$WORK/disk.qcow2"
VARS="$WORK/vars.fd"
if [ "${SKIP_INSTALL:-0}" = 1 ]; then
  # Reutilizar el disco instalado exige conservar también la NVRAM UEFI: la
  # entrada de arranque al sistema instalado la escribió la fase A ahí.
  [ -f "$DISK" ] && [ -f "$VARS" ] || die "SKIP_INSTALL=1 pero falta $DISK o $VARS"
else
  cp "$OVMF_VARS" "$VARS"
  chmod u+w "$VARS"
  qemu-img create -q -f qcow2 "$DISK" "${DISK_GIB}G"
fi

# ── install.toml para el invitado ──────────────────────────────────────────
CLEAN=true; [ "$MODE" = dual ] && CLEAN=false
NIX_SYSTEM="$ARCH-linux"
cat > "$WORK/install.toml" <<EOF
# Generado por install-smoke.sh ($MODE)
target_device = "/dev/vda"
clean_install = $CLEAN
confirm_wipe = true
target_mount = "/mnt/target"
hostname = "antos-smoke"
username = "antos"
timezone = "UTC"
keymap = "us"
locale = "en_US.UTF-8"
system = "$NIX_SYSTEM"
# password_hash lo añade el invitado con mkpasswd (T36.4)
EOF
echo "$MODE" > "$WORK/mode"

run_phase() { # <nombre> <guion-invitado> <timeout> <log> <args-qemu…>
  local name="$1" guest="$2" timeout="$3" log="$4"; shift 4
  : > "$log"
  echo "── Fase $name ($MODE, $ARCH) · log: $log"
  "$QEMU" "${MACHINE[@]}" "${ACCEL[@]}" -m "$MEM_MIB" -smp 2 \
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" \
    -drive "if=pflash,format=raw,file=$VARS" \
    -drive "file=$DISK,if=virtio,format=qcow2" \
    -fw_cfg "name=opt/antos/smoke,file=$guest" \
    -fw_cfg "name=opt/antos/install.toml,file=$WORK/install.toml" \
    -fw_cfg "name=opt/antos/mode,file=$WORK/mode" \
    -nic none -display none -no-reboot \
    -serial "file:$log" "$@" &
  local pid=$!
  local deadline=$(( $(date +%s) + timeout ))
  while kill -0 "$pid" 2>/dev/null; do
    if grep -q "ANTOS-SMOKE-$name: " "$log" 2>/dev/null; then
      # El invitado ya decidió; le damos 60 s para apagarse.
      local grace=$(( $(date +%s) + 60 ))
      while kill -0 "$pid" 2>/dev/null && [ "$(date +%s)" -lt "$grace" ]; do sleep 1; done
      kill "$pid" 2>/dev/null || true
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      kill "$pid" 2>/dev/null || true
      die "fase $name: sin veredicto en $timeout s (últimas líneas)$(printf '\n'; tail -n 30 "$log")"
    fi
    sleep 5
  done
  wait "$pid" 2>/dev/null || true
  if grep -q "ANTOS-SMOKE-$name: OK" "$log"; then
    echo "   ✓ $name OK"
  else
    die "fase $name: $(grep "ANTOS-SMOKE-$name: " "$log" | tail -n 1)$(printf '\n'; tail -n 40 "$log")"
  fi
}

if [ "${SKIP_INSTALL:-0}" = 1 ]; then
  # Repetir solo la fase B sobre el disco ya instalado: la fase A tarda
  # minutos y el guion de verificación entra por `fw_cfg` en cada arranque,
  # así que iterar sobre ella no exige reinstalar.
  echo "── Fase INSTALL omitida (SKIP_INSTALL=1): se reutiliza $DISK"
else
  run_phase INSTALL "$HERE/install-smoke-guest-install.sh" "$TIMEOUT_INSTALL" "$WORK/serial-install.log" \
    -cdrom "$ISO" -boot d
fi
# La GPU virtual es solo de la fase B: sin un dispositivo DRM el compositor
# de la sesión no arranca y `antos-barra` no llega a existir. En la fase A
# estorba — con GPU el live renderiza Plasma por software y la instalación
# pasó de 3 a más de 40 minutos en la misma máquina. La dirección PCI va
# fijada y alta a propósito: el disco conserva la suya (`0x2`) y la entrada
# de arranque que la fase A escribió en la NVRAM UEFI sigue resolviendo —
# con la GPU en el primer hueco libre, el firmware no encontraba el disco y
# caía a la shell EFI.
run_phase VERIFY "$HERE/install-smoke-guest-verify.sh" "$TIMEOUT_VERIFY" "$WORK/serial-verify.log" \
  -device virtio-gpu-pci,addr=0x9

echo "SMOKE OK ($MODE, $ARCH) · $WORK"
