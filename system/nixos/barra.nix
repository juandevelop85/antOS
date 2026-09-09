# Paquete Nix de la barra de escritorio antOS (`antos-barra`).
#
# `system/barra` es un workspace de Cargo aparte (GTK4 y `gtk4-layer-shell`
# solo existen en Linux con Wayland, así que no puede formar parte del
# workspace que se compila en macOS). Este paquete lo construye desde su
# propio `Cargo.lock`, arrastrando `system/protocolo` porque `antos-protocol`
# es una dependencia por ruta (`../protocolo`).
{ lib
, rustPlatform
, pkg-config
, wrapGAppsHook4
, gtk4
, gtk4-layer-shell
, wayland
, glib
}:

rustPlatform.buildRustPackage {
  pname = "antos-barra";
  version = "0.1.0";

  # Solo lo que influye en el binario: el crate de la barra y el del protocolo
  # (dependencia por ruta). Nada de la raíz del repositorio, para que editar
  # documentación o el kernel no invalide el hash.
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../system/barra
      ../../system/protocolo
    ];
  };

  # `system/barra` tiene su propio workspace y su propio lockfile.
  sourceRoot = "source/system/barra";
  cargoLock.lockFile = ../../system/barra/Cargo.lock;

  nativeBuildInputs = [ pkg-config wrapGAppsHook4 ];
  buildInputs = [ gtk4 gtk4-layer-shell wayland glib ];

  # El CSS viaja embebido (`include_str!("estilo.css")`), así que no hay
  # recursos que instalar aparte del binario.
  doCheck = false;

  meta = {
    description = "Barra de intención y centro de agentes de antOS (Wayland GTK4 layer-shell)";
    mainProgram = "antos-barra";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
