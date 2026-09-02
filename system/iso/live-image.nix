# Receta declarativa NixOS para generar la imagen Live ISO autoarrancable de antOS (T14.3)
{ config, pkgs, lib, ... }:

{
  imports = [
    <nixpkgs/nixos/modules/installer/cd-dvd/installation-cd-graphical-plasma5.nix>
  ];

  # Identidad del sistema
  networking.hostName = "antos-live";
  time.timeZone = "UTC";
  i18n.defaultLocale = "en_US.UTF-8";

  # Usuario en vivo preconfigurado
  users.users.antos = {
    isNormalUser = true;
    extraGroups = [ "wheel" "video" "input" "audio" "networkmanager" ];
    initialPassword = "antos";
  };

  # Autologin en entorno de escritorio Wayland
  services.displayManager.autoLogin = {
    enable = true;
    user = "antos";
  };

  # Compositor Wayland ligero y configurable
  programs.sway = {
    enable = true;
    extraPackages = with pkgs; [
      foot
      grim
      slurp
      wl-clipboard
      mako
      waybar
    ];
  };

  # Paquetes base y herramientas de desarrollo integradas
  environment.systemPackages = with pkgs; [
    git
    curl
    wget
    vim
    htop
    tmux
    ripgrep
    fd
    jq
    gcc
    gnumake
    qemu
  ];

  # Demonio del sistema antOS gestionado por systemd
  systemd.services.antosd = {
    description = "antOS Operating System Daemon";
    after = [ "network.target" ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "/usr/local/bin/antosd";
      Restart = "always";
      RestartSec = "2s";
      Environment = [
        "ANTOS_LIVE_MODE=1"
        "ANTOS_DESKTOP_AUTOSYNC=1"
        "RUST_BACKTRACE=1"
      ];
    };
  };

  # Configuración del instalador ISO
  isoImage.isoBaseName = "antos-live";
  isoImage.volumeID = "ANTOS_LIVE";
  isoImage.makeEfiBootable = true;
  isoImage.makeUsbBootable = true;

  system.stateVersion = "25.05";
}
