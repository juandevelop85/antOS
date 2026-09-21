# ISO instalable / Live de antOS Linux.
#
# Es "antOS Linux" (Método 5), NO el Live del kernel bare-metal que produce
# `antos usb build` (Método 6). Arranca a la sesión gráfica de antOS y trae el
# instalador de NixOS disponible.
#
# ## Autosuficiente (T36.3)
#
# La ISO lleva dentro (a) la closure de la máquina instalada de referencia
# (`installed.nix`: lo mismo que `antos install` genera) para que
# `nixos-install` no construya ni descargue nada, (b) el árbol fuente de
# antOS en `/etc/antos/source` (lo que el instalador copia a
# `/etc/nixos/antos` del destino) y (c) la fuente de `nixpkgs` en el store
# (`nix.registry`), para que el `flake.nix` generado evalúe sin red. Los
# tres llegan por `_module.args` desde `flake.nix`; si falta alguno (una
# evaluación fuera del flake), la ISO se construye igual pero no es
# autosuficiente, y `antos install --apply` lo dirá al no encontrar
# `/etc/antos/source`.
{ config, lib, pkgs, modulesPath
, antosSource ? null
, nixpkgsFlake ? null
, installedSystem ? null
, antosVersion ? "unknown"
, ... }:

let
  # ── Marca de arranque (T30.9, seguimiento) ────────────────────────────
  # El menú de arranque de la ISO (GRUB) y el splash de Plymouth salían con
  # la marca de NixOS. El icono y el fondo oficiales viven en
  # `system/desktop/assets/` y `branding.nix` deriva de ellos el logo de GRUB
  # (icono + «antOS»), el fondo oscurecido y el icono de Plymouth.
  branding = import ./branding.nix { inherit pkgs lib; };
  magick = "${pkgs.imagemagick}/bin/magick";

  # Tema de GRUB: parte del de NixOS (fuentes `.pf2`, iconos, cajas del
  # terminal) y sustituye logotipo, fondo, cuadro de selección y `theme.txt`
  # por una versión oscura. El menú va sin caja (transparente sobre el fondo).
  # Toda imagen generada va como `PNG32:` (RGBA): el lector PNG de GRUB no
  # entiende PNG de paleta y con una sola imagen ilegible cae a modo texto.
  antosGrubTheme = pkgs.runCommand "antos-grub2-theme" { } ''
    mkdir -p $out
    cp -r ${pkgs.nixos-grub2-theme}/. $out/
    chmod -R u+w $out
    rm -f $out/boot_menu_*.png
    cp ${branding.art}/grub-logo.png $out/logo.png
    cp ${branding.art}/grub-background.png $out/background.png
    # Cuadro de selección: nueve piezas de color sólido (GRUB exige el juego
    # completo para `selected_item_pixmap_style`).
    for p in c n s e w ne nw se sw; do
      ${magick} -size 8x8 xc:'#2a2e3f' PNG32:$out/select_$p.png
    done
    cat > $out/theme.txt <<'THEME'
    # Tema de arranque de antOS Linux (derivado del de NixOS; paleta antOS-Dark).
    title-text: ""
    title-font: "DejaVu Regular"
    title-color: "#c0caf5"

    + image {
    	top = 3%
    	height = 100
    	width = 319
    	left = 50%-160
    	file = "logo.png"
    }

    desktop-image: "background.png"
    desktop-color: "#1a1b26"
    message-font: "DejaVu Regular"
    message-color: "#c0caf5"

    terminal-font: "Unifont Regular"
    terminal-box: "terminal_*.png"

    + progress_bar {
    	id = "__timeout__"
    	top = 95%-32
    	left = 50%-25%
    	height = 32
    	width = 50%
    	show_text = true
    	text = "@TIMEOUT_NOTIFICATION_MIDDLE@"
    	font = "DejaVu Regular"
    	text_color = "#c0caf5"
    	border_color = #2a2e3f
    	bg_color = #14151c
    	fg_color = #7aa2f7
    }

    + boot_menu {
    	left = 50%-400
    	width = 800
    	top = 3%+100+3%
    	height = 100%-3%-100-3%-3%-32-3%

    	item_font = "DejaVu Regular"
    	item_color = "#c0caf5"
    	item_height = 40
    	item_icon_space = 12
    	item_spacing = 4
    	item_padding = 8

    	selected_item_font = "DejaVu Regular"
    	selected_item_color = "#ffffff"
    	selected_item_pixmap_style = "select_*.png"

    	icon_height = 32
    	icon_width = 42
    	scrollbar = false
    }
    THEME
    # El heredoc va indentado dentro del `runCommand`; quitar la sangría.
    sed -i 's/^    //' $out/theme.txt
  '';
