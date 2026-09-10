#!/usr/bin/env python3
"""antOS · T31.12 — heuristic scan for Spanish identifiers in Rust code.

Checks `fn`, `let`, `struct`, `enum`, `trait`, `const`, `static`, and `mod`
declarations for a curated list of Spanish word-stems that essentially never
appear in English identifiers in this codebase. Never looks inside string
literals or comments (`//`, `///`, `/* */`) — Spanish prose there is
intentional (see `.agents/rules/antos-development.md`) and out of scope.

This is a developer-facing heuristic, not a hard CI gate (T31.12, Alcance
punto 5, "evaluar"): a substring match against natural-language word stems
inevitably has false positives on English/Spanish cognates ("config",
"total", "version", "variable", "error"...). Wiring an imperfect version of
this into CI as a blocking check would train people to ignore it. Run it by
hand after adding new public identifiers; treat every hit as "look at this",
not as "this is definitely wrong".

Usage: python3 check-spanish-identifiers.py [path ...]
Exit code 0 if no hits, 1 if any hit (so it CAN be wired into CI later if the
word list is ever tightened enough to trust unattended).
"""
import re
import sys
import pathlib

# Whole Spanish word-stems chosen because they do not occur as substrings of
# common English identifiers used in this codebase. Deliberately excludes
# real cognates (config, version, total, variable, error, info, data...).
SPANISH_WORDS = [
    "ruta", "clave", "archivo", "fichero", "directorio", "tarea", "mensaje",
    "resultado", "entrada", "salida", "nodo", "paquete", "destino", "origen",
    "vacio", "vacío", "valido", "válido", "invalido", "inválido", "correcto",
    "incorrecto", "texto", "linea", "línea", "numero", "número", "cantidad",
    "listar", "obtener", "crear", "eliminar", "borrar", "actualizar",
    "guardar", "cargar", "leer", "escribir", "buscar", "encontrar",
    "verificar", "comprobar", "validar", "calcular", "generar", "construir",
    "iniciar", "detener", "ejecutar", "aplicar", "procesar", "manejar",
    "revisar", "analizar", "extraer", "insertar", "modificar", "convertir",
    "transformar", "traducir", "renombrar", "servir", "intencion",
    "intención", "avanzar", "tarjeta", "usuario", "segundos", "ahora",
    "parsear", "articulo", "artículo", "ancestros", "prueba", "cabecera",
    "encabezado", "etiqueta", "contrasena", "contraseña", "correo",
    "estampa", "instantanea", "instantánea", "respaldo", "concesion",
    "concesión", "recinto", "cerrojo", "pendiente", "escritura", "borrado",
    "razon", "razón", "detalle", "aviso", "anterior", "siguiente",
    "nombre", "sistema", "hilo",
]
SPANISH_RE = re.compile(
    r"\b(" + "|".join(re.escape(w) for w in SPANISH_WORDS) + r")",
    re.IGNORECASE,
)

DECL_RE = re.compile(
    r"^\s*(pub(\([^)]*\))?\s+)?(fn|struct|enum|trait|const|static|mod)\s+(\w+)"
)
LET_RE = re.compile(r"^\s*let\s+(mut\s+)?(\w+)")


def strip_line_comment(line: str) -> str:
    in_str = False
    i = 0
    while i < len(line) - 1:
        c = line[i]
        if c == '"' and (i == 0 or line[i - 1] != "\\"):
            in_str = not in_str
        if not in_str and line[i : i + 2] == "//":
            return line[:i]
        i += 1
    return line


TEST_MOD_RE = re.compile(r"^\s*mod\s+tests\s*\{")


def scan_file(path: pathlib.Path):
    hits = []
    in_block_comment = False
    in_test_mod = False
    test_mod_depth = 0
    for lineno, raw in enumerate(path.read_text(errors="replace").splitlines(), 1):
        line = raw
        # Test-only identifiers are a separate, pervasive, already-established
        # convention in this codebase (Spanish test names throughout) and are
        # explicitly out of scope for T31.12's identifier rule — skip
        # anything inside a `mod tests { ... }` block once entered, tracking
        # brace depth so it un-skips at the matching close.
        if in_test_mod:
            test_mod_depth += line.count("{") - line.count("}")
            if test_mod_depth <= 0:
                in_test_mod = False
            continue
        if TEST_MOD_RE.match(strip_line_comment(line)):
            in_test_mod = True
            test_mod_depth = line.count("{") - line.count("}")
            continue
        if in_block_comment:
            if "*/" in line:
                line = line.split("*/", 1)[1]
                in_block_comment = False
            else:
                continue
        if "/*" in line:
            before, _, after = line.partition("/*")
            line = before
            if "*/" not in after:
                in_block_comment = True
        line = strip_line_comment(line)
        if not line.strip():
            continue

        m = DECL_RE.match(line)
        if m and SPANISH_RE.search(m.group(4)):
            hits.append((lineno, "decl", m.group(4), raw.strip()))
            continue

        m = LET_RE.match(line)
        if m and SPANISH_RE.search(m.group(2)):
            hits.append((lineno, "let", m.group(2), raw.strip()))
    return hits


def main():
    roots = [pathlib.Path(p) for p in (sys.argv[1:] or ["."])]
    total_hits = 0
    for root in roots:
        for path in sorted(root.rglob("*.rs")):
            if "target" in path.parts:
                continue
            hits = scan_file(path)
            if hits:
                total_hits += len(hits)
                print(f"=== {path} ({len(hits)}) ===")
                for lineno, kind, ident, raw in hits:
                    print(f"  {lineno}:{kind}:{ident}: {raw[:100]}")
    print(f"\nTOTAL: {total_hits} hit(s)")
    return 1 if total_hits else 0


if __name__ == "__main__":
    sys.exit(main())
