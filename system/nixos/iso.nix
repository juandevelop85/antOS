# ISO instalable / Live de antOS Linux.
#
# Es "antOS Linux" (Método 5), NO el Live del kernel bare-metal que produce
# `antos usb build` (Método 6). Arranca a la sesión gráfica de antOS y trae el
# instalador de NixOS disponible.
{ lib, modulesPath, ... }:

{
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-graphical-base.nix"
  ];

  services.antos.enable = true;
  services.antos.desktop.enable = true;

  # `isoImage.isoName` se renombró a `image.fileName` en nixpkgs recientes.
  image.fileName = lib.mkForce "antos-linux-${lib.version}.iso";
  isoImage.volumeID = lib.mkForce "ANTOS_LINUX";

  # La base gráfica del instalador ya trae su propio compositor de rescate; el
  # de antOS (Labwc + antos-barra) es el que se autoinicia por `greetd`.
  networking.hostName = lib.mkForce "antos-live";
}
