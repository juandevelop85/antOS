# La máquina, definida como un valor.
{ modulesPath, ... }:

{
  imports = [
    "${modulesPath}/profiles/qemu-guest.nix"
    # Aquí está el remate de todo el diseño: lo que syso declara ES parte de
    # la definición del sistema. No hay un paso intermedio en el que alguien
    # traduzca la intención a comandos — la intención se convierte en una
    # línea de este fichero, la apruebas viendo el diff, y aplicarla es
    # cambiar de generación.
    ./syso-paquetes.nix
  ];

  services.syso.enable = true;

  boot.loader.grub.device = "/dev/vda";
  fileSystems."/" = {
    device = "/dev/vda1";
    fsType = "ext4";
  };

  # Una máquina de desarrollo, no de producción.
  users.users.root.initialPassword = "syso";
  services.getty.autologinUser = "root";
  networking.hostName = "syso";

  system.stateVersion = "25.05";
}
