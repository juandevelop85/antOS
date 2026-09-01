{
  description = "syso — el sistema hace lo que le pides, y puedes deshacerlo";

  # El flake vive en la raíz porque un flake no puede referenciar rutas por
  # encima de sí mismo, y el paquete necesita el workspace de cargo entero.

  # nixos-unstable porque la rama de versión concreta depende de cuándo leas
  # esto. Para una máquina de verdad, fija una release.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      sistemas = [ "aarch64-linux" "x86_64-linux" ];
      paraCada = f: nixpkgs.lib.genAttrs sistemas (s: f nixpkgs.legacyPackages.${s});

      # Lo común a todas las variantes de la máquina.
      base = [
        ./system/nixos/configuracion.nix
        self.nixosModules.default
        { nixpkgs.overlays = [ (final: prev: { sysod = final.callPackage ./system/nixos/paquete.nix { }; }) ]; }
      ];

      maquina = extra: nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = base ++ extra;
      };
    in
    {
      packages = paraCada (pkgs: {
        default = pkgs.callPackage ./system/nixos/paquete.nix { };
        sysod = pkgs.callPackage ./system/nixos/paquete.nix { };
      });

      nixosModules.default = import ./system/nixos/modulo.nix;

      # La máquina entera, definida como un valor. Esto es lo que hace posible
      # que "deshacer" a nivel de sistema sea volver a la generación anterior
      # en vez de reconstruir a mano lo que había.
      nixosConfigurations.syso = maquina [ ./system/nixos/arranque.nix ];

      # La misma máquina, arrancable en QEMU.
      #
      # `system.build.vm` se construye SIN necesitar una VM, que es lo que
      # importa en Apple Silicon: aquí no hay virtualización anidada, y los
      # generadores de imágenes de disco montan una VM para ensamblarse.
      nixosConfigurations.syso-vm = maquina [ ./system/nixos/vm.nix ];
    };
}
