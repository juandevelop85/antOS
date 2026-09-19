# Stacks de proyecto (T35.1)

Cada fichero `.toml` de este directorio es un *stack*: lo que antOS sabe de
una tecnología para crear un proyecto nuevo con `project.scaffold`
(`antos "crea un proyecto en nestjs llamado api"`). Los stacks son datos:
añadir uno es escribir un TOML, no recompilar el demonio.

```toml
id        = "nestjs"                  # único; nombre del fichero
language  = "typescript"              # lo que `project.scaffold` llama `language`
framework = "nestjs"                  # vacío en los stacks base de un lenguaje
aliases   = ["nest", "nestjs"]        # palabras que el planificador reconoce
summary   = "API HTTP con NestJS"

[toolchain]
nix      = ["nodejs_22"]              # paquetes de nixpkgs (T35.2 los pone en el flake)
binaries = ["node", "npm"]            # lo que debe existir para install/test/dev

[commands]                            # SIEMPRE vectores; T35.2 los ejecuta sin shell
install = ["npm", "install"]
test    = ["npm", "test", "--silent"]
dev     = ["npm", "run", "start:dev"]
port    = 3000                        # opcional

[network]                             # informativo para el blast (T35.2)
hosts = ["registry.npmjs.org"]

[[file]]                              # `{{name}}` se sustituye por el nombre del proyecto
path    = "src/main.ts"
content = '''…'''
```

Reglas que el cargador impone: `id` y `aliases` sin colisiones, `path`
relativo y sin `..`, cada comando declarado con al menos el programa, y el
programa dentro de la lista blanca de T35.2. Los stacks base (`rust`,
`typescript`, `python`) producen exactamente lo que `project.scaffold`
producía antes de T35.1; los de framework llevan un test que pasa tras
`install`, que es lo que T35.3 verifica al terminar el andamio.

El binario lleva una copia embebida de este directorio: fuera del árbol de
antOS (imagen instalada) se usa esa; dentro, o con `ANTOS_STACKS=<dir>`, se
lee del disco y los cambios se ven sin recompilar.
