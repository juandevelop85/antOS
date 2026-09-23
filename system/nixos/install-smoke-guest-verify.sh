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
# Al fallar, antes de apagar: lo que hace falta para entender por qué no
# arrancó lo gráfico (el diario de la sesión no sale por la serie, y el
# compositor se reinicia en bucle sin dejar rastro en ella).
diag() {
  say "--- diag: dispositivos DRM"
  ls -l /dev/dri 2>&1 | tee "$SERIAL"
  say "--- diag: greetd"
  journalctl -u greetd.service --no-pager -n 40 2>&1 | tail -n 40 | tee "$SERIAL"
  say "--- diag: sesión del usuario"
  journalctl _UID="$(id -u "$USER_NAME" 2>/dev/null || echo 1000)" --no-pager -n 40 2>&1 | tail -n 40 | tee "$SERIAL"
  say "--- diag: procesos"
  ps -eo pid,comm 2>&1 | grep -E "labwc|plasma|barra|greetd|antos" | tee "$SERIAL"
}
fail() { diag; finish "FAIL $*"; }
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
# El modo (clean|dual) entra por `fw_cfg`, igual que en la fase A.
MODE="$(tr -d '[:space:]' < /sys/firmware/qemu_fw_cfg/by_name/opt/antos/mode/raw 2>/dev/null || echo clean)"
# `/etc/set-environment` da por hecho que `$HOME` existe (lo escribe para
# sesiones de usuario), y un servicio de systemd no la define: con `set -u`
# el guion moría ahí antes de la primera comprobación (smoke real,
# 2026-09-22). Se corre como root, así que `HOME=/root`, y el sourcing va
# sin `-u` por si el perfil vuelve a asumir otra variable de sesión.
export HOME="${HOME:-/root}"
if [ -f /etc/set-environment ]; then
  set +u
  . /etc/set-environment
  set -u
fi
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
# `pgrep -x antos-barra` no la encuentra nunca: el envoltorio de NixOS se
# llama `.antos-barra-wrapped` y el kernel recorta `comm` a 15 caracteres
# (`.antos-barra-wr`). Se busca por la línea de órdenes completa, que sí
# lleva la ruta del store con el nombre entero.
wait_for 120 "antos-barra en ejecución" pgrep -f "antos-barra"

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

# 2c. Actualización (T36.6). Esta VM no tiene red a propósito, y comprobar
#     si hay algo nuevo exige salir a internet: `nix flake update` refresca
#     TODAS las entradas del flake, también `nixpkgs` (github:), por mucho
#     que la fuente de antOS sea `path:/etc/nixos/antos`. Lo que se exige
#     aquí, entonces, es que lo diga — código 20 (EXIT_NO_NETWORK) y un
#     mensaje claro — y no que invente un «estás al día» sin comprobarlo ni
#     vomite el error crudo de nix. `generations` sí es local y debe
#     funcionar, y la generación no puede cambiar.
GEN_BEFORE="$(nixos-rebuild list-generations --json | tr -d ' \n')"
say "+ antos system update --check (sin red: debe decirlo, código 20)"
set +e
as_user antos system update --check 2>&1 | tee "$SERIAL" | grep -q 'sin acceso a la red'
# `PIPESTATUS` solo sobrevive al primer mandato posterior: cualquier otra
# cosa en medio (un `said=$?`) la reescribe y `rc` salía 0 aunque el mandato
# hubiera salido 20.
estado=("${PIPESTATUS[@]}")
rc=${estado[0]}
said=${estado[1]}
set -e
[ "$rc" = 20 ] || fail "antos system update --check salió con $rc (esperado 20, sin red)"
[ "$said" = 0 ] || fail "antos system update --check no explicó la falta de red"
as_user antos system generations 2>&1 | tee "$SERIAL" | grep -q '●' || fail "antos system generations sin generación actual"
say "+ antos system update --yes (sin red: debe decirlo, código 20)"
set +e
as_user antos system update --yes 2>&1 | tee "$SERIAL" | grep -q 'sin acceso a la red'
# `PIPESTATUS` solo sobrevive al primer mandato posterior: cualquier otra
# cosa en medio (un `said=$?`) la reescribe y `rc` salía 0 aunque el mandato
# hubiera salido 20.
estado=("${PIPESTATUS[@]}")
rc=${estado[0]}
said=${estado[1]}
set -e
[ "$rc" = 20 ] || fail "antos system update --yes salió con $rc (esperado 20, sin red)"
[ "$said" = 0 ] || fail "antos system update --yes no explicó la falta de red"
GEN_AFTER="$(nixos-rebuild list-generations --json | tr -d ' \n')"
[ "$GEN_BEFORE" = "$GEN_AFTER" ] || fail "update sin red cambió las generaciones"
say "✓ antos system update/generations"

# 2d. Dual-boot (T36.2): ya arrancados con systemd-boot, el gestor publica
#     sus entradas en `LoaderEntries` y `bootctl list` las enumera — es el
#     único sitio donde se puede comprobar que el vecino sigue arrancable,
#     porque esa entrada la sintetiza el gestor, no está en disco.
if [ "$MODE" = dual ]; then
  say "+ bootctl list (dual: antOS y el vecino)"
  BOOTCTL="$(bootctl list 2>&1 || true)"
  say "$BOOTCTL"
  echo "$BOOTCTL" | grep -qi 'nixos\|antos' || fail "bootctl list no muestra a antOS"
  echo "$BOOTCTL" | grep -qi 'windows' || fail "bootctl list no muestra a Windows"
  [ -f /boot/EFI/Microsoft/Boot/bootmgfw.efi ] || fail "el cargador del vecino desapareció"
  say "✓ dual-boot: systemd-boot ofrece antOS y el vecino"
fi

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
