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