in
{
  imports = [
    "${modulesPath}/installer/cd-dvd/installation-cd-graphical-base.nix"
  ];

  # Identidad: «antOS Linux 0.1 · escritorio en vivo» en el menú de arranque
  # y en `/etc/os-release` (lo que Plasma muestra en «Acerca de este
  # sistema»). `label` es la versión de antOS, no la de nixpkgs.
  system.nixos.distroName = "antOS Linux";
  system.nixos.label = lib.mkForce "0.1";
  isoImage.appendToMenuLabel = " · escritorio en vivo";
  isoImage.grubTheme = antosGrubTheme;
  # 10 s de espera en el menú era demasiado para una ISO que arranca sola.
  boot.loader.timeout = lib.mkForce 3;
  # Splash de arranque (Plymouth lo activa la base gráfica del instalador).
  boot.plymouth.logo = "${branding.art}/icon-256.png";

  services.antos.enable = true;
  services.antos.desktop.enable = true;
  # La verificación automatizada de la instalación (T36.2,
  # `system/nixos/install-smoke.sh`) le pasa un guion por `fw_cfg`; sin QEMU
  # el servicio no hace nada.
  services.antos.smoke.enable = true;
  # La sesión en vivo corre desde RAM: contraseña de desarrollo para poder
  # usar `sudo` (el instalador corre como root). La del sistema instalado la
  # elige el asistente (T36.4).
  users.users.${config.services.antos.desktop.autologinUser}.initialPassword = "antos";
  # La ISO en vivo trae el escritorio completo (T30.9): KDE Plasma 6 Wayland
  # con `antos-barra` arriba. `"labwc"` devuelve la sesión ligera de T30.6.
  services.antos.desktop.flavor = "plasma";
  # La ISO en vivo corre desde RAM: sin motor de modelos de serie (T34.3).
  # El sistema instalado desde ella sí lo trae, porque hereda `desktop`.
  services.antos.llm.enable = false;

  # `isoImage.isoName` se renombró a `image.fileName` en nixpkgs recientes.
  # El nombre lleva la arquitectura: la release publica una ISO por cada una.
  image.fileName = lib.mkForce "antos-linux-${antosVersion}-${pkgs.stdenv.hostPlatform.uname.processor}.iso";
  isoImage.volumeID = lib.mkForce "ANTOS_LINUX";
  # Compresión fija: el tamaño de la ISO es una cifra que se documenta y se
  # compara entre releases; no puede depender del valor por defecto del día.
  isoImage.squashfsCompression = "zstd -Xcompression-level 15";

  # ── Autosuficiencia (T36.3) ───────────────────────────────────────────
  # La closure de la máquina instalada de referencia viaja en el store de la
  # ISO: `nixos-install` copia de ahí y no construye nada.
  isoImage.storeContents = lib.optional (installedSystem != null) installedSystem;

  # El árbol fuente de antOS, en la forma exacta que `antos install` copia al
  # destino (`DeployEngine::copy_antos_source`, T36.1). Sin `docs/`, sin
  # `target/`: solo lo que el flake necesita para evaluar y construir.
  environment.etc = lib.mkMerge [
    (lib.mkIf (antosSource != null) {
      "antos/source".source = lib.fileset.toSource {
        root = antosSource;
        # `maybeMissing`: `rust-toolchain.toml` o `builder/` pueden faltar
        # en un árbol recortado, igual que en `copy_antos_source`.
        fileset = lib.fileset.unions (map lib.fileset.maybeMissing [
          (antosSource + "/flake.nix")
          (antosSource + "/flake.lock")
          (antosSource + "/Cargo.toml")
          (antosSource + "/Cargo.lock")
          (antosSource + "/rust-toolchain.toml")
          (antosSource + "/recipes")
          (antosSource + "/builder/Cargo.toml")
          (antosSource + "/builder/src")
          (antosSource + "/system")
        ]);
      };
    })
    { "antos/VERSION".text = "${antosVersion}\n"; }
  ];

  # La fuente de nixpkgs en el store: el `flake.nix` generado la fija por
  # `rev`+`narHash` y nix la encuentra ahí sin salir a la red.
  nix.registry = lib.mkIf (nixpkgsFlake != null) {
    nixpkgs.flake = nixpkgsFlake;
  };

  # Las herramientas del instalador, declaradas y no heredadas del perfil:
  # son las que `DeployEngine::real_install_preconditions` exige.
  environment.systemPackages = with pkgs; [
    parted
    dosfstools
    e2fsprogs
    util-linux
    efibootmgr
    nixos-install-tools
    # `mkpasswd -m yescrypt --stdin`: el asistente convierte la contraseña
    # en hash con él (T36.4); la contraseña en claro no sale de la memoria.
    mkpasswd
  ];

  # La base gráfica del instalador ya trae su propio compositor de rescate; el
  # de antOS (Labwc + antos-barra) es el que se autoinicia por `greetd`.
  networking.hostName = lib.mkForce "antos-live";
}
