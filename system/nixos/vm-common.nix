# Lo que solo tiene sentido en una MÁQUINA VIRTUAL de desarrollo (T36.4):
# perfil de invitado QEMU, consola por el puerto serie y root con la sesión
# abierta para verla arrancar. Hasta T36.4 esto vivía en `configuration.nix`
# y lo heredaba también la máquina «para hardware»; ahora la máquina física
# es `installed.nix` + `machine.nix` y no pasa por aquí.
{ config, lib, modulesPath, ... }:

{
  imports = [ "${modulesPath}/profiles/qemu-guest.nix" ];

  # La consola en el puerto serie: es lo que permite ver el arranque entero
  # sin ventana gráfica, igual que hacemos con el kernel de la Vía B.
  boot.kernelParams = [ "console=ttyAMA0,115200" ];

  # Sin contraseña y con sesión abierta: es una máquina de desarrollo para
  # verla arrancar, no algo que dejar en una red.
  services.getty.autologinUser = "root";

  # El usuario del escritorio, si lo hay, con la contraseña de desarrollo.
  # `desktop.nix` ya no pone ninguna por defecto (T36.4): en una máquina
  # real la elige el asistente.
  # El `mkIf` va en `users.users`, no dentro del atributo: si no, la VM
  # headless (sin escritorio) definiría igualmente al usuario, sin
  # `isNormalUser` ni grupo.
  users.users = lib.mkMerge [
    { root.initialPassword = "antos"; }
    (lib.mkIf config.services.antos.desktop.enable {
      ${config.services.antos.desktop.autologinUser}.initialPassword = lib.mkDefault "antos";
    })
  ];
}
