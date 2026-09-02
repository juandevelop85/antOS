#!/usr/bin/env bash
# Pipeline de Empaquetado Release y Distribución v0.1.0 para antOS (T14.3)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION="v0.1.0"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
PACKAGE_NAME="antos-${VERSION}-${OS}-${ARCH}"
RELEASE_DIR="$ROOT_DIR/target/release-pkg"
PKG_DIR="$RELEASE_DIR/$PACKAGE_NAME"
TARBALL="$ROOT_DIR/target/${PACKAGE_NAME}.tar.gz"
SHA256_FILE="$ROOT_DIR/target/SHA256SUMS"

echo "════════════════════════════════════════════════════════════════"
echo "  antOS · Pipeline de Empaquetado Release $VERSION ($OS-$ARCH)"
echo "════════════════════════════════════════════════════════════════"

# 1. Compilación optimizada en modo release del workspace
echo "  [1/5] Compilando workspace en perfil release..."
(cd "$ROOT_DIR" && cargo build --workspace --release)

# 2. Preparar árbol de directorios de distribución
echo "  [2/5] Ensamblando árbol del paquete en $PKG_DIR..."
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/bin"
mkdir -p "$PKG_DIR/capabilities"
mkdir -p "$PKG_DIR/config"
mkdir -p "$PKG_DIR/docs"
mkdir -p "$PKG_DIR/iso"

# Copiar binarios compilados
cp "$ROOT_DIR/target/release/antos" "$PKG_DIR/bin/"
if [ -f "$ROOT_DIR/target/release/builder" ]; then
    cp "$ROOT_DIR/target/release/builder" "$PKG_DIR/bin/"
fi

# Strip de símbolos para optimizar tamaño binario
if command -v strip >/dev/null 2>&1; then
    echo "  [3/5] Reduciendo tamaño de binarios con strip..."
    strip "$PKG_DIR/bin/antos" 2>/dev/null || true
    if [ -f "$PKG_DIR/bin/builder" ]; then
        strip "$PKG_DIR/bin/builder" 2>/dev/null || true
    fi
fi

# Copiar capacidades declarativas
cp "$ROOT_DIR/system/capabilities/"*.toml "$PKG_DIR/capabilities/"

# Copiar recetas y configuraciones
if [ -d "$ROOT_DIR/system/nixos" ]; then
    cp -r "$ROOT_DIR/system/nixos" "$PKG_DIR/config/"
fi
if [ -d "$ROOT_DIR/system/iso" ]; then
    cp -r "$ROOT_DIR/system/iso"/* "$PKG_DIR/iso/"
fi

# Copiar documentación clave
cp "$ROOT_DIR/README.md" "$PKG_DIR/"
cp "$ROOT_DIR/docs/manual-de-comandos.md" "$PKG_DIR/docs/"
if [ -f "$ROOT_DIR/CHANGELOG.md" ]; then
    cp "$ROOT_DIR/CHANGELOG.md" "$PKG_DIR/"
fi
if [ -f "$ROOT_DIR/LICENSE" ]; then
    cp "$ROOT_DIR/LICENSE" "$PKG_DIR/"
fi

# 3. Generar Live ISO básica si es posible
echo "  [4/5] Generando artefacto Live ISO..."
(cd "$ROOT_DIR" && ./system/iso/build-iso.sh || true)

# 4. Empaquetar tarball comprimido
echo "  [5/5] Comprimiendo paquete release en $TARBALL..."
(cd "$RELEASE_DIR" && tar -czf "$TARBALL" "$PACKAGE_NAME")

# 5. Generar sumas de verificación criptográfica SHA256SUMS
echo "  [*] Calculando sumas SHA256 para la distribución..."
cd "$ROOT_DIR/target"

TMP_SUMS=$(mktemp)
for file in "${PACKAGE_NAME}.tar.gz" "antos-live-x86_64.iso"; do
    if [ -f "$file" ]; then
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$file" >> "$TMP_SUMS"
        elif command -v shasum >/dev/null 2>&1; then
            shasum -a 256 "$file" >> "$TMP_SUMS"
        fi
    fi
done
mv "$TMP_SUMS" "$SHA256_FILE"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  ✓ Release $VERSION generado con éxito:"
echo "    Tarball:   $TARBALL"
echo "    Checksums: $SHA256_FILE"
echo ""
cat "$SHA256_FILE"
echo "════════════════════════════════════════════════════════════════"
