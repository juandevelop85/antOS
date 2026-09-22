# El módulo que convierte a antOS en parte del sistema.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.antos;
in
{
  # El caché binario (T36.3) va con el módulo base para que la máquina que
  # `antos install` genera (`nixosModules.default`) lo herede sin más.
  imports = [ ./cache.nix ];

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

    user = lib.mkOption {
      type = lib.types.str;
      default = "root";
      description = ''
        Usuario bajo el que corre el demonio (`antos demonio`). El socket IPC
        se crea `0600` a propósito (T31.8), así que este es el único usuario
        que puede hablarle: con el escritorio activado
        (`services.antos.desktop`) tiene que ser el mismo usuario de la sesión
        Wayland — ese módulo lo ajusta solo. `root` es lo correcto para un uso
        headless.
      '';
    };

    smoke.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Gancho de verificación automatizada (T36.2): si QEMU pasa un guion
        por `-fw_cfg name=opt/antos/smoke,file=…`, el servicio
        `antos-smoke` lo ejecuta como root tras el arranque y vuelca su
        salida al journal y a la consola. Sin `fw_cfg` (hardware real, otra
        VM) no hace nada. La ISO en vivo lo trae activado para
        `system/nixos/install-smoke.sh`; una máquina instalada no, salvo
        que el smoke lo pida.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # antOS Linux es monousuario en esta fase (T36.5): el demonio corre como
    # el usuario de la sesión y el socket es `0600`. Una configuración que
    # separe ambos no funcionaría y no debe evaluar en silencio.
    assertions = [{
      assertion = !config.services.antos.desktop.enable
        || cfg.user == config.services.antos.desktop.autologinUser;
      message = ''
        services.antos.user (${cfg.user}) tiene que ser el usuario del escritorio
        (services.antos.desktop.autologinUser = ${config.services.antos.desktop.autologinUser}):
        antOS Linux es monousuario en esta versión; el socket IPC es 0600 y solo
        habla con quien lo abrió. Un demonio por usuario es un ticket futuro.
      '';
    }];

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

    # `workspace` y `state` son propiedad de `cfg.user`: el demonio corre como
    # ese usuario y necesita crear el socket dentro de `state` y escribir
    # instantáneas/bitácora. Con `user = "root"` (headless) esto es idéntico a
    # como estaba.
    systemd.tmpfiles.rules = [
      "d ${cfg.workspace} 0755 ${cfg.user} root -"
      "d ${cfg.state} 0700 ${cfg.user} root -"
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

    # Gancho de smoke por QEMU `fw_cfg` (T36.2). `qemu_fw_cfg` expone el
    # fichero en sysfs; sin él (o sin QEMU) el guion no existe y el servicio
    # termina sin hacer nada. Ejecutar un guion es aquí la funcionalidad
    # pedida (T31.4), y solo entra por un canal que controla quien arranca
    # la VM.
    boot.kernelModules = lib.mkIf cfg.smoke.enable [ "qemu_fw_cfg" ];
    systemd.services.antos-smoke = lib.mkIf cfg.smoke.enable {
      description = "antOS · smoke: ejecuta el guion que QEMU pasa por fw_cfg";
      wantedBy = [ "multi-user.target" ];
      after = [ "antos.service" "systemd-modules-load.service" "greetd.service" ];
      wants = [ "antos.service" ];
      # `nix` va explícito: `nixos-install` lo invoca por nombre y un
      # servicio de systemd no hereda el PATH del sistema — el primer smoke
      # real (2026-09-22) llegó hasta `nixos-install` y murió ahí con
      # «nix: command not found», con el disco ya particionado y montado.
      path = with pkgs; [
        bash coreutils util-linux gnugrep gnused findutils procps systemd
        gnutar gzip xz jq config.nix.package
        parted dosfstools e2fsprogs nixos-install-tools mkpasswd cfg.package
      ];
      environment = config.environment.variables;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        StandardOutput = "journal+console";
        StandardError = "journal+console";
      };
      script = ''
        f=/sys/firmware/qemu_fw_cfg/by_name/opt/antos/smoke/raw
        if [ ! -r "$f" ]; then
          echo "antos-smoke: sin guion en fw_cfg (opt/antos/smoke); nada que hacer"
          exit 0
        fi
        install -m 0700 "$f" /run/antos-smoke.sh
        exec bash /run/antos-smoke.sh
      '';
    };

    # El demonio de intenciones: es quien hace `ipc::serve` y crea el socket
    # (`antos demonio`). Sin este servicio, `services.antos.enable` monta las
    # rutas y el `doctor` pero nada escucha en `${cfg.state}/antos.sock`, y
    # `antos-barra` no tiene con quién hablar (T31.18).
    systemd.services.antos = {
      description = "antOS · demonio de intenciones (IPC)";
      wantedBy = [ "multi-user.target" ];
      after = [ "antos-doctor.service" "systemd-tmpfiles-setup.service" "local-fs.target" ];
      wants = [ "antos-doctor.service" ];
      environment = config.environment.variables;
      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        ExecStart = "${lib.getExe cfg.package} demonio";
        Restart = "on-failure";
        RestartSec = 2;
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}
