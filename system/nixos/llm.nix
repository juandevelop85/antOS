# El motor de modelos locales de la imagen (T34.3).
#
# `services.antos.llm` deja Ollama escuchando en loopback desde el arranque,
# sobre el módulo `services.ollama` de nixpkgs (ninguna dependencia nueva).
# antOS lo encuentra como servicio externo gestionado por systemd
# (`antos services` → `ollama · external`), `antos llm setup` solo tiene
# que descargar un modelo, y `antos service up ollama` no arranca un segundo
# demonio: adopta este.
#
# Dos decisiones deliberadas:
# - Solo `127.0.0.1`. Un Ollama en `0.0.0.0` es un servidor de inferencia sin
#   autenticación abierto a la red. Se puede cambiar, pero avisa.
# - `models = [ ]` por defecto. `loadModels` descarga en la activación de la
#   configuración, y un `nixos-rebuild switch` que baje 4 GB sin preguntar
#   es lo contrario del «explícito y tuyo» de `antos-paquetes.nix`. La
#   descarga la hace el usuario con `antos llm setup` / `antos llm pull`.
#   Quien quiera preinstalar, lo declara.
{ config, lib, pkgs, ... }:

let
  cfg = config.services.antos.llm;
  loopbackHosts = [ "127.0.0.1" "localhost" "::1" ];
  isLoopback = builtins.elem cfg.host loopbackHosts;
in
{
  options.services.antos.llm = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Motor de modelos locales de serie. El escritorio
        (`services.antos.desktop`) lo activa por defecto; una máquina
        headless o la ISO no, por RAM.
      '';
    };

    engine = lib.mkOption {
      type = lib.types.enum [ "ollama" ];
      default = "ollama";
      description = "Motor. Hoy solo Ollama; deja sitio a llama.cpp.";
    };

    package = lib.mkOption {
      type = lib.types.package;
      # nixpkgs retiró `services.ollama.acceleration`: ahora la GPU se elige
      # por paquete. La opción `acceleration` de antOS se conserva y se
      # traduce aquí (primer hallazgo de la evaluación real del flake en la
      # podman-machine, 2026-09-22).
      default =
        if cfg.acceleration == "cuda" then pkgs.ollama-cuda
        else if cfg.acceleration == "rocm" then pkgs.ollama-rocm
        else if cfg.acceleration == false then pkgs.ollama-cpu
        else pkgs.ollama;
      defaultText = lib.literalExpression "pkgs.ollama (o ollama-cuda / ollama-rocm / ollama-cpu según `acceleration`)";
      description = "Paquete de Ollama (de la `nixpkgs` fijada en `flake.lock`).";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = ''
        Dirección de escucha. Cualquier cosa que no sea loopback expone un
        servidor de inferencia sin autenticación: el módulo avisa al evaluar.
      '';
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 11434;
      description = "Puerto de escucha (el que antOS sondea por defecto).";
    };

    acceleration = lib.mkOption {
      type = lib.types.nullOr (lib.types.enum [ false "rocm" "cuda" ]);
      default = null;
      description = ''
        `null` deja el paquete genérico (`pkgs.ollama`), `"cuda"`/`"rocm"`
        eligen `ollama-cuda`/`ollama-rocm`, `false` fuerza `ollama-cpu`.
        (`services.ollama.acceleration` ya no existe en nixpkgs; esto fija
        `package`.)
      '';
    };

    models = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "qwen2.5-coder:7b" ];
      description = ''
        Modelos a descargar en la activación de la configuración
        (`services.ollama.loadModels`). Vacío a propósito: son gigabytes y
        la descarga debe ser una decisión del usuario (`antos llm setup`).
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    warnings = lib.optional (!isLoopback) ''
      services.antos.llm.host = "${cfg.host}": Ollama escuchará fuera de
      loopback SIN autenticación. Cualquiera que alcance ${cfg.host}:${toString cfg.port}
      puede usar (y cargar) los modelos de esta máquina.
    '';

    services.ollama = {
      enable = true;
      package = cfg.package;
      host = cfg.host;
      port = cfg.port;
      loadModels = cfg.models;
      # Los modelos son del sistema, no de una sesión de antOS: el
      # directorio estable que nixpkgs usa. `antos llm doctor` lo muestra
      # junto al de `$STATE/services/ollama/data` cuando ambos existen.
      # (`services.ollama.home` es `/var/lib/ollama`; `models` cuelga de él.)
    };

    # Además del endurecimiento que nixpkgs trae: Ollama no tiene por qué
    # leer los directorios personales.
    systemd.services.ollama.serviceConfig.ProtectHome = lib.mkDefault true;

    # `OLLAMA_HOST` para los clientes de todo el sistema (el CLI de `ollama`
    # y cualquier herramienta que lo lea), coherente con el puerto elegido.
    environment.variables.OLLAMA_HOST = "http://${cfg.host}:${toString cfg.port}";
  };
}
