# El escritorio antOS Linux, definido como un valor.
#
# `services.antos.enable` (en `modulo.nix`) monta el demonio. Este módulo
# añade `services.antos.desktop`: la sesión gráfica completa —compositor
# Labwc + `antos-barra` + variables XDG + autologin Wayland— descrita de
# forma declarativa. Activarla es una línea del `configuration.nix`.
#
# Los ficheros `rc.xml` / `autostart` / `environment` de la sesión son los
# mismos que consume `system/desktop/start-session.sh` sobre un Linux
# no-NixOS (T13.0): aquí se reutilizan tal cual.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.antos.desktop;

  # Variables de entorno de la sesión Wayland — el contenido de
  # `system/desktop/environment`, declarado aquí para que sea parte de la
  # definición del sistema.
  sessionEnv = {
    XDG_CURRENT_DESKTOP = "antOS";
    XDG_SESSION_DESKTOP = "antOS";
    XDG_SESSION_TYPE = "wayland";
    GDK_BACKEND = "wayland,x11,*";
    QT_QPA_PLATFORM = "wayland;xcb";
    SDL_VIDEODRIVER = "wayland";
    CLUTTER_BACKEND = "wayland";
    MOZ_ENABLE_WAYLAND = "1";
    _JAVA_AWT_WM_NONREPARENTING = "1";
    ANTOS_DESKTOP = "1";
    ANTOS_COMPOSITOR = "labwc";
    EDITOR = "nvim";
    VISUAL = "nvim";
    ANTOS_DEFAULT_EDITOR = "nvim";
  };

  # Guion de arranque de la sesión: instala la configuración de Labwc en el
  # `$HOME` del usuario (Labwc lee de `~/.config/labwc`) y ejecuta el
  # compositor con el `autostart` de antOS. Equivale a
  # `system/desktop/start-session.sh` pero con rutas del store.
  sessionScript = pkgs.writeShellScript "antos-desktop-session" ''
    set -eu
    export XDG_CONFIG_HOME="''${XDG_CONFIG_HOME:-$HOME/.config}"
    mkdir -p "$XDG_CONFIG_HOME/labwc"
    cp -f /etc/antos/desktop/rc.xml "$XDG_CONFIG_HOME/labwc/rc.xml"
    cp -f /etc/antos/desktop/autostart "$XDG_CONFIG_HOME/labwc/autostart"
    chmod +x "$XDG_CONFIG_HOME/labwc/autostart"
    exec ${lib.getExe cfg.compositor} -s "$XDG_CONFIG_HOME/labwc/autostart"
  '';
in
{
  options.services.antos.desktop = {
    enable = lib.mkEnableOption "el escritorio antOS Linux (Wayland + antos-barra)";

    compositor = lib.mkOption {
      type = lib.types.package;
      default = pkgs.labwc;
      defaultText = lib.literalExpression "pkgs.labwc";
      description = "Compositor Wayland `wlroots` que ancla la barra por `wlr-layer-shell`.";
    };

    autologinUser = lib.mkOption {
      type = lib.types.str;
      default = "antos";
      description = "Usuario que inicia sesión automáticamente en el escritorio Wayland.";
    };

    terminal = lib.mkOption {
      type = lib.types.package;
      default = pkgs.foot;
      defaultText = lib.literalExpression "pkgs.foot";
      description = "Emulador de terminal Wayland incluido en la sesión.";
    };

    editor = lib.mkOption {
      type = lib.types.package;
      default = pkgs.neovim;
      defaultText = lib.literalExpression "pkgs.neovim";
      description = "Editor por defecto (`EDITOR`/`VISUAL`).";
    };

    barra = lib.mkOption {
      type = lib.types.package;
      default = pkgs.antos-barra;
      defaultText = lib.literalExpression "pkgs.antos-barra";
      description = "Paquete de la barra de intención (`system/nixos/barra.nix`).";
    };
  };

  config = lib.mkIf cfg.enable {
    # El escritorio implica el demonio.
    services.antos.enable = lib.mkDefault true;

    environment.systemPackages = [
      cfg.compositor
      cfg.barra
      cfg.terminal
      cfg.editor
      pkgs.wl-clipboard
      pkgs.grim # captura de pantalla (wlr-screencopy) para QA visual (T14.2)
      pkgs.slurp
      pkgs.git
    ];

    environment.sessionVariables = sessionEnv;

    # Configuración declarativa de la sesión, tomada de `system/desktop/`.
    environment.etc."antos/desktop/rc.xml".source = ../desktop/rc.xml;
    environment.etc."antos/desktop/autostart" = {
      source = ../desktop/autostart;
      mode = "0755";
    };
    environment.etc."antos/desktop/environment".source = ../desktop/environment;

    # Entrada de sesión Wayland + autologin. `greetd` con `default_session`
    # arranca la sesión de antOS para `autologinUser` sin más pasos.
    services.greetd = {
      enable = true;
      settings.default_session = {
        command = "${sessionScript}";
        user = cfg.autologinUser;
      };
    };

    users.users.${cfg.autologinUser} = {
      isNormalUser = lib.mkDefault true;
      description = lib.mkDefault "antOS";
      extraGroups = lib.mkDefault [ "wheel" "video" "input" ];
      initialPassword = lib.mkDefault "antos";
    };

    # Pila gráfica para el compositor Wayland.
    hardware.graphics.enable = lib.mkDefault true;
    programs.dconf.enable = lib.mkDefault true;
    fonts.packages = [ pkgs.dejavu_fonts pkgs.noto-fonts ];

    xdg.portal = {
      enable = true;
      extraPortals = [ pkgs.xdg-desktop-portal-wlr ];
      config.common.default = "wlr";
    };
  };
}
