# La máquina FÍSICA de antOS Linux (T36.4): `services.antos.machine`.
#
# `services.antos.desktop` describe la sesión (compositor, barra, demonio);
# este módulo describe lo que un portátil o un sobremesa necesita para ser
# usable el primer día y que una VM no: WiFi por NetworkManager, sonido
# por PipeWire, bluetooth, firmware propietario, teclado en el idioma del
# usuario (consola, Wayland y Plasma coherentes), locale, zona horaria,
# `sudo` con contraseña, gestión de energía, swap comprimido en RAM y
# `systemd-boot` en la ESP. Nada de `qemu-guest.nix`, consola serie ni
# autologin de root: eso es `vm-common.nix`.
#
# Lo activa `installed.nix` (la máquina que `antos install` genera) y el
# `configuration.nix` que escribe el instalador, con el teclado, el locale
# y la zona horaria que el usuario eligió en el asistente.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.antos.machine;
  desktop = config.services.antos.desktop;

  # Layout XKB → keymap de consola. La mayoría coinciden; estos no.
  consoleKeymapFor = layout: {
    gb = "uk";
    latam = "la-latin1";
    "pt-br" = "br-abnt2";
    br = "br-abnt2";
    ch = "sg";
  }.${layout} or layout;
in
{
  options.services.antos.machine = {
    enable = lib.mkEnableOption "el perfil de máquina física de antOS Linux (red, audio, bluetooth, teclado, locale, energía)";

    keyboardLayout = lib.mkOption {
      type = lib.types.strMatching "[a-z][a-z-]*";
      default = "us";
      example = "es";
      description = ''
        Layout XKB (`us`, `es`, `latam`, `de`, `fr`, `gb`, `pt-br`, `it`, …).
        Se aplica a la consola (`console.keyMap`, con la traducción de los
        nombres que difieren), a Labwc (`XKB_DEFAULT_LAYOUT`) y a Plasma
        (`services.xserver.xkb`).
      '';
    };

    keyboardVariant = lib.mkOption {
      type = lib.types.strMatching "[a-z_,-]*";
      default = "";
      example = "nodeadkeys";
      description = "Variante XKB, vacía si ninguna.";
    };

    locale = lib.mkOption {
      type = lib.types.strMatching "[A-Za-z_@.0-9-]+";
      default = "en_US.UTF-8";
      example = "es_ES.UTF-8";
      description = "`i18n.defaultLocale` de la máquina.";
    };

    timeZone = lib.mkOption {
      type = lib.types.strMatching "[A-Za-z0-9_+/-]+";
      default = "UTC";
      example = "Europe/Madrid";
      description = "`time.timeZone` de la máquina.";
    };
  };

  config = lib.mkIf cfg.enable {
    # ── Arranque ─────────────────────────────────────────────────────────
    boot.loader.systemd-boot.enable = lib.mkDefault true;
    boot.loader.efi.canTouchEfiVariables = lib.mkDefault true;

    # ── Red ──────────────────────────────────────────────────────────────
    networking.networkmanager.enable = true;
    users.users.${desktop.autologinUser}.extraGroups =
      lib.mkIf desktop.enable [ "networkmanager" ];

    # ── Hardware ─────────────────────────────────────────────────────────
    hardware.enableRedistributableFirmware = true;
    hardware.bluetooth.enable = true;
    hardware.bluetooth.powerOnBoot = lib.mkDefault true;
    # Bajo Plasma el applet de bluetooth lo trae Plasma; con Labwc, blueman.
    services.blueman.enable = desktop.enable && desktop.flavor == "labwc";

    # ── Sonido ───────────────────────────────────────────────────────────
    security.rtkit.enable = true;
    services.pipewire = {
      enable = true;
      alsa.enable = true;
      alsa.support32Bit = pkgs.stdenv.hostPlatform.isx86_64;
      pulse.enable = true;
      wireplumber.enable = true;
    };

    # ── Teclado, locale, zona horaria ────────────────────────────────────
    console.keyMap = lib.mkDefault (consoleKeymapFor cfg.keyboardLayout);
    services.xserver.xkb = {
      layout = cfg.keyboardLayout;
      variant = cfg.keyboardVariant;
    };
    environment.sessionVariables = {
      XKB_DEFAULT_LAYOUT = cfg.keyboardLayout;
    } // lib.optionalAttrs (cfg.keyboardVariant != "") {
      XKB_DEFAULT_VARIANT = cfg.keyboardVariant;
    };
    i18n.defaultLocale = cfg.locale;
    time.timeZone = cfg.timeZone;

    # ── Seguridad y energía ──────────────────────────────────────────────
    security.sudo.wheelNeedsPassword = true;
    services.power-profiles-daemon.enable = lib.mkDefault true;
    services.fstrim.enable = true;
    zramSwap.enable = lib.mkDefault true;
  };
}
