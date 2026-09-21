# La máquina, definida como un valor: lo común a TODAS las variantes que
# son «la máquina antOS» de desarrollo (headless, VM gráfica). Lo que es
# propio de una VM (perfil de invitado QEMU, consola serie, root con sesión
# abierta) está en `vm-common.nix`; lo que es propio de una máquina física
# (red, audio, teclado, energía) en `machine.nix`, que activa `installed.nix`.
{ ... }:

{
  imports = [
    # Aquí está el remate de todo el diseño: lo que antOS declara ES parte de
    # la definición del sistema. No hay un paso intermedio en el que alguien
    # traduzca la intención a comandos — la intención se convierte en una
    # línea de este fichero, la apruebas viendo el diff, y aplicarla es
    # cambiar de generación.
    ./antos-paquetes.nix
  ];

  services.antos.enable = true;

  # El escritorio antOS Linux (Wayland + antos-barra + Neovim/Git). Descomenta
  # para arrancar directo a la sesión gráfica. La imagen gráfica de VM/ISO es
  # la Fase 30 (T30.2); el módulo y sus opciones están en `desktop.nix`.
  # services.antos.desktop.enable = true;

  # El motor de modelos locales (T34.3): Ollama en 127.0.0.1:11434 desde el
  # arranque. El escritorio lo activa solo; en una máquina headless se pide
  # aquí (los modelos se descargan después con `antos llm setup`).
  # services.antos.llm.enable = true;

  networking.hostName = "antos";

  system.stateVersion = "25.05";
}
