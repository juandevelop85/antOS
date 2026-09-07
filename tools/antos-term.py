#!/usr/bin/env python3
"""
antOS Interactive Terminal Bridge (VirtualBox ARM64)
Connects directly to antOS PL011 UART over TCP (127.0.0.1:2323)
with raw TTY mode, character-by-character echoing, and command execution.
"""
import socket
import sys
import tty
import termios
import select
import time

HOST = '127.0.0.1'
PORT = 2323

def main():
    print(f"\033[1;36mantOS Terminal Bridge\033[0m · Conectando a {HOST}:{PORT}...")
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((HOST, PORT))
    except Exception as e:
        print(f"\033[1;31mError al conectar con antOS: {e}\033[0m")
        print("Verifica que la máquina virtual 'antOS' esté encendida en VirtualBox.")
        return 1

    print("\033[1;32mConectado al shell soberano (PID 1, EL0).\033[0m")
    print("\033[2mEscribe 'help' para ver los comandos. Usa Ctrl+C para salir.\033[0m\n")

    # Send initial newline so antOS immediately echoes 'antos> '
    s.sendall(b"\n")
    time.sleep(0.1)

    if not sys.stdin.isatty():
        # Fallback for piped input
        for line in sys.stdin:
            s.sendall(line.encode('utf-8'))
            time.sleep(0.2)
            resp = s.recv(4096)
            sys.stdout.write(resp.decode('utf-8', errors='replace'))
        s.close()
        return 0

    old_settings = termios.tcgetattr(sys.stdin)
    try:
        tty.setraw(sys.stdin.fileno())
        while True:
            rlist, _, _ = select.select([s, sys.stdin], [], [])
            if s in rlist:
                data = s.recv(1024)
                if not data:
                    break
                # Replace raw \n with \r\n for raw terminal display
                sys.stdout.buffer.write(data.replace(b'\r\n', b'\n').replace(b'\n', b'\r\n'))
                sys.stdout.buffer.flush()
            if sys.stdin in rlist:
                ch = sys.stdin.read(1)
                if ch == '\x03' or ch == '\x04': # Ctrl+C / Ctrl+D
                    break
                s.sendall(ch.encode('utf-8'))
    finally:
        termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_settings)
        s.close()
        print("\r\n\033[1;33mDesconectado de antOS.\033[0m\r\n")

if __name__ == '__main__':
    sys.exit(main() or 0)
