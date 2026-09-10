{
  description = "antOS — el sistema hace lo que le pides, y puedes deshacerlo";

  # El flake vive en la raíz porque un flake no puede referenciar rutas por
  # encima de sí mismo, y el paquete necesita el workspace de cargo entero.

  # nixos-unstable porque la rama de versión concreta depende de cuándo leas
  # esto. Para una máquina de verdad, fija una release.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      sistemas = [ "aarch64-linux" "x86_64-linux" ];
      paraCada = f: nixpkgs.lib.genAttrs sistemas (s: f nixpkgs.legacyPackages.${s});

      # El overlay con los paquetes propios de antOS.
      overlayAntos = final: _prev: {
        antosd = final.callPackage ./system/nixos/package.nix { };
        antos-barra = final.callPackage ./system/nixos/barra.nix { };
      };

      # Los módulos de antOS y su overlay. Sin la configuración de la máquina,
      # para que el instalador (que trae la suya) pueda reutilizarlos.
      nucleo = [
        self.nixosModules.default
        self.nixosModules.desktop
        { nixpkgs.overlays = [ overlayAntos ]; }
      ];

      # Lo común a las variantes que SÍ son "la máquina antOS".
      base = [ ./system/nixos/configuration.nix ] ++ nucleo;

      maquina = extra: maquinaPara "aarch64-linux" extra;
      maquinaPara = system: extra: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = base ++ extra;
      };

      # La ISO parte de `nucleo` (sin `configuration.nix`): el perfil del
      # instalador aporta arranque, particiones y autologin propios.
      isoPara = system: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = nucleo ++ [ ./system/nixos/iso.nix ];
      };
    in
    {
      packages = paraCada (pkgs: {
        default = pkgs.callPackage ./system/nixos/package.nix { };
        antosd = pkgs.callPackage ./system/nixos/package.nix { };
        antos-barra = pkgs.callPackage ./system/nixos/barra.nix { };

        # ISO instalable de antOS Linux (Método 5), por arquitectura.
        iso = (isoPara pkgs.stdenv.hostPlatform.system).config.system.build.isoImage;
      });

      # El entorno para compilar la barra de intención.
      #
      # Va aquí y no en un guion suelto porque `nix shell` solo pone binarios
      # en el PATH: no prepara PKG_CONFIG_PATH, así que glib-2.0 no aparece.
      # Un devShell sí ejecuta los ganchos de las dependencias.
      devShells = paraCada (pkgs: {
        barra = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.pkg-config pkgs.rustc pkgs.cargo ];
          buildInputs = [ pkgs.gtk4 pkgs.gtk4-layer-shell pkgs.wayland ];
        };
      });

      nixosModules.default = import ./system/nixos/module.nix;
      nixosModules.desktop = import ./system/nixos/desktop.nix;

      # La máquina entera, definida como un valor. Esto es lo que hace posible
      # que "deshacer" a nivel de sistema sea volver a la generación anterior
      # en vez de reconstruir a mano lo que había.
      nixosConfigurations.antos = maquina [ ./system/nixos/boot.nix ];

      # La misma máquina, arrancable en QEMU.
      nixosConfigurations.antos-vm = maquina [ ./system/nixos/vm.nix ];

      # La máquina con el escritorio antOS Linux activado (T30.1). Sirve para
      # evaluar el camino `services.antos.desktop.enable = true` en hardware.
      nixosConfigurations.antos-desktop = maquina [
        ./system/nixos/boot.nix
        { services.antos.desktop.enable = true; }
      ];

      # La VM gráfica: arranca directa al escritorio antOS (T30.2).
      # `system/arrancar-vm.sh --grafica` construye y lanza esta.
      nixosConfigurations.antos-desktop-vm = maquina [ ./system/nixos/vm-grafica.nix ];

      # La ISO instalable de antOS Linux (T30.2). `nix build .#iso`.
      nixosConfigurations.antos-iso = isoPara "aarch64-linux";
    };
}
