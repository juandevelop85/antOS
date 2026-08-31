{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "sysod";
  version = "0.1.0";

  # La raíz del repositorio: sysod vive en un workspace junto a `builder`.
  src = ../..;
  cargoLock.lockFile = ../../Cargo.lock;

  # Solo sysod. `builder` no se construye aquí a propósito: su dependencia
  # `bootloader` lanza cargo desde su build.rs, y eso no funciona —ni debe—
  # dentro del recinto de compilación de Nix.
  cargoBuildFlags = [ "-p" "sysod" ];
  doCheck = false;

  # El catálogo viaja con el binario. Un sistema en el que las capacidades
  # vinieran de un sitio mutable sería un sistema en el que se pueden añadir
  # capacidades sin que nadie lo apruebe.
  postInstall = ''
    mkdir -p $out/share/syso
    cp -r system/capabilities $out/share/syso/capabilities
  '';

  meta = {
    description = "Intérprete de intenciones: capacidades tipadas, aisladas y reversibles";
    mainProgram = "syso";
    license = lib.licenses.mit;
  };
}
