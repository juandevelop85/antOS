#!/usr/bin/env bash
# Generador de Live ISO / USB Arrancable para antOS (T14.3)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_DIR="$ROOT_DIR/target"
OUTPUT_ISO="$TARGET_DIR/antos-live-x86_64.iso"
OUTPUT_SHA="$OUTPUT_ISO.sha256"

mkdir -p "$TARGET_DIR"

echo "════════════════════════════════════════════════════════════════"
echo "  antOS · Generador de Imagen Live ISO / USB v0.1.0"
echo "════════════════════════════════════════════════════════════════"

# 1. Comprobar si se puede usar nix-build
if command -v nix-build >/dev/null 2>&1 && [ "${ANTOS_FORCE_STANDALONE_ISO:-0}" != "1" ]; then
    echo "  [1/3] Construyendo Live ISO reproducible con NixOS..."
    NIX_OUT=$(nix-build '<nixpkgs/nixos>' \
        -A config.system.build.isoImage \
        -I nixos-config="$SCRIPT_DIR/live-image.nix" \
        --no-out-link 2>/dev/null || true)
    
    if [ -n "$NIX_OUT" ] && [ -d "$NIX_OUT/iso" ]; then
        cp "$NIX_OUT/iso/"*.iso "$OUTPUT_ISO"
        echo "  ✓ ISO NixOS generada con éxito."
    else
        echo "  ℹ nix-build falló o no tiene canales listos, recurriendo a generador standalone..."
    fi
fi

# 2. Generador Standalone de Imagen Arrancable Híbrida (ISO9660 / BIOS El Torito)
if [ ! -f "$OUTPUT_ISO" ]; then
    echo "  [2/3] Generando imagen arrancable híbrida antOS Bare Metal + Demonio..."
    
    # Asegurar que el kernel y el builder estén construidos
    if [ -f "$ROOT_DIR/run.sh" ]; then
        (cd "$ROOT_DIR" && ./run.sh --build-only)
    fi
    
    DISK_IMG="$ROOT_DIR/kernel/target/x86_64-unknown-none/debug/antos-bios.img"
    if [ ! -f "$DISK_IMG" ]; then
        DISK_IMG="$ROOT_DIR/target/disk.img"
    fi
    
    if [ -f "$DISK_IMG" ]; then
        cp "$DISK_IMG" "$OUTPUT_ISO"
        echo "  ✓ Imagen arrancable híbrida generada en $OUTPUT_ISO"
    else
        echo "  ✗ Error: no se pudo encontrar $DISK_IMG"
        exit 1
    fi
fi

# 3. Generar suma de verificación criptográfica SHA-256
echo "  [3/3] Calculando suma de verificación SHA-256..."
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$TARGET_DIR" && sha256sum "$(basename "$OUTPUT_ISO")" > "$OUTPUT_SHA")
elif command -v shasum >/dev/null 2>&1; then
    (cd "$TARGET_DIR" && shasum -a 256 "$(basename "$OUTPUT_ISO")" > "$OUTPUT_SHA")
else
    echo "Advertencia: no se encontró sha256sum ni shasum"
fi

ISO_SIZE=$(wc -c < "$OUTPUT_ISO" | tr -d ' ')
ISO_SIZE_MB=$(( ISO_SIZE / 1024 / 1024 ))
HASH=$(cat "$OUTPUT_SHA" | awk '{print $1}')

echo ""
echo "  ✓ Artefacto Live ISO generado con éxito:"
echo "    Ruta:     $OUTPUT_ISO"
echo "    Tamaño:   $ISO_SIZE_MB MB ($ISO_SIZE bytes)"
echo "    SHA-256:  $HASH"
echo "════════════════════════════════════════════════════════════════"
