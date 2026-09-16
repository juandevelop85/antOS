# Arte de antOS derivado de `system/desktop/assets/` en tiempo de
# construcción (ImageMagick, dependencia solo de build). Lo consumen
# `desktop.nix` (fondo de escritorio y de bloqueo, icono del lanzador) e
# `iso.nix` (logo y fondo de GRUB, logo de Plymouth).
#
# Fuentes: `antos-icon.png` (1254×1254, RGB, cuadrado redondeado sobre fondo
# navy con margen) y `antos-wallpaper.png` (1672×941, RGB).
#
# Toda salida PNG va como `PNG32:` (RGBA de 8 bits): el lector PNG de GRUB
# no entiende PNG de paleta (lo que ImageMagick elige para colores planos) y
# con una sola imagen ilegible el tema entero cae a modo texto.
{ pkgs, lib }:

let
  assets = ../desktop/assets;
  font = "${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans-Bold.ttf";
  magick = "${pkgs.imagemagick}/bin/magick";
  iconSizes = [ 16 22 24 32 48 64 128 256 512 ];
in
rec {
  art = pkgs.runCommand "antos-art" { } ''
    mkdir -p $out
    # ── Icono: recortar el cuadrado redondeado (86 % centrado del original)
    #    y hacer transparentes las esquinas con una máscara redondeada.
    ${magick} ${assets}/antos-icon.png -gravity Center -crop 1078x1078+0+0 +repage \
      -resize 1024x1024 \
      \( +clone -alpha opaque -fill black -colorize 100 \
         -fill white -draw "roundrectangle 0,0,1023,1023,230,230" \) \
      -alpha off -compose CopyOpacity -composite \
      PNG32:$out/icon-1024.png
    ${lib.concatMapStringsSep "\n" (s: ''
      ${magick} $out/icon-1024.png -resize ${toString s}x${toString s} PNG32:$out/icon-${toString s}.png
    '') iconSizes}

    # ── Fondo de escritorio (tal cual) ──
    cp ${assets}/antos-wallpaper.png $out/wallpaper.png

    # ── GRUB: logo 319×100 (icono + «antOS») y fondo 1920×1080 oscurecido
    #    para que el menú lea bien encima.
    ${magick} -size 319x100 xc:none \
      \( $out/icon-1024.png -resize 92x92 \) -gravity West -geometry +2+0 -composite \
      -font ${font} -pointsize 58 \
      -gravity West -fill '#ffffff' -annotate +106+0 'ant' \
      -gravity West -fill '#7aa2f7' -annotate +212+0 'OS' \
      PNG32:$out/grub-logo.png
    ${magick} ${assets}/antos-wallpaper.png -resize 1920x1080^ -gravity Center -extent 1920x1080 \
      -fill black -colorize 35% PNG32:$out/grub-background.png
  '';

  # Iconos `hicolor` para el lanzador (`Icon=antos` en los `.desktop`).
  icons = pkgs.runCommand "antos-icons" { } ''
    ${lib.concatMapStringsSep "\n" (s: ''
      d=$out/share/icons/hicolor/${toString s}x${toString s}/apps
      mkdir -p $d && cp ${art}/icon-${toString s}.png $d/antos.png
    '') iconSizes}
  '';
}
