# Cómo arranca una máquina FÍSICA con esta configuración.
#
# Va aparte porque el generador de imágenes aporta lo suyo: al construir un
# qcow2 para QEMU, el arranque y las particiones los define el formato, y
# declarar los nuestros aquí chocaría con los suyos.
{ ... }:

{
  boot.loader.grub.device = "/dev/vda";
  fileSystems."/" = {
    device = "/dev/vda1";
    fsType = "ext4";
  };
}
