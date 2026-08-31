# Paquetes del sistema, declarados por syso.
#
# Esto NO instala nada: describe qué debe tener la máquina. Aplicarlo es un
# paso aparte, explícito y tuyo:
#
#     sudo nixos-rebuild switch
#
# Editarlo a mano es correcto: syso respeta lo que encuentre aquí.
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
    htop
  ];
}
