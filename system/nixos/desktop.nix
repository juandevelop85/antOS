# El escritorio antOS Linux, definido como un valor.
#
# `services.antos.enable` (en `module.nix`) monta el demonio. Este módulo
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
    ${lib.optionalString cfg.panel.enable ''
      cp -f /etc/antos/desktop/labwc/menu.xml "$XDG_CONFIG_HOME/labwc/menu.xml"
    ''}
    chmod +x "$XDG_CONFIG_HOME/labwc/autostart"
    exec ${lib.getExe cfg.compositor} -s "$XDG_CONFIG_HOME/labwc/autostart"
  '';

  # ── Escritorio tradicional opcional (T30.6) ──────────────────────────────
  # `autostart` base (demonio + antos-barra) de `system/desktop/`, más —si el
  # panel está activado— las piezas `wlroots` del mobiliario clásico. Se
  # genera aquí para no meter waybar/swaybg en el `autostart` compartido que
  # también usa `start-session.sh` sobre Linux no-NixOS.
  panelAutostart = ''

    # ── Escritorio tradicional (T30.6) ──
    ${pkgs.swaybg}/bin/swaybg ${
      if cfg.panel.wallpaper != null
      then ''-i "${cfg.panel.wallpaper}"''
      else "-c 1a1b26"
    } -m fill >/dev/null 2>&1 &
    ${lib.getExe cfg.panel.package} -c /etc/antos/desktop/waybar/config -s /etc/antos/desktop/waybar/style.css >/dev/null 2>&1 &
    ${pkgs.mako}/bin/mako >/dev/null 2>&1 &
    ${pkgs.swayidle}/bin/swayidle -w timeout 600 '${pkgs.swaylock}/bin/swaylock -f' >/dev/null 2>&1 &
  '';

  sessionAutostart = pkgs.writeShellScript "antos-autostart"
    (builtins.readFile ../desktop/autostart
      + lib.optionalString cfg.panel.enable panelAutostart);

  # Panel inferior: botón de menú (→ lanzador), reloj, CPU/RAM/red y bandeja.
  # Formatos de texto: sin dependencia de una fuente de iconos.
  waybarConfig = {
    layer = "top";
    position = "bottom";
    height = 30;
    modules-left = [ "custom/menu" ];
    modules-center = [ "clock" ];
    modules-right = [ "cpu" "memory" "network" "tray" ];
    "custom/menu" = {
      format = "≡ Aplicaciones";
      on-click = lib.getExe cfg.panel.launcher;
      tooltip = false;
    };
    clock.format = "{:%a %d %b  %H:%M}";
    cpu = { format = "CPU {usage}%"; interval = 3; };
    memory = { format = "RAM {percentage}%"; interval = 3; };
    network = {
      format-ethernet = "NET {ipaddr}";
      format-wifi = "WIFI {essid} {signalStrength}%";
      format-disconnected = "NET —";
      interval = 5;
    };
    tray.spacing = 8;
  };

  waybarStyle = ''
    * { font-family: "DejaVu Sans", sans-serif; font-size: 12px; }
    window#waybar {
      background: rgba(20, 21, 28, 0.92);
      color: #c0caf5;
      border-top: 1px solid #2a2e3f;
    }
    #custom-menu, #clock, #cpu, #memory, #network, #tray { padding: 0 12px; }
    #custom-menu {
      background: #2a2e3f;
      margin: 4px 6px;
      border-radius: 6px;
      font-weight: bold;
    }
    #clock { font-weight: bold; }
  '';

  labwcMenu = ''
    <?xml version="1.0" encoding="UTF-8"?>
    <openbox_menu>
      <menu id="root-menu" label="antOS">
        <item label="Terminal">
          <action name="Execute" command="${lib.getExe cfg.terminal}"/>
        </item>
        <item label="Archivos">
          <action name="Execute" command="${lib.getExe cfg.panel.fileManager}"/>
        </item>
        <item label="Editor">
          <action name="Execute" command="antos edit"/>
        </item>
        <item label="antOS · dev">
          <action name="Execute" command="antos dev"/>
        </item>
        <separator/>
        <item label="Lanzador de aplicaciones">
          <action name="Execute" command="${lib.getExe cfg.panel.launcher}"/>
        </item>
        <separator/>
        <item label="Recargar Labwc">
          <action name="Reconfigure"/>
        </item>
        <item label="Salir de la sesión">
          <action name="Exit"/>
        </item>
      </menu>
    </openbox_menu>
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

    devTools = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Incluye el userland de desarrollo en la imagen: Neovim configurado
        contra `antos lsp`, Git + `gh` + `delta`, terminal, `ripgrep`/`fd`/
        `bat`/`jq`, `direnv` y Flatpak para `antos app` (T30.3).
      '';
    };

    browser = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = pkgs.firefox;
      defaultText = lib.literalExpression "pkgs.firefox";
      description = "Navegador incluido de fábrica (`null` para no incluir ninguno).";
    };

    # ── Mobiliario de escritorio tradicional (T30.6) ────────────────────
    panel = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Añade a la sesión Labwc el mobiliario de un escritorio al uso —
          panel inferior (`waybar`), lanzador de aplicaciones (`fuzzel`),
          fondo (`swaybg`), notificaciones (`mako`), menú de clic derecho y
          gestor de archivos— sin dejar de ser una sesión `wlroots` ligera.
          `false` devuelve la sesión mínima de solo `antos-barra`.
        '';
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.waybar;
        defaultText = lib.literalExpression "pkgs.waybar";
        description = "El panel. Se lanza con `-c`/`-s` apuntando a la config de antOS.";
      };

      launcher = lib.mkOption {
        type = lib.types.package;
        default = pkgs.fuzzel;
        defaultText = lib.literalExpression "pkgs.fuzzel";
        description = "Lanzador de aplicaciones (escanea los `.desktop` del sistema).";
      };

      fileManager = lib.mkOption {
        type = lib.types.package;
        default = pkgs.xfce.thunar;
        defaultText = lib.literalExpression "pkgs.xfce.thunar";
        description = "Gestor de archivos gráfico.";
      };

      wallpaper = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Imagen de fondo. `null` → color sólido.";
      };
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [ {
    # El escritorio implica el demonio.
    services.antos.enable = lib.mkDefault true;

    # El socket IPC se crea `0600` (T31.8): para que `antos-barra` pueda
    # hablar con el demonio, este tiene que correr como el mismo usuario que
    # la sesión Wayland (T31.18).
    services.antos.user = lib.mkDefault cfg.autologinUser;

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
    # `autostart` = base (`system/desktop/autostart`: demonio + antos-barra)
    # más el mobiliario del panel si `panel.enable` (T30.6). Generado, no
    # copiado tal cual, por eso.
    environment.etc."antos/desktop/autostart" = {
      source = sessionAutostart;
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
  }

  # ── Userland de desarrollo (T30.3) ────────────────────────────────────
  (lib.mkIf cfg.devTools {
    # Neovim configurado contra el servidor LSP unificado de antOS
    # (`antos lsp`, T12.1). No hay binario `antos-lsp` aparte: es un
    # subcomando del CLI `antos` que aporta el paquete `antosd`.
    programs.neovim = {
      enable = true;
      defaultEditor = true;
      viAlias = true;
      vimAlias = true;
      configure = {
        packages.antos.start = with pkgs.vimPlugins; [
          nvim-lspconfig
          (nvim-treesitter.withPlugins (p: [ p.rust p.python p.nix p.lua p.markdown p.bash p.javascript p.typescript ]))
          telescope-nvim
          plenary-nvim
          fzf-lua
        ];
        customRC = ''
          set number expandtab shiftwidth=2 tabstop=2
          lua << EOF
            local ok, lspconfig = pcall(require, 'lspconfig')
            if ok then
              local configs = require('lspconfig.configs')
              if not configs.antos_lsp then
                configs.antos_lsp = {
                  default_config = {
                    cmd = { 'antos', 'lsp' },
                    filetypes = { 'rust', 'python', 'javascript', 'typescript', 'lua', 'nix', 'markdown' },
                    root_dir = lspconfig.util.root_pattern('.git', 'Cargo.toml', 'flake.nix'),
                  },
                }
              end
              lspconfig.antos_lsp.setup {}
            end
          EOF
        '';
      };
    };

    programs.direnv = {
      enable = true;
      nix-direnv.enable = true;
    };

    # Flatpak para `antos app` (el remoto `flathub` se añade en el primer
    # arranque: `flatpak remote-add --if-not-exists flathub …`).
    services.flatpak.enable = true;

    environment.systemPackages = with pkgs; [
      gh
      delta
      tmux
      ripgrep
      fd
      bat
      jq
      htop
      fastfetch
      yazi
      wget
      curl
    ] ++ lib.optional (cfg.browser != null) cfg.browser;
  })

  # ── Escritorio tradicional sobre Labwc (T30.6) ────────────────────────
  (lib.mkIf cfg.panel.enable {
    environment.systemPackages = [
      cfg.panel.package
      cfg.panel.launcher
      cfg.panel.fileManager
      pkgs.swaybg
      pkgs.mako
      pkgs.swaylock
      pkgs.swayidle
      pkgs.libnotify # `notify-send`, para probar `mako`
    ];

    environment.etc."antos/desktop/waybar/config".text = builtins.toJSON waybarConfig;
    environment.etc."antos/desktop/waybar/style.css".text = waybarStyle;
    environment.etc."antos/desktop/labwc/menu.xml".text = labwcMenu;
  })
  ]);
}
