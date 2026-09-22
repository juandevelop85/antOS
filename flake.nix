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
        self.nixosModules.llm
        self.nixosModules.machine
        { nixpkgs.overlays = [ overlayAntos ]; }
      ];

      # Lo común a las variantes que SÍ son "la máquina antOS" DE DESARROLLO:
      # las VM. La máquina física es `instaladaPara` (T36.4).
      base = [ ./system/nixos/configuration.nix ./system/nixos/vm-common.nix ] ++ nucleo;

      maquina = extra: maquinaPara "aarch64-linux" extra;
      maquinaPara = system: extra: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = base ++ extra;
      };

      # La máquina instalada de referencia (T36.3): lo que `antos install`
      # deja en el disco (`system/nixos/installed.nix`). Su closure viaja en
      # la ISO para que `nixos-install` no construya nada, y la CI la evalúa
      # y construye por arquitectura.
      instaladaPara = system: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = nucleo ++ [
          ./system/nixos/installed.nix
          # Igual que en el `flake.nix` generado: las fuentes de nixpkgs y de
          # antOS en el registro, y con ello en la closure.
          { nix.registry.nixpkgs.flake = nixpkgs; nix.registry.antos.flake = self; }
        ];
      };

      # Versión que la ISO y la release llevan en el nombre: el tag o el
      # commit corto; «dirty» si el árbol tiene cambios sin confirmar.
      version = self.shortRev or self.dirtyShortRev or "dirty";

      # La ISO parte de `nucleo` (sin `configuration.nix`): el perfil del
      # instalador aporta arranque, particiones y autologin propios. Recibe
      # por `_module.args` lo que la hace autosuficiente (T36.3).
      isoPara = system: nixpkgs.lib.nixosSystem {
        inherit system;
        modules = nucleo ++ [
          ./system/nixos/iso.nix
          {
            _module.args = {
              antosSource = self.outPath;
              nixpkgsFlake = nixpkgs;
              installedSystem = (instaladaPara system).config.system.build.toplevel;
              antosVersion = version;
            };
          }
        ];
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

      # El overlay, exportado para que un flake ajeno (el `/etc/nixos/flake.nix`
      # que escribe `antos install`, T36.1) obtenga `pkgs.antosd` y
      # `pkgs.antos-barra` sin referenciar rutas internas de este árbol —
      # `paquete.nix` se renombró en T31.12 y el instalador siguió apuntando
      # al nombre viejo durante meses sin que nada lo detectara.
      overlays.default = overlayAntos;

      nixosModules.default = import ./system/nixos/module.nix;
      nixosModules.desktop = import ./system/nixos/desktop.nix;
      # El motor de modelos locales (T34.3): `services.antos.llm` sobre
      # `services.ollama` de nixpkgs, solo loopback.
      nixosModules.llm = import ./system/nixos/llm.nix;
      # El perfil de máquina física (T36.4): `services.antos.machine`.
      nixosModules.machine = import ./system/nixos/machine.nix;

      # La máquina entera, definida como un valor. Esto es lo que hace posible
      # que "deshacer" a nivel de sistema sea volver a la generación anterior
      # en vez de reconstruir a mano lo que había.
      nixosConfigurations.antos = maquina [ ./system/nixos/boot.nix ];

      # La misma máquina, arrancable en QEMU.
      nixosConfigurations.antos-vm = maquina [ ./system/nixos/vm.nix ];

      # La máquina física con el escritorio antOS Linux (T30.1 / T36.4): es la
      # instalada de referencia (`installed.nix` + `machine.nix`), sin nada de
      # VM. Hasta T36.4 heredaba `qemu-guest.nix`, la consola serie y el
      # autologin de root de `configuration.nix`.
      nixosConfigurations.antos-desktop = instaladaPara "aarch64-linux";

      # La VM gráfica: arranca directa al escritorio antOS (T30.2).
      # `system/arrancar-vm.sh --grafica` construye y lanza esta.
      nixosConfigurations.antos-desktop-vm = maquina [ ./system/nixos/vm-grafica.nix ];

      # La ISO instalable de antOS Linux (T30.2 / T36.3), por arquitectura.
      # `nix build .#iso` construye la del sistema anfitrión.
      nixosConfigurations.antos-iso = isoPara "aarch64-linux";
      nixosConfigurations.antos-iso-aarch64 = isoPara "aarch64-linux";
      nixosConfigurations.antos-iso-x86_64 = isoPara "x86_64-linux";

      # La máquina instalada de referencia (T36.3), por arquitectura.
      nixosConfigurations.antos-installed-aarch64 = instaladaPara "aarch64-linux";
      nixosConfigurations.antos-installed-x86_64 = instaladaPara "x86_64-linux";
    };
}
