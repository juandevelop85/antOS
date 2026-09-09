# El módulo que convierte a antOS en parte del sistema.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.antos;
in
{
  options.services.antos = {
    enable = lib.mkEnableOption "antOS como capa de sistema";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.antosd;
      description = "El paquete de antosd que usará el sistema.";
    };

    workspace = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/antos/workspace";
      description = "El único sitio donde las capacidades pueden tocar ficheros.";
    };

    state = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/antos/estado";
      description = "Instantáneas, bitácora y concesiones.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # Las rutas van en el entorno de todo el sistema para que `antos` haga lo
    # mismo lo lances desde donde lo lances.
    environment.variables = {
      ANTOS_WORKSPACE = cfg.workspace;
      ANTOS_STATE = cfg.state;
      ANTOS_CAPABILITIES = "${cfg.package}/share/antos/capabilities";
      ANTOS_SYSTEM_CONFIG = "/etc/nixos";
      # Compatibilidad hacia atrás con la marca `syso` de antes del
      # renombrado (T31.11): un ciclo de transición suave — `antos` siempre
      # prefiere el `ANTOS_*` de arriba, así que estas nunca se leen aquí,
      # pero cualquier otra herramienta del sistema que todavía las exporte
      # o las espere sigue funcionando mientras se retiran del todo.
      SYSO_WORKSPACE = cfg.workspace;
      SYSO_STATE = cfg.state;
      SYSO_CAPABILITIES = "${cfg.package}/share/antos/capabilities";
      SYSO_SYSTEM_CONFIG = "/etc/nixos";
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.workspace} 0755 root root -"
      "d ${cfg.state} 0700 root root -"
    ];

    # antOS comprueba su propio recinto al arrancar, antes de que nadie pueda
    # pedirle nada.
    systemd.services.antos-doctor = {
      description = "antOS · comprobar que el recinto es real";
      wantedBy = [ "multi-user.target" ];
      after = [ "local-fs.target" ];
      environment = config.environment.variables;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${lib.getExe cfg.package} doctor";
        StandardOutput = "journal+console";
        StandardError = "journal+console";
      };
    };
  };
}
