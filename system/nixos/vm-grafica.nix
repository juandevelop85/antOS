# La VM de antOS con salida GRÁFICA: arranca directa al escritorio antOS
# (Labwc + antos-barra), no a una consola serie.
#
# `vm.nix` sigue siendo la variante headless para CI y arranque por serie;
# esta la reemplaza con `graphics = true`, una GPU `virtio` y el módulo
# `services.antos.desktop` activado. La imagen/ISO instalable es `iso.nix`.
{ lib, modulesPath, ... }:

{
  imports = [ "${modulesPath}/virtualisation/qemu-vm.nix" ];

  services.antos.desktop.enable = true;

  virtualisation = {
    # 3 GiB: GTK4 + compositor + Mesa no caben cómodos en 768 MiB.
    memorySize = 3072;
    cores = 2;
    diskSize = 8192;

    # Ventana gráfica en vez de sólo serie.
    graphics = true;

    # GPU `virtio` con aceleración y entrada USB (tablet = puntero absoluto,
    # sin captura de ratón). `-display` lo fija quien arranca el guion:
    # `arrancar-vm.sh --grafica` usa VNC (arranque desde contenedor);
    # en un host con ventana, sustitúyelo por `gtk,gl=on` o `cocoa`.
    qemu.options = [
      "-device virtio-gpu-pci"
      "-device qemu-xhci"
      "-device usb-tablet"
      "-device usb-kbd"
    ];
  };

  # KMS de `virtio-gpu` en el arranque para que el compositor tenga DRM.
  boot.initrd.availableKernelModules = [ "virtio_gpu" ];
  # La consola serie de `configuration.nix` se conserva; se añade la de vídeo.
  boot.kernelParams = lib.mkAfter [ "console=tty0" ];
}
