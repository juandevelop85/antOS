# La máquina, arrancable dentro de QEMU.
#
# `system.build.vm` de NixOS se construye SIN necesitar una VM: el resultado
# es un guion que lanza QEMU montando el store del anfitrión. Es la diferencia
# que importa aquí, porque en Apple Silicon no hay virtualización anidada y
# los formatos de imagen de disco sí montan una VM para ensamblarse.
{ modulesPath, ... }:

{
  # `nixos-rebuild build-vm` importa esto solo; desde un flake hay que
  # pedirlo a mano. Sin él no existe `system.build.vm` ni sus opciones.
  imports = [ "${modulesPath}/virtualisation/qemu-vm.nix" ];

  virtualisation = {
    memorySize = 768;
    diskSize = 4096;
    # Sin ventana: la consola sale por el puerto serie, igual que hacemos con
    # el kernel de la Vía B.
    graphics = false;
  };
}
