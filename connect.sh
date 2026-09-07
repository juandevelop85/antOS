#!/bin/bash
# Conexión directa e interactiva al shell soberano de antOS en VirtualBox
exec python3 "$(dirname "$0")/tools/antos-term.py"
