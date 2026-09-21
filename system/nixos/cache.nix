# El caché binario de antOS (T36.3): `services.antos.binaryCache`.
#
# `cache.nixos.org` sirve prehecho todo lo de nixpkgs; lo único que un
# `nixos-rebuild` tendría que compilar en la máquina del usuario son las
# derivaciones propias de antOS (`antosd`, `antos-barra`: Rust y GTK4,
# varios minutos y bastante RAM). La CI (`release.yml`, job `nix-cache`)
# firma y publica esas closures como un caché estático (`nix copy --to
# file://…`, sin servicio externo) en la rama `nix-cache` del repositorio,
# que GitHub Pages sirve bajo `url`.
#
# ## Estado de implementación
#
# El módulo está desactivado hasta que exista una clave: `publicKey` se lee
# de `system/nixos/cache-public-key.txt` (vacío en el árbol hasta que el
# mantenedor genere el par y suba la privada a `secrets.NIX_CACHE_SIGNING_KEY`;
# ver `docs/manual-de-comandos.md`, Método 5). Con el fichero vacío no se
# añade ningún substituter ni ninguna clave de confianza: un caché sin firma
# conocida sería un vector para meter binarios ajenos en la máquina.
{ config, lib, ... }:

let
  cfg = config.services.antos.binaryCache;
  keyFile = ./cache-public-key.txt;
  keyFromTree =
    if builtins.pathExists keyFile
    then lib.strings.trim (builtins.readFile keyFile)
    else "";
in
{
  options.services.antos.binaryCache = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = cfg.publicKey != "";
      defaultText = lib.literalExpression ''cfg.publicKey != ""'';
      description = ''
        Añadir el caché binario de antOS a `nix.settings.substituters`. Por
        defecto solo si hay clave pública conocida.
      '';
    };

    url = lib.mkOption {
      type = lib.types.str;
      default = "https://juandevelop85.github.io/antOS/cache";
      description = "URL del caché estático (directorio con `nix-cache-info`).";
    };

    publicKey = lib.mkOption {
      type = lib.types.str;
      default = keyFromTree;
      defaultText = lib.literalExpression "builtins.readFile ./cache-public-key.txt";
      description = ''
        Clave pública con la que la CI firma el caché
        (`antos-cache-1:<base64>`). Vacía = caché desactivado.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [{
      assertion = cfg.publicKey != "";
      message = "services.antos.binaryCache: activado sin publicKey; un substituter sin clave de confianza no se acepta.";
    }];
    nix.settings.substituters = [ cfg.url ];
    nix.settings.trusted-public-keys = [ cfg.publicKey ];
  };
}
