#!/usr/bin/env bash
# Fase A del smoke de instalación (T36.2). Corre COMO ROOT dentro de la ISO
# en vivo, lanzado por `antos-smoke.service` (services.antos.smoke) con el
# guion recibido por QEMU `fw_cfg`. Habla con el host por la consola serie:
# la última línea es `ANTOS-SMOKE-INSTALL: OK` o `ANTOS-SMOKE-INSTALL: FAIL …`.
set -u -o pipefail

SERIAL=/dev/ttyS0
[ -c /dev/ttyAMA0 ] && SERIAL=/dev/ttyAMA0
say() { echo "$*"; echo "$*" > "$SERIAL"; }
finish() { say "ANTOS-SMOKE-INSTALL: $*"; sync; sleep 2; systemctl poweroff -f || poweroff -f; exit 0; }
fail() { finish "FAIL $*"; }
run() { say "+ $*"; "$@" 2>&1 | tee "$SERIAL"; return "${PIPESTATUS[0]}"; }

FW=/sys/firmware/qemu_fw_cfg/by_name/opt/antos
MODE="$(tr -d '[:space:]' < "$FW/mode/raw" 2>/dev/null || echo clean)"
install -m 0600 "$FW/install.toml/raw" /tmp/install.toml || fail "sin install.toml en fw_cfg"
DEV=/dev/vda
HOST=antos-smoke

say "antos-smoke: fase INSTALL, modo $MODE"
[ -b "$DEV" ] || fail "no existe $DEV"

# ── Dual-boot: preparar una ESP ajena y otra partición ────────────────────
# Reproduce lo que deja otro sistema: una ESP con su cargador (aquí un
# fichero de relleno con el nombre de Windows) y una partición de datos.
# Se guarda el checksum de la ESP ajena para compararlo tras instalar.
if [ "$MODE" = dual ]; then
  run parted -s "$DEV" mklabel gpt \
    mkpart ESP fat32 1MiB 513MiB set 1 esp on \
    mkpart other ext4 513MiB 4609MiB || fail "preparando el disco dual"
  run partprobe "$DEV"; udevadm settle
  run mkfs.vfat -F32 -n OTHER_ESP "${DEV}1" || fail "mkfs.vfat ESP ajena"
  run mkfs.ext4 -q -F -L other "${DEV}2" || fail "mkfs.ext4 partición ajena"
  mkdir -p /mnt/other-esp
  mount "${DEV}1" /mnt/other-esp || fail "montando la ESP ajena"
  mkdir -p /mnt/other-esp/EFI/Microsoft/Boot
  printf 'not a real Windows boot manager (antOS smoke)\n' > /mnt/other-esp/EFI/Microsoft/Boot/bootmgfw.efi
  ESP_SUM_BEFORE="$(cd /mnt/other-esp && find . -type f -path './EFI/Microsoft/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1)"
  umount /mnt/other-esp
  say "ESP ajena preparada; checksum $ESP_SUM_BEFORE"
fi

# ── La instalación real ────────────────────────────────────────────────────
export ANTOS_WORKSPACE=/tmp/antos-smoke-ws ANTOS_STATE=/tmp/antos-smoke-state
mkdir -p "$ANTOS_WORKSPACE" "$ANTOS_STATE"
[ -d /etc/antos/source ] && export ANTOS_SOURCE=/etc/antos/source
run antos install --config /tmp/install.toml --apply || fail "antos install --apply"

# ── Activar el gancho de smoke en el sistema instalado ────────────────────
# El instalador no lo trae (ni debe): se añade `smoke.nix` al `/etc/nixos`
# recién escrito y se vuelve a pasar `nixos-install` — la closure ya está
# en el disco, así que solo re-evalúa y activa.
mkdir -p /mnt/target
mount /dev/disk/by-label/antos-root /mnt/target || fail "montando la raíz instalada"
mkdir -p /mnt/target/boot
if [ "$MODE" = dual ]; then ESP_DEV="${DEV}1"; else ESP_DEV=/dev/disk/by-label/ANTOS_ESP; fi
mount "$ESP_DEV" /mnt/target/boot || fail "montando la ESP"
[ -f /mnt/target/etc/nixos/flake.nix ] || fail "el sistema instalado no tiene /etc/nixos/flake.nix"
cat > /mnt/target/etc/nixos/smoke.nix <<'EOF'
# Añadido por install-smoke.sh (T36.2): gancho fw_cfg para la fase VERIFY.
{ ... }: { services.antos.smoke.enable = true; }
EOF
sed -i 's|imports = \[ ./hardware-configuration.nix \];|imports = [ ./hardware-configuration.nix ./smoke.nix ];|' \
  /mnt/target/etc/nixos/configuration.nix
grep -q 'smoke.nix' /mnt/target/etc/nixos/configuration.nix || fail "no pude añadir smoke.nix a configuration.nix"
run nixos-install --root /mnt/target --flake "/mnt/target/etc/nixos#$HOST" \
  --no-root-passwd --no-channel-copy \
  --override-input antos path:/mnt/target/etc/nixos/antos --no-write-lock-file \
  || fail "segundo nixos-install (gancho de smoke)"

# ── Dual-boot: la ESP ajena sigue intacta y systemd-boot ve a Windows ─────
if [ "$MODE" = dual ]; then
  ESP_SUM_AFTER="$(cd /mnt/target/boot && find . -type f -path './EFI/Microsoft/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1)"
  [ "$ESP_SUM_AFTER" = "$ESP_SUM_BEFORE" ] || fail "la ESP ajena cambió ($ESP_SUM_BEFORE → $ESP_SUM_AFTER)"
  [ -f /mnt/target/boot/EFI/Microsoft/Boot/bootmgfw.efi ] || fail "bootmgfw.efi desapareció"
  [ -b /dev/disk/by-label/other ] || fail "la partición ajena desapareció"
  BOOTCTL="$(bootctl --esp-path=/mnt/target/boot list 2>&1 || true)"
  say "$BOOTCTL"
  echo "$BOOTCTL" | grep -qi 'windows' || fail "bootctl list no muestra a Windows"
  echo "$BOOTCTL" | grep -qi 'nixos\|antos' || fail "bootctl list no muestra a antOS"
fi
ls /mnt/target/boot/EFI/systemd/ >/dev/null 2>&1 || fail "systemd-boot no está en la ESP"

sync
umount -R /mnt/target || fail "umount -R /mnt/target"
finish "OK"
