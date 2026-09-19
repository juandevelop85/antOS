# La máquina, definida como un valor.
{ modulesPath, ... }:

{
  imports = [
    "${modulesPath}/profiles/qemu-guest.nix"
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

  # La consola en el puerto serie: es lo que permite ver el arranque entero
  # sin ventana gráfica, igual que hacemos con el kernel de la Vía B.
  boot.kernelParams = [ "console=ttyAMA0,115200" ];

  # Sin contraseña y con sesión abierta: es una máquina de desarrollo para
  # verla arrancar, no algo que dejar en una red.
  users.users.root.initialPassword = "antos";
  services.getty.autologinUser = "root";
  networking.hostName = "antos";

  system.stateVersion = "25.05";
}
