{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "antosd";
  version = "0.1.0";

  # Solo lo que de verdad influye en el binario.
  #
  # Antes esto era `src = ../..`, la raíz entera, y cualquier cambio en el
  # repositorio —un README, el kernel de la Vía B— cambiaba el hash y obligaba
  # a recompilar antosd. Con un conjunto explícito, editar documentación deja
  # de costar una compilación.
  #
  # `builder` entra solo por su manifiesto: es miembro del workspace, así que
  # cargo necesita poder leerlo aunque no se compile.
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../system/protocolo
      ../../system/antosd
      ../../system/capabilities
      ../../builder/Cargo.toml
      ../../builder/src
    ];
  };
  cargoLock.lockFile = ../../Cargo.lock;

  # Solo antosd. `builder` no se construye aquí a propósito: su dependencia
  # `bootloader` lanza cargo desde su build.rs, y eso no funciona —ni debe—
  # dentro del recinto de compilación de Nix.
  cargoBuildFlags = [ "-p" "antosd" ];
  doCheck = false;

  # El catálogo viaja con el binario. Un sistema en el que las capacidades
  # vinieran de un sitio mutable sería un sistema en el que se pueden añadir
  # capacidades sin que nadie lo apruebe.
  postInstall = ''
    mkdir -p $out/share/antos
    cp -r system/capabilities $out/share/antos/capabilities
  '';

  meta = {
    description = "Intérprete de intenciones: capacidades tipadas, aisladas y reversibles";
    mainProgram = "antos";
    license = lib.licenses.mit;
  };
}
