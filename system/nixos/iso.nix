# ISO instalable / Live de antOS Linux.
#
# Es "antOS Linux" (Método 5), NO el Live del kernel bare-metal que produce
# `antos usb build` (Método 6). Arranca a la sesión gráfica de antOS y trae el
# instalador de NixOS disponible.
{ lib, pkgs, modulesPath, ... }:

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
  # La ISO en vivo trae el escritorio completo (T30.9): KDE Plasma 6 Wayland
  # con `antos-barra` arriba. `"labwc"` devuelve la sesión ligera de T30.6.
  services.antos.desktop.flavor = "plasma";

  # `isoImage.isoName` se renombró a `image.fileName` en nixpkgs recientes.
  image.fileName = lib.mkForce "antos-linux-${lib.version}.iso";
  isoImage.volumeID = lib.mkForce "ANTOS_LINUX";

  # La base gráfica del instalador ya trae su propio compositor de rescate; el
  # de antOS (Labwc + antos-barra) es el que se autoinicia por `greetd`.
  networking.hostName = lib.mkForce "antos-live";
}
