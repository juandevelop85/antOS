# antOS · Memoria de Proyecto (Claude Code)

Este archivo se carga automáticamente en cada sesión de Claude Code. Reutiliza las
mismas reglas y guía que ya usas con otros LLMs en [`.agents/`](.agents) y
[`AGENTS.md`](AGENTS.md), para no duplicar contenido entre herramientas.

@AGENTS.md
@.agents/rules/antos-development.md

## Skills disponibles en Claude Code

- `antos-ticket-runner` está enlazada en [`.claude/skills/antos-ticket-runner`](.claude/skills/antos-ticket-runner)
  (symlink a [`.agents/skills/antos-ticket-runner`](.agents/skills/antos-ticket-runner), misma fuente que usan los
  otros agentes). Claude Code la detecta automáticamente y la activa por descripción, o puedes invocarla
  explícitamente con `/antos-ticket-runner`.
