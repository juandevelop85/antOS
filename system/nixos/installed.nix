# La máquina antOS Linux tal y como la deja `antos install` (T36.3).
#
# Es el espejo en Nix de lo que `DeployEngine::generate_nixos_config`
# escribe en `/etc/nixos/configuration.nix` del destino: mismo escritorio,
# mismo gestor de arranque, misma disposición de disco por etiquetas
# (`antos-root` / `ANTOS_ESP`, las que pone el formateo de T36.2), mismo
# usuario. Sirve para dos cosas:
#
# 1. Meter su closure en la ISO (`isoImage.storeContents`): así
#    `nixos-install` dentro del live no tiene nada que construir ni
#    descargar — el hostname, el usuario o el teclado que el asistente elija
#    solo cambian ficheros de texto; los paquetes son estos.
# 2. Evaluarla y construirla en CI como referencia de lo que se instala.
#
# Si el generador de Rust y este fichero divergen, la instalación sigue
# funcionando (nix construye lo que falte) pero deja de ser offline; el
# test `test_generated_configuration_matches_installed_nix` fija las líneas
# que tienen que coincidir.
{ lib, modulesPath, ... }:

{
  imports = [ (modulesPath + "/installer/scan/not-detected.nix") ];

  boot.initrd.availableKernelModules = [ "nvme" "ahci" "xhci_pci" "usbhid" "sd_mod" "virtio_pci" "virtio_blk" ];
  boot.loader.grub.enable = lib.mkDefault false;

  fileSystems."/" = {
    device = "/dev/disk/by-label/antos-root";
    fsType = "ext4";
  };
  fileSystems."/boot" = {
    device = "/dev/disk/by-label/ANTOS_ESP";
    fsType = "vfat";
    options = [ "fmask=0077" "dmask=0077" ];
  };

  networking.hostName = lib.mkDefault "antos-box";

  services.antos.enable = true;
  services.antos.desktop.enable = true;
  services.antos.desktop.autologinUser = lib.mkDefault "antos";

  # El perfil de máquina física (T36.4): red, audio, bluetooth, firmware,
  # teclado, locale, zona horaria, sudo, energía, systemd-boot.
  services.antos.machine = {
    enable = true;
    keyboardLayout = lib.mkDefault "us";
    locale = lib.mkDefault "en_US.UTF-8";
    timeZone = lib.mkDefault "UTC";
  };

  users.users.antos = {
    isNormalUser = true;
    description = "antOS";
    extraGroups = [ "wheel" "video" "input" "networkmanager" ];
    # Sin contraseña utilizable: esta máquina es la referencia cuya closure
    # viaja en la ISO, no algo que se instale tal cual. La real la escribe
    # el instalador (`initialHashedPassword` del hash del asistente).
    initialHashedPassword = "!";
  };

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  system.stateVersion = "25.05";
}
