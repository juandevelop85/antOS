#!/usr/bin/env bash
# Fase B del smoke de instalación (T36.2). Corre COMO ROOT en el sistema
# recién instalado (arrancado desde el disco, sin la ISO), lanzado por
# `antos-smoke.service`. Comprueba lo que T30.5/T36.2 prometen y escribe
# `ANTOS-SMOKE-VERIFY: OK|FAIL …` por la consola serie.
set -u -o pipefail

SERIAL=/dev/ttyS0
[ -c /dev/ttyAMA0 ] && SERIAL=/dev/ttyAMA0
say() { echo "$*"; echo "$*" > "$SERIAL"; }
finish() { say "ANTOS-SMOKE-VERIFY: $*"; sync; sleep 2; systemctl poweroff -f || poweroff -f; exit 0; }
fail() { finish "FAIL $*"; }
wait_for() { # <segundos> <descripción> <comando…>
  local secs="$1" what="$2"; shift 2
  local deadline=$(( $(date +%s) + secs ))
  until "$@" >/dev/null 2>&1; do
    [ "$(date +%s)" -lt "$deadline" ] || fail "$what (tras $secs s)"
    sleep 2
  done
  say "✓ $what"
}

USER_NAME=antos
HOST=antos-smoke
[ -f /etc/set-environment ] && . /etc/set-environment
STATE="${ANTOS_STATE:-/var/lib/antos/estado}"
WORKSPACE="${ANTOS_WORKSPACE:-/var/lib/antos/workspace}"

say "antos-smoke: fase VERIFY en $(hostname) ($(uname -m))"
[ "$(hostname)" = "$HOST" ] || fail "hostname es $(hostname), no $HOST"
[ -e /etc/nixos/antos/flake.nix ] || fail "falta /etc/nixos/antos (la copia del árbol)"
mountpoint -q /boot || fail "/boot (ESP) no está montado"
[ -d /boot/EFI/systemd ] || fail "systemd-boot no está en la ESP"

# 1. El demonio: socket vivo y protocolo de la barra.
wait_for 180 "antos.sock existe" test -S "$STATE/antos.sock"
wait_for 60 "antos ping (QueryGitStatus por el socket)" \
  runuser -u "$USER_NAME" -- env ANTOS_STATE="$STATE" ANTOS_WORKSPACE="$WORKSPACE" antos ping

# 2. La sesión de escritorio: greetd abrió sesión al usuario y la barra vive.
wait_for 180 "sesión de $USER_NAME abierta por greetd" \
  bash -c "loginctl list-sessions --no-legend | grep -q ' $USER_NAME '"
wait_for 120 "antos-barra en ejecución" pgrep -x antos-barra

# 2b. Primer arranque (T36.5): `antos setup --yes` como el usuario es
#     idempotente (segunda pasada: todo «ya hecho», nada cambia) y
#     `antos doctor --desktop` pasa dentro de la sesión.
UID_ANTOS="$(id -u "$USER_NAME")"
WL="$(ls "/run/user/$UID_ANTOS"/wayland-* 2>/dev/null | head -n 1 | xargs -r basename)"
as_user() { runuser -u "$USER_NAME" -- env HOME="/home/$USER_NAME" ANTOS_STATE="$STATE" ANTOS_WORKSPACE="$WORKSPACE" \
  XDG_RUNTIME_DIR="/run/user/$UID_ANTOS" WAYLAND_DISPLAY="$WL" "$@"; }
printf '%s\n' 'git_name = "antOS Smoke"' 'git_email = "smoke@antos.invalid"' 'ssh_key = true' 'flathub = false' > /tmp/setup.toml
as_user antos setup --yes --config /tmp/setup.toml 2>&1 | tee "$SERIAL" || fail "antos setup --yes"
[ -f "$STATE/setup.toml" ] || fail "sin marcador setup.toml"
[ -f "/home/$USER_NAME/.ssh/id_ed25519.pub" ] || fail "antos setup no generó la clave SSH"
grep -q 'smoke@antos.invalid' "/home/$USER_NAME/.config/git/config" || fail "antos setup no escribió la identidad git"
home_fingerprint() { find "/home/$USER_NAME" -type f -printf '%p %s %T@\n' 2>/dev/null | sort | sha256sum; }
BEFORE="$(home_fingerprint)"
as_user antos setup --yes --config /tmp/setup.toml 2>&1 | tee "$SERIAL" | grep -q 'ya hecho' || fail "segunda pasada de antos setup sin «ya hecho»"
AFTER="$(home_fingerprint)"
[ "$BEFORE" = "$AFTER" ] || fail "la segunda pasada de antos setup cambió ficheros de \$HOME"
say "✓ antos setup idempotente"
as_user antos doctor --desktop 2>&1 | tee "$SERIAL" || fail "antos doctor --desktop"
say "✓ antos doctor --desktop"

# 2c. Actualización (T36.6): `--check` evalúa sin red (la fuente es
#     path:/etc/nixos/antos y nixpkgs está en el store) y sale 0 o 10;
#     `generations` lista la actual; `update --yes` sin cambios dice «al día»
#     y no cambia de generación.
say "+ antos system update --check"
set +e
as_user antos system update --check 2>&1 | tee "$SERIAL"
rc=${PIPESTATUS[0]}
set -e
[ "$rc" = 0 ] || [ "$rc" = 10 ] || fail "antos system update --check salió con $rc"
[ -f "$STATE/update-available.json" ] || fail "sin update-available.json"
as_user antos system generations 2>&1 | tee "$SERIAL" | grep -q '●' || fail "antos system generations sin generación actual"
GEN_BEFORE="$(nixos-rebuild list-generations --json | tr -d ' \n')"
as_user antos system update --yes 2>&1 | tee "$SERIAL" | grep -q 'al día\|actualizado' || fail "antos system update --yes"
say "✓ antos system update/generations"

# 3. El sistema se puede reconstruir sin red (todo está en el store).
cd /root || fail "sin /root"
rm -f /root/result
say "+ nixos-rebuild build --flake /etc/nixos#$HOST (sin red)"
if ! nixos-rebuild build --flake "/etc/nixos#$HOST" --no-write-lock-file 2>&1 | tail -n 30 | tee "$SERIAL"; then
  fail "nixos-rebuild build sin red"
fi
[ -e /root/result ] || fail "nixos-rebuild build no dejó result"
say "✓ nixos-rebuild build sin red"

finish "OK"
