#!/usr/bin/env bash
# antOS Desktop · Smoke headless (T30.4)
#
# Arranca un compositor `wlroots` sin pantalla (`labwc` con el backend
# headless), levanta `antosd` + `antos-barra`, y verifica:
#   1. el compositor anuncia `zwlr_layer_shell_v1` (lo que ancla la barra),
#   2. `antos-barra` sigue vivo tras crear su superficie,
#   3. el socket IPC de `antosd` responde,
#   4. `grim` captura un PNG NO vacío del escritorio.
# Falla con un mensaje claro y código != 0 en cuanto uno no se cumple.
#
# Requisitos (Linux): labwc, grim, wayland-utils (`wayland-info`), y `antos` +
# `antos-barra` en el PATH (o sus rutas en $ANTOS_BIN / $ANTOS_BARRA_BIN).
# En CI se instalan por Nix o apt; ver `.github/workflows/ci.yml`.
set -u

fail() { echo "SMOKE FALLO: $*" >&2; cleanup; exit 1; }
ok()   { echo "  ✓ $*"; }

ANTOS_BIN="${ANTOS_BIN:-antos}"
ANTOS_BARRA_BIN="${ANTOS_BARRA_BIN:-antos-barra}"
RUNTIME="$(mktemp -d /tmp/antos-smoke.XXXXXX)"
export XDG_RUNTIME_DIR="$RUNTIME"
export WLR_BACKENDS=headless
export WLR_HEADLESS_OUTPUTS=1
export WLR_RENDERER=pixman          # sin GPU en CI
export ANTOS_STATE="$RUNTIME/state"
export ANTOS_WORKSPACE="$RUNTIME/workspace"
mkdir -p "$ANTOS_STATE" "$ANTOS_WORKSPACE"
( cd "$ANTOS_WORKSPACE" && git init -q 2>/dev/null || true )

COMP_PID="" ; BAR_PID="" ; DAEMON_PID=""
cleanup() {
  for p in "$BAR_PID" "$DAEMON_PID" "$COMP_PID"; do
    [ -n "$p" ] && kill "$p" 2>/dev/null
  done
  wait 2>/dev/null
  rm -rf "$RUNTIME"
}
trap cleanup EXIT

command -v labwc        >/dev/null || fail "labwc no está en el PATH"
command -v "$ANTOS_BIN" >/dev/null || fail "'$ANTOS_BIN' no está en el PATH"
command -v "$ANTOS_BARRA_BIN" >/dev/null || fail "'$ANTOS_BARRA_BIN' no está en el PATH"

# ── 1. Compositor headless ────────────────────────────────────────────────
labwc >"$RUNTIME/labwc.log" 2>&1 &
COMP_PID=$!

WAYLAND_DISPLAY=""
for i in $(seq 1 100); do
  for sock in "$RUNTIME"/wayland-*; do
    case "$sock" in *.lock) continue;; esac
    [ -S "$sock" ] && WAYLAND_DISPLAY="$(basename "$sock")" && break
  done
  [ -n "$WAYLAND_DISPLAY" ] && break
  kill -0 "$COMP_PID" 2>/dev/null || fail "labwc murió al arrancar:\n$(cat "$RUNTIME/labwc.log")"
  sleep 0.1
done
[ -n "$WAYLAND_DISPLAY" ] || fail "labwc no expuso un socket Wayland en 10 s"
export WAYLAND_DISPLAY
ok "compositor headless en \$WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

# ── 2. El compositor anuncia el protocolo layer-shell ─────────────────────
if command -v wayland-info >/dev/null; then
  wayland-info 2>/dev/null | grep -q "zwlr_layer_shell_v1" \
    || fail "el compositor no anuncia zwlr_layer_shell_v1 (la barra no podría anclarse)"
  ok "zwlr_layer_shell_v1 disponible"
else
  echo "  · wayland-info ausente; se omite la comprobación explícita del global"
fi

