# Evaluación de agentes (T33.5)

Casos declarativos para el runtime de agente (T33.2) y sus roles (T33.3).
Cada `*.toml` describe un **fixture** (proyecto de ejemplo en `fixtures/`),
un **objetivo**, opcionalmente un **guion** del proveedor `fake` y lo que se
**espera**. Dos usos:

- `antos eval agent` — smoke determinista con el guion `fake`: sin red ni
  clave, verifica el contrato del runtime (orden de herramientas, tests en
  verde, ficheros permitidos, motivo de parada). Corre también en
  `cargo test` (`agent::eval::tests`) y en CI.
- `antos eval agent --live` — los mismos casos contra el proveedor
  configurado (`antos llm use …`), con métricas por caso en
  `.antos/evals/<fecha>.json`; `antos eval diff` compara con la ejecución
  anterior y señala regresiones. Nunca en CI: gasta dinero.

Un caso:

```toml
name = "rust-fix-failing-test"
fixture = "rust-red-test"           # se copia a un workspace temporal
goal = "haz que pase el test sums"
budget_steps = 10
requires = ["cargo"]                # binarios necesarios; si faltan, se omite
live = true                         # false: contrato del runtime, solo con fake
script = '''[ ...turnos del proveedor fake... ]'''

[expect]
tools = ["fs.read", "fs.patch", "test.run", "finalizar"]  # orden exacto (fake)
stop_reason = "finished"
tests_green = true                  # se ejecuta la suite del fixture al final
files_allowed = ["src/lib.rs"]      # cualquier otra escritura es fallo
```

Con `--live` no se exige el orden de herramientas (es del modelo); sí
`tests_green`, `files_allowed` y `stop_reason`.
