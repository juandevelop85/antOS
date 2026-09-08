#!/usr/bin/env python3
"""Headless QEMU boot + input-injection smoke test for antOS (T28.10).

Driven entirely by environment variables so the same harness serves the
x86_64 (`run.sh`) and AArch64 (`system/run-arm.sh`) matrices:

    ANTOS_QEMU_BIN      qemu binary (e.g. qemu-system-x86_64)
    ANTOS_QEMU_ARGS     newline-separated qemu arguments (serial to stdio)
    ANTOS_BANNER        substring that proves the kernel booted
    ANTOS_INJECT_INPUT  "1" to also inject keyboard + mouse events via the
                        QEMU monitor and require the kernel to acknowledge them
    ANTOS_BOOT_TIMEOUT  seconds to wait for the banner (default 20)
    ANTOS_INPUT_TIMEOUT seconds to wait for the input-rx marker (default 15)

Exit code 0 on success, 1 on any failed expectation.
"""

import os
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time

ANSI = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")


def strip_ansi(s: str) -> str:
    return ANSI.sub("", s)


def main() -> int:
    qemu_bin = os.environ["ANTOS_QEMU_BIN"]
    qemu_args = [a for a in os.environ["ANTOS_QEMU_ARGS"].splitlines() if a]
    banner = os.environ.get("ANTOS_BANNER", "antOS")
    inject = os.environ.get("ANTOS_INJECT_INPUT", "0") == "1"
    boot_timeout = float(os.environ.get("ANTOS_BOOT_TIMEOUT", "20"))
    input_timeout = float(os.environ.get("ANTOS_INPUT_TIMEOUT", "15"))

    sock_path = os.path.join(tempfile.mkdtemp(prefix="antos-mon-"), "monitor.sock")
    cmd = [qemu_bin, *qemu_args, "-monitor", f"unix:{sock_path},server,nowait", "-no-reboot"]

    print(">> qemu:", " ".join(cmd), flush=True)
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    log_lines: list[str] = []
    log_lock = threading.Lock()

    def reader() -> None:
        assert proc.stdout is not None
        for line in proc.stdout:
            with log_lock:
                log_lines.append(line)

    t = threading.Thread(target=reader, daemon=True)
    t.start()

    def log_text() -> str:
        with log_lock:
            return strip_ansi("".join(log_lines))

    def wait_for(needle: str, timeout: float) -> bool:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if needle in log_text():
                return True
            if proc.poll() is not None:
                # QEMU exited; give the reader a beat to flush.
                time.sleep(0.2)
                return needle in log_text()
            time.sleep(0.2)
        return False

    ok = True

    if not wait_for(banner, boot_timeout):
        print(f"FALLO: no apareció el banner {banner!r} en {boot_timeout:.0f}s")
        ok = False

    if ok and inject:
        # Give the userspace shell a moment to start polling SYS_READ.
        time.sleep(2.0)
        try:
            mon = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            mon.settimeout(5)
            mon.connect(sock_path)
            time.sleep(0.3)
            for line in (
                "sendkey a",
                "sendkey b",
                "sendkey c",
                "mouse_move 40 25",
                "mouse_move -15 10",
                "mouse_button 1",
                "mouse_button 0",
            ):
                mon.sendall((line + "\n").encode())
                time.sleep(0.25)
            mon.close()
        except OSError as e:
            print(f"FALLO: no se pudo hablar con el monitor QEMU: {e}")
            ok = False

        if ok and not wait_for("input-rx:", input_timeout):
            print(f"FALLO: el kernel no acusó recibo de entrada (sin 'input-rx:') en {input_timeout:.0f}s")
            ok = False

    # Tear down.
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()

    text = log_text()
    if ok:
        print("✓ humo de arranque correcto")
        for ln in text.splitlines():
            if any(m in ln for m in (banner, "input-rx:", "ps2-mouse", "usb-xhci",
                                     "virtio-input", "roothub", "GICv", "timer ",
                                     "ramfb", "virtio-gpu")):
                print("  " + ln)
    else:
        print("---- salida serie (cola) ----")
        print("\n".join(text.splitlines()[-40:]))

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