# ── 3. Demonio + IPC ─────────────────────────────────────────────────────
# T33.4: el demonio arranca con un guion `fake` de agente (sin red ni clave)
# para poder ejercitar el diálogo AgentRun → AgentStep ×4 → AgentDone que
# la barra pinta como línea de tiempo.
printf 'hola\n' > "$ANTOS_WORKSPACE/README.md"
cat > "$RUNTIME/agent-script.json" <<'JSON'
[
  {"text": "Miro el espacio de trabajo.", "calls": [{"tool": "fs.list", "input": {"path": "."}}], "tokens": 10},
  {"calls": [{"tool": "fs.read", "input": {"path": "README.md"}}], "tokens": 10},
  {"calls": [{"tool": "fs.read", "input": {"path": "README.md"}}], "tokens": 10},
  {"calls": [{"tool": "finalizar", "input": {"resumen": "leído; nada que cambiar"}}], "tokens": 5}
]
JSON
export ANTOS_AGENT_FAKE_SCRIPT="$RUNTIME/agent-script.json"
"$ANTOS_BIN" demonio >"$RUNTIME/antosd.log" 2>&1 &
DAEMON_PID=$!
SOCK="$ANTOS_STATE/antos.sock"
# El demonio canonicaliza ANTOS_STATE; /tmp/... no cambia, así que la ruta vale.
for i in $(seq 1 100); do
  [ -S "$SOCK" ] && break
  kill -0 "$DAEMON_PID" 2>/dev/null || fail "antosd murió:\n$(cat "$RUNTIME/antosd.log")"
  sleep 0.1
done
[ -S "$SOCK" ] || fail "antosd no creó el socket IPC en 10 s"
# Ping: una conexión + una consulta de estado de git (lo que pinta la barra).
if command -v python3 >/dev/null; then
  python3 - "$SOCK" "$ANTOS_WORKSPACE" <<'PY' || fail "el socket IPC no respondió a QueryGitStatus"
import socket, sys, json
s = socket.socket(socket.AF_UNIX); s.settimeout(10); s.connect(sys.argv[1])
s.sendall((json.dumps({"QueryGitStatus": {"workspace_path": sys.argv[2]}}) + "\n").encode())
s.shutdown(socket.SHUT_WR)
data = s.makefile().readline()
sys.exit(0 if ("GitStatus" in data or "NotGitRepo" in data) else 1)
PY
  ok "socket IPC responde (QueryGitStatus)"
else
  ok "socket IPC creado (sin python3 para el ping de respuesta)"
fi

# ── 3b. Diálogo de agente por IPC (T33.4) ────────────────────────────────
# Lo que la barra consume para su línea de tiempo: un `AgentRun` con el
# guion `fake` produce exactamente 4 `AgentStep` (list, read, read,
# finalizar) y un `AgentDone` con `stop_reason: finished`.
if command -v python3 >/dev/null; then
  python3 - "$SOCK" <<'PY' || fail "el diálogo AgentRun no produjo 4 pasos y un informe"
import socket, sys, json
s = socket.socket(socket.AF_UNIX); s.settimeout(60); s.connect(sys.argv[1])
s.sendall((json.dumps({"AgentRun": {"goal": "smoke", "provider": None, "toolset": None, "budget": None, "dry_run": False}}) + "\n").encode())
f = s.makefile(); steps = 0; done = None
for line in f:
    ev = json.loads(line)
    if "AgentStep" in ev: steps += 1
    if "AgentDone" in ev: done = ev["AgentDone"]; break
    if "Error" in ev: print("Error:", ev["Error"]); sys.exit(1)
print(f"AgentStep x{steps}, AgentDone: {done and done['stop_reason']}")
sys.exit(0 if steps == 4 and done and done["stop_reason"] == "finished" else 1)
PY
  ok "AgentRun por IPC: 4 pasos y un informe (proveedor fake)"
else
  echo "  · python3 ausente; se omite el diálogo de agente por IPC"
fi

# ── 4. La barra crea su superficie y sobrevive ───────────────────────────
# T33.4: arranca directamente con un run de agente («agente: …») para que la
# línea de tiempo y el informe se pinten en la captura.
"$ANTOS_BARRA_BIN" --intencion "agente: smoke de la barra" >"$RUNTIME/barra.log" 2>&1 &
BAR_PID=$!
sleep 4
kill -0 "$BAR_PID" 2>/dev/null \
  || fail "antos-barra terminó antes de tiempo:\n$(cat "$RUNTIME/barra.log")"
ok "antos-barra vivo tras crear su superficie layer-shell y lanzar un run de agente"

# ── 5. Captura de pantalla NO vacía (flujo de QA visual, T14.2) ──────────
command -v grim >/dev/null || fail "grim no está en el PATH"
grim "$RUNTIME/shot.png" 2>"$RUNTIME/grim.log" \
  || fail "grim no pudo capturar:\n$(cat "$RUNTIME/grim.log")"
SZ=$(wc -c < "$RUNTIME/shot.png")
[ "$SZ" -gt 1000 ] || fail "la captura de pantalla está vacía ($SZ bytes)"
ok "captura de escritorio: $SZ bytes"

echo "SMOKE OK"
