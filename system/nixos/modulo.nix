# El módulo que convierte a syso en parte del sistema.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.syso;
in
{
  options.services.syso = {
    enable = lib.mkEnableOption "syso como capa de sistema";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.sysod;
      description = "El paquete de sysod que usará el sistema.";
    };

    workspace = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/syso/workspace";
      description = "El único sitio donde las capacidades pueden tocar ficheros.";
    };

    state = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/syso/estado";
      description = "Instantáneas, bitácora y concesiones.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # Las rutas van en el entorno de todo el sistema para que `syso` haga lo
    # mismo lo lances desde donde lo lances.
    environment.variables = {
      SYSO_WORKSPACE = cfg.workspace;
      SYSO_STATE = cfg.state;
      SYSO_CAPABILITIES = "${cfg.package}/share/syso/capabilities";
      SYSO_SYSTEM_CONFIG = "/etc/nixos";
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.workspace} 0755 root root -"
      "d ${cfg.state} 0700 root root -"
    ];

    # syso comprueba su propio recinto al arrancar, antes de que nadie pueda
    # pedirle nada.
    #
    # Es lo contrario de confiar: si Landlock no está en este kernel, o si el
    # confinamiento no se comporta como dice, se quiere saber ahora y no la
    # primera vez que una capacidad se salga de su sitio.
    systemd.services.syso-doctor = {
      description = "syso · comprobar que el recinto es real";
      wantedBy = [ "multi-user.target" ];
      after = [ "local-fs.target" ];
      environment = config.environment.variables;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${lib.getExe cfg.package} doctor";
        # A la consola, no solo al diario. Una comprobación de seguridad que
        # solo se ve rebuscando en los registros es una comprobación que nadie
        # mira: si el recinto no se comporta, tiene que salir en el arranque.
        StandardOutput = "journal+console";
        StandardError = "journal+console";
      };
    };
  };
}
