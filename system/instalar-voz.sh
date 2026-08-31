#!/usr/bin/env bash
# Deja lista la transcripción local para `syso escucha`.
#
# Todo corre en esta máquina: el audio no sale a ningún sitio.
set -euo pipefail

MODELO="${1:-base}"   # tiny | base | small | medium
DESTINO="$HOME/.cache/syso/modelos"
FICHERO="$DESTINO/ggml-${MODELO}.bin"

echo ">> herramientas"
command -v whisper-cli >/dev/null || { echo "   falta whisper: brew install whisper-cpp"; exit 1; }
command -v ffmpeg      >/dev/null || { echo "   falta ffmpeg:  brew install ffmpeg"; exit 1; }
echo "   whisper-cli y ffmpeg presentes"

if [ -f "$FICHERO" ]; then
  echo ">> el modelo '$MODELO' ya está en $FICHERO"
else
  echo ">> descargando el modelo '$MODELO' (esto tarda)"
  mkdir -p "$DESTINO"
  curl -L --fail --progress-bar \
    -o "$FICHERO" \
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${MODELO}.bin"
fi

ls -lh "$FICHERO"
echo
echo "listo. Pruébalo sin micrófono, sintetizando la frase:"
echo "  say -v Monica -o /tmp/prueba.aiff \"crea un proyecto rust llamado demo\""
echo "  target/debug/syso escucha --desde /tmp/prueba.aiff"
echo
echo "Con micrófono (macOS pedirá permiso la primera vez):"
echo "  target/debug/syso escucha --segundos 5"
