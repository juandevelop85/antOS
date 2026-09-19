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
  isLabwc = cfg.flavor == "labwc";
  isPlasma = cfg.flavor == "plasma";
  # Arte de antOS (icono y fondo de `system/desktop/assets/`, derivados en
  # construcción; ver `branding.nix`).
  branding = import ./branding.nix { inherit pkgs lib; };

  # Variables de entorno de la sesión Wayland — el contenido de
  # `system/desktop/environment`, declarado aquí para que sea parte de la
  # definición del sistema. Con el sabor `plasma`, `XDG_CURRENT_DESKTOP` y
  # `XDG_SESSION_DESKTOP` los fija `startplasma-wayland` a `KDE` (portales,
  # filtrado de `.desktop`, etc.): no se declaran `antOS` para no contradecirlo.
  sessionEnv = lib.optionalAttrs isLabwc {
    XDG_CURRENT_DESKTOP = "antOS";
    XDG_SESSION_DESKTOP = "antOS";
  } // {
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
  # `$HOME` del usuario (Labwc lee de `~/.config/labwc`, `autostart`
  # incluido: lo ejecuta él mismo con `sh` cuando el socket Wayland está
  # listo) y ejecuta el compositor. Equivale a
  # `system/desktop/start-session.sh` pero con rutas del store.
  #
  # El `autostart` NO se pasa además con `labwc -s`: Labwc ejecuta ambos
  # (`session_autostart_init` + el comando de `-s`), el guion corría dos
  # veces en paralelo y las guardas `pgrep` de cada instancia veían al
  # `pgrep` de la otra — ningún cliente arrancaba y el escritorio quedaba
  # negro (sin `antos-barra`, sin panel, sin fondo).
  sessionScript = pkgs.writeShellScript "antos-desktop-session" ''
    set -e

    # Predefinir rutas base XDG por si no vienen fijadas en el entorno
    export HOME="''${HOME:-/home/${cfg.autologinUser}}"
    export XDG_CONFIG_HOME="''${XDG_CONFIG_HOME:-$HOME/.config}"
    export XDG_DATA_HOME="''${XDG_DATA_HOME:-$HOME/.local/share}"
    export XDG_STATE_HOME="''${XDG_STATE_HOME:-$HOME/.local/state}"
    export XDG_CACHE_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}"

    # Cargar el entorno global de NixOS: PATH (/run/current-system/sw/bin),
    # XDG_DATA_DIRS, XDG_CONFIG_DIRS y variables de sesión declaradas.
    # Desactivamos set -u porque /etc/set-environment se diseñó para shells
    # estándar de login (/etc/profile) y evalúa variables opcionales.
    if [ -f /etc/set-environment ]; then
      set +u
      . /etc/set-environment
    fi

    mkdir -p "$XDG_CONFIG_HOME/labwc"
    cp -f /etc/antos/desktop/rc.xml "$XDG_CONFIG_HOME/labwc/rc.xml"
    cp -f /etc/antos/desktop/autostart "$XDG_CONFIG_HOME/labwc/autostart"
    ${lib.optionalString cfg.panel.enable ''
      cp -f /etc/antos/desktop/labwc/menu.xml "$XDG_CONFIG_HOME/labwc/menu.xml"
    ''}
    chmod +x "$XDG_CONFIG_HOME/labwc/autostart"

    # Lanzar el compositor dentro de una sesión D-Bus propia para que
    # Waybar (módulo tray / SNI), Mako (notificaciones) y antos-barra
    # tengan un bus de sesión funcional.
    exec ${pkgs.dbus}/bin/dbus-run-session ${lib.getExe cfg.compositor}
  '';

  # ── Escritorio tradicional opcional (T30.6) ──────────────────────────────
  # `autostart` base (demonio + antos-barra) de `system/desktop/`, con —si el
  # panel está activado— las piezas `wlroots` del mobiliario clásico
  # insertadas en la marca `# @antos:panel@`, es decir, ANTES de
  # `antos-barra` (ver el porqué en el propio `autostart`). Se genera aquí
  # para no meter waybar/swaybg en el `autostart` compartido que también usa
  # `start-session.sh` sobre Linux no-NixOS.
  panelAutostart = ''
    # ── Escritorio tradicional (T30.6) ──
    # `_a` lanza en segundo plano solo si no hay ya una instancia: la sesión
    # de greetd se reinicia cuando el compositor sale (y el primer arranque en
    # frío bajo emulación puede provocarlo), y sin esta guarda quedarían dos
    # de cada cliente apilados. La guarda es `_running` del `autostart` base
    # (patrón anclado al ejecutable; ver allí por qué ni `pgrep -f <patrón>`
    # suelto ni `pgrep -x` sirven).
    _a() { p="$1"; shift; if ! _running "$p"; then "$@" >> /tmp/antos-desktop.log 2>&1 & fi; }

    _a swaybg ${pkgs.swaybg}/bin/swaybg ${
      if cfg.panel.wallpaper != null
      then ''-i "${cfg.panel.wallpaper}" -m fill''
      else "-c '#1a1b26'"
    }
    _a waybar ${lib.getExe cfg.panel.package} -c /etc/antos/desktop/waybar/config -s /etc/antos/desktop/waybar/style.css
    _a mako ${pkgs.mako}/bin/mako
    ${lib.optionalString (cfg.panel.idleLockSeconds > 0) ''
      _a swayidle ${pkgs.swayidle}/bin/swayidle -w timeout ${toString cfg.panel.idleLockSeconds} '${pkgs.swaylock}/bin/swaylock -f -c 1a1b26 --indicator-idle-visible'
    ''}

    # Esperar (hasta 3 s) a que waybar publique `org.kde.StatusNotifierWatcher`
    # antes de que `antos-barra` intente registrar su icono de bandeja. Si
    # waybar tarda más, seguimos igual: ksni se reengancha cuando el watcher
    # aparece claramente después.
    _i=0
    while [ "$_i" -lt 30 ]; do
      if ${pkgs.dbus}/bin/dbus-send --session --print-reply --dest=org.freedesktop.DBus \
           /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
           string:org.kde.StatusNotifierWatcher 2>/dev/null | grep -q 'boolean true'; then
        break
      fi
      sleep 0.1
      _i=$((_i + 1))
    done
  '';

  # ── Escala de salida (HiDPI) ─────────────────────────────────────────
  # Labwc no configura salidas desde `rc.xml`; se hace por
  # `wlr-output-management` con `wlr-randr`. Con `outputScale = "auto"`, una
  # salida de ≥ 2560 px de ancho pasa a escala 2: es el caso de QEMU en
  # macOS a pantalla completa, donde Cocoa entrega al invitado la
  # resolución nativa Retina (p. ej. 3024×1900) y a escala 1 el panel y la
  # barra quedan minúsculos. Como la resolución de `virtio-gpu` sigue al
  # tamaño de la ventana del host (entrar/salir de pantalla completa la
  # cambia en caliente), un vigilante reaplica la escala tras cada cambio
  # de modo. Sondea `wlr-randr --json` cada 2 s en vez de escuchar udev:
  # un modeset iniciado dentro del invitado (p. ej. `wlr-randr --mode`) no
  # emite evento `drm`, y el sondeo cubre ambos casos con un coste nulo.
  outputScaleSnippet = let
    wantScale =
      if cfg.outputScale == "auto"
      then ''if [ "$width" -ge 2560 ]; then want=2; else want=1; fi''
      else ''want=${toString cfg.outputScale}'';
  in ''
    _apply_output_scale() {
      ${lib.getExe pkgs.wlr-randr} --json 2>/dev/null \
        | ${lib.getExe pkgs.jq} -r '.[] | select(.enabled) | "\(.name) \(.scale * 1) \((.modes[] | select(.current) | .width) // 0)"' \
        | while read -r name scale width; do
            ${wantScale}
            if [ "$scale" != "$want" ]; then
              ${lib.getExe pkgs.wlr-randr} --output "$name" --scale "$want" >> /tmp/antos-desktop.log 2>&1 || true
            fi
          done
    }
    _apply_output_scale
    ( while sleep 2; do _apply_output_scale; done ) &
  '';

  # ── Sabor Plasma (T30.9) ─────────────────────────────────────────────
  # Sesión: el mismo `greetd` con autologin, pero `exec startplasma-wayland`.
  # Sin `dbus-run-session`: Plasma 6 arranca por `systemd --user` y usa el
  # bus de usuario (`$XDG_RUNTIME_DIR/bus`); un bus propio lo rompería.
  plasmaSessionScript = pkgs.writeShellScript "antos-plasma-session" ''
    set -e
    if [ -f /etc/set-environment ]; then
      set +u
      . /etc/set-environment
    fi
    exec ${pkgs.kdePackages.plasma-workspace}/bin/startplasma-wayland
  '';

  # Escala HiDPI bajo Plasma: mismo criterio y mismo sondeo que
  # `outputScaleSnippet`, pero con `kscreen-doctor` (KScreen es quien manda
  # sobre las salidas en Plasma; `wlr-randr` funcionaría contra KWin pero
  # KScreen lo pisaría al reaplicar su configuración guardada).
  plasmaScaleScript = let
    kscreenDoctor = "${pkgs.kdePackages.libkscreen}/bin/kscreen-doctor";
    wantScale =
      if cfg.outputScale == "auto"
      then ''if [ "$width" -ge 2560 ]; then want=2; else want=1; fi''
      else ''want=${toString cfg.outputScale}'';
  in pkgs.writeShellScript "antos-plasma-output-scale" ''
    _apply_output_scale() {
      ${kscreenDoctor} -j 2>/dev/null \
        | ${lib.getExe pkgs.jq} -r '.outputs[] | select(.enabled) | . as $o | "\(.name) \(.scale * 1) \(($o.modes[] | select(.id == $o.currentModeId) | .size.width) // 0)"' \
        | while read -r name scale width; do
            ${wantScale}
            if [ "$scale" != "$want" ]; then
              ${kscreenDoctor} "output.$name.scale.$want" >> /tmp/antos-desktop.log 2>&1 || true
            fi
          done
    }
    _apply_output_scale
    while sleep 2; do _apply_output_scale; done
  '';

  # Super+Space bajo Plasma (T33.4): KWin no ejecuta el `rc.xml` de Labwc,
  # pero kglobalaccel registra el atajo `X-KDE-Shortcuts` de cualquier
  # entrada `.desktop` instalada. El atajo activa el icono SNI de la barra
  # por D-Bus — lo mismo que un clic en la bandeja, y lo único que la barra
  # entiende como «abrir/cerrar» (T30.8) — en vez de lanzar otra instancia.
  barraToggleScript = pkgs.writeShellScript "antos-barra-toggle" ''
    dbus=${pkgs.dbus}/bin/dbus-send
    name=$($dbus --session --print-reply --dest=org.freedesktop.DBus \
             /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>/dev/null \
           | grep -o 'org.kde.StatusNotifierItem-[0-9]*-[0-9]*' \
           | while read -r n; do
               pid=''${n#org.kde.StatusNotifierItem-}; pid=''${pid%%-*}
               if tr '\0' ' ' < /proc/"$pid"/cmdline 2>/dev/null | grep -q antos-barra; then
                 echo "$n"; break
               fi
             done)
    if [ -z "$name" ]; then
      exec ${lib.getExe cfg.barra}
    fi
    exec $dbus --session --dest="$name" /StatusNotifierItem \
      org.kde.StatusNotifierItem.Activate int32:0 int32:0
  '';

  barraToggleDesktopItem = pkgs.makeDesktopItem {
    name = "antos-barra-toggle";
    desktopName = "antOS · abrir o cerrar la barra de intención";
    exec = "${barraToggleScript}";
    icon = "antos";
    noDisplay = true;
    extraConfig = {
      "X-KDE-Shortcuts" = "Meta+space";
    };
  };

  # Entrada de lanzador para la barra (Kickoff la muestra bajo «Utilidades»).
  barraDesktopItem = pkgs.makeDesktopItem {
    name = "antos-barra";
    desktopName = "Barra de intención antOS";
    comment = "Barra de intención y centro de agentes de antOS";
    exec = lib.getExe cfg.barra;
    icon = "antos"; # `branding.icons` (hicolor)
    categories = [ "Utility" "System" ];
  };

  # Aspecto de Plasma al iniciar sesión: tema Breeze Dark (a juego con el
  # arte de antOS) y fondo de escritorio. `plasma-apply-lookandfeel` y
  # `plasma-apply-wallpaperimage` hablan con plasmashell por D-Bus, así que
  # se espera a que esté en el bus; el tema va ANTES del fondo porque
  # aplicar un paquete look-and-feel puede reponer el fondo por defecto.
  # Se aplica UNA vez por usuario (marca en `~/.config`): en la ISO en vivo
  # el `$HOME` es nuevo en cada arranque, y en un sistema instalado no
  # pisa lo que el usuario haya elegido después.
  plasmaLookScript = pkgs.writeShellScript "antos-plasma-look" ''
    marker="''${XDG_CONFIG_HOME:-$HOME/.config}/antos-look-applied"
    [ -e "$marker" ] && exit 0
    for _ in $(seq 1 60); do
      if ${pkgs.dbus}/bin/dbus-send --session --print-reply --dest=org.freedesktop.DBus \
           /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner string:org.kde.plasmashell \
           2>/dev/null | grep -q 'boolean true'; then
        break
      fi
      sleep 1
    done
    sleep 2
    ${pkgs.kdePackages.plasma-workspace}/bin/plasma-apply-lookandfeel -a org.kde.breezedark.desktop 2>/dev/null || true
    sleep 2
    ${pkgs.kdePackages.plasma-workspace}/bin/plasma-apply-wallpaperimage ${branding.art}/wallpaper.png \
      && touch "$marker"
  '';

  sessionAutostart = pkgs.writeShellScript "antos-autostart"
    (builtins.replaceStrings [ "# @antos:outputs@" "# @antos:panel@" ]
      [ outputScaleSnippet (lib.optionalString cfg.panel.enable panelAutostart) ]
      (builtins.readFile ../desktop/autostart));

  # Panel superior: botón de menú (→ lanzador), reloj, CPU/RAM/red y bandeja.
  # Formatos de texto: sin dependencia de una fuente de iconos.
  waybarConfig = {
    layer = "top";
    position = "top";
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
      /* El borde va en el lado que da hacia el escritorio: con el panel
         arriba, eso es el borde inferior (antes era border-top, cuando
         el panel vivía abajo). */
      border-bottom: 1px solid #2a2e3f;
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

  # `fuzzel` sin configurar arranca con `lines=15`/`width=30` (defaults de
  # fuzzel.ini) y centrado en pantalla: en un sistema NixOS con decenas de
  # entradas `.desktop` (muchas del propio sistema, no solo apps de
  # usuario — p. ej. el Manual de NixOS, que fuzzel no filtra por
  # `XDG_CURRENT_DESKTOP` salvo que se le pida con `filter-desktop`) esa
  # ventana se queda corta y hay que desplazarse para ver el resto. La
  # agrandamos y la anclamos arriba, justo bajo el panel (ahora también
  # arriba), en vez de dejarla centrada.
  #
  # `dpi-aware=no` (T30.6, seguimiento): fuzzel por defecto escala la fuente
  # con el DPI físico que anuncia la salida, y QEMU-Cocoa en macOS informa
  # de un tamaño físico erróneo (la mitad del real → ~256 DPI), con lo que el
  # menú salía a 2,7× — «desproporcionado». Con `no`, la fuente sigue solo
  # la escala de la salida (la misma que usan waybar y antos-barra). `foot`
  # ya trae `dpi-aware=no` por defecto y no lo sufre.
  # Colores: los del panel (`waybarStyle`), para que el lanzador no sea una
  # ventana crema sobre un escritorio oscuro.
  fuzzelConfig = ''
    [main]
    font=monospace:size=11
    dpi-aware=no
    lines=16
    width=50
    anchor=top
    y-margin=36
    horizontal-pad=16
    vertical-pad=10
    inner-pad=6

    [colors]
    background=14151cf2
    text=c0caf5ff
    prompt=7aa2f7ff
    input=c0caf5ff
    match=7aa2f7ff
    selection=2a2e3fff
    selection-text=ffffffff
    selection-match=7aa2f7ff
    border=2a2e3fff

    [border]
    width=1
    radius=8
  '';

  labwcMenu = ''
    <?xml version="1.0" encoding="UTF-8"?>
    <openbox_menu>
      <menu id="root-menu" label="antOS">
        <item label="Terminal">
          <action name="Execute" command="${lib.getExe cfg.terminal}"/>
        </item>
        <item label="Barra de intención antOS">
          <action name="Execute" command="${lib.getExe cfg.barra} --panel"/>
        </item>
        <item label="Archivos">
          <action name="Execute" command="${lib.getExe cfg.panel.fileManager}"/>
        </item>
        <item label="Editor">
          <action name="Execute" command="${lib.getExe cfg.terminal} -e antos edit"/>
        </item>
        <item label="antOS · dev">
          <action name="Execute" command="${lib.getExe cfg.terminal} -e antos dev"/>
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

    flavor = lib.mkOption {
      type = lib.types.enum [ "labwc" "plasma" ];
      default = "labwc";
      description = ''
        Sabor del escritorio (T30.9). `labwc`: compositor `wlroots` ligero +
        `antos-barra` + el mobiliario opcional de `panel` (T30.6). `plasma`:
        KDE Plasma 6 en Wayland completo (KWin, plasmashell, Dolphin,
        Konsole, Ajustes…) con `antos-barra` anclada arriba por layer-shell
        y su icono en la bandeja de Plasma; `panel` y `compositor` se
        ignoran. Ambos usan el mismo autologin por `greetd`.
      '';
    };

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

    toolchains = lib.mkOption {
      type = lib.types.listOf (lib.types.enum [ "node" "python" "rust" "go" ]);
      default = [ ];
      example = [ "node" "python" ];
      description = ''
        Toolchains de proyecto en la imagen (T35.2). Vacío a propósito: por
        defecto `antos "crea un proyecto …"` trae el toolchain por proyecto
        con `nix shell nixpkgs#…` según el stack, y la imagen no engorda.
        Quien prefiera `node`/`cargo`/`uv`/`go` globales los declara aquí.
      '';
    };

    outputScale = lib.mkOption {
      type = lib.types.either (lib.types.enum [ "auto" ]) lib.types.number;
      default = "auto";
      example = 2;
      description = ''
        Escala de las salidas Wayland, aplicada con `wlr-randr` al arrancar
        la sesión y tras cada cambio de modo (evento `drm` de udev).
        `"auto"`: escala 2 en salidas de ≥ 2560 px de ancho (QEMU en macOS
        a pantalla completa entrega la resolución Retina nativa), 1 en el
        resto. Un número fija esa escala en todas las salidas.
      '';
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
        default = pkgs.thunar;
        defaultText = lib.literalExpression "pkgs.thunar";
        description = "Gestor de archivos gráfico.";
      };

      wallpaper = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = "${branding.art}/wallpaper.png";
        defaultText = lib.literalExpression "el fondo de `system/desktop/assets/`";
        description = "Imagen de fondo. `null` → color sólido.";
      };

      idleLockSeconds = lib.mkOption {
        type = lib.types.ints.unsigned;
        default = 600;
        description = ''
          Segundos de inactividad tras los que `swayidle` bloquea la sesión
          con `swaylock` (contraseña del usuario de la sesión). `0` desactiva
          el bloqueo automático.
        '';
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

    # El escritorio trae el motor de modelos locales de serie (T34.3):
    # Ollama en loopback desde el arranque, sin modelos hasta que el usuario
    # los pida (`antos llm setup`). `services.antos.llm.enable = false` lo
    # quita de la imagen.
    services.antos.llm.enable = lib.mkDefault true;

    environment.systemPackages = [
      cfg.barra
      cfg.terminal
      cfg.editor
      pkgs.wl-clipboard
      pkgs.git
      pkgs.jq
    ];

    environment.sessionVariables = sessionEnv;

    # Entrada de sesión Wayland + autologin. `greetd` con `default_session`
    # arranca la sesión de antOS para `autologinUser` sin más pasos; el
    # comando depende del sabor (T30.9).
    services.greetd = {
      enable = true;
      settings.default_session = {
        command = if isPlasma then "${plasmaSessionScript}" else "${sessionScript}";
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

  }

  # ── Sabor Labwc (por defecto) ──────────────────────────────────────────
  (lib.mkIf isLabwc {
    environment.systemPackages = [
      cfg.compositor
      pkgs.grim # captura de pantalla (wlr-screencopy) para QA visual (T14.2)
      pkgs.slurp
      pkgs.wlr-randr # escala HiDPI de las salidas (`outputScale`)
    ];

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

    xdg.portal = {
      enable = true;
      extraPortals = [ pkgs.xdg-desktop-portal-wlr ];
      config.common.default = "wlr";
    };
  })

  # ── Sabor Plasma 6 Wayland (T30.9) ─────────────────────────────────────
  (lib.mkIf isPlasma {
    services.desktopManager.plasma6.enable = true;

    environment.systemPackages = [
      barraDesktopItem
      barraToggleDesktopItem # Super+Space (T33.4)
      branding.icons
      pkgs.kdePackages.libkscreen # `kscreen-doctor` (escala HiDPI)
    ];

    # Fondo de la pantalla de bloqueo (kscreenlocker lee `$XDG_CONFIG_DIRS`).
    environment.etc."xdg/kscreenlockerrc".text = ''
      [Greeter][Wallpaper][org.kde.image][General]
      Image=file://${branding.art}/wallpaper.png
      PreviewImage=file://${branding.art}/wallpaper.png
    '';

    # `antos-barra` arranca con la sesión por autostart XDG (Plasma lo
    # convierte en una unidad `systemd --user`), y el vigilante de escala
    # igual. `OnlyShowIn=KDE`: no interfiere si el usuario elige otra sesión.
    environment.etc."xdg/autostart/antos-barra.desktop".text = ''
      [Desktop Entry]
      Type=Application
      Name=Barra de intención antOS
      Exec=${lib.getExe cfg.barra}
      OnlyShowIn=KDE;
      X-KDE-autostart-phase=2
    '';
    environment.etc."xdg/autostart/antos-output-scale.desktop".text = ''
      [Desktop Entry]
      Type=Application
      Name=antOS · escala HiDPI de las salidas
      Exec=${plasmaScaleScript}
      OnlyShowIn=KDE;
      X-KDE-autostart-phase=2
    '';
    environment.etc."xdg/autostart/antos-look.desktop".text = ''
      [Desktop Entry]
      Type=Application
      Name=antOS · tema y fondo de escritorio
      Exec=${plasmaLookScript}
      OnlyShowIn=KDE;
      X-KDE-autostart-phase=2
    '';
  })

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

  # ── Toolchains globales opcionales (T35.2) ────────────────────────────
  (lib.mkIf (cfg.toolchains != [ ]) {
    environment.systemPackages =
      lib.optionals (builtins.elem "node" cfg.toolchains) [ pkgs.nodejs_22 ]
      ++ lib.optionals (builtins.elem "python" cfg.toolchains) [ pkgs.python312 pkgs.uv ]
      ++ lib.optionals (builtins.elem "rust" cfg.toolchains) [ pkgs.cargo pkgs.rustc ]
      ++ lib.optionals (builtins.elem "go" cfg.toolchains) [ pkgs.go ];
  })

  # ── Escritorio tradicional sobre Labwc (T30.6) ────────────────────────
  # Solo con el sabor Labwc: bajo Plasma el mobiliario lo trae Plasma.
  (lib.mkIf (isLabwc && cfg.panel.enable) {
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

    # Sin esta entrada PAM, `swaylock` usa la pila «other» de NixOS
    # (`pam_warn` + denegar) y rechaza siempre la contraseña: la sesión
    # quedaba bloqueada en gris para siempre a los `idleLockSeconds` de
    # inactividad (visto en la VM: «Wrong» con la contraseña correcta).
    security.pam.services.swaylock = {};

    environment.etc."antos/desktop/waybar/config".text = builtins.toJSON waybarConfig;
    environment.etc."antos/desktop/waybar/style.css".text = waybarStyle;
    environment.etc."antos/desktop/labwc/menu.xml".text = labwcMenu;
    # `/etc/xdg` es el valor por defecto de `XDG_CONFIG_DIRS` cuando la
    # variable no está fijada (especificación XDG Base Directory) — no
    # hace falta declararla en `sessionEnv` para que fuzzel la encuentre.
    environment.etc."xdg/fuzzel/fuzzel.ini".text = fuzzelConfig;
  })
  ]);
}
