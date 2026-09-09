//! Máquina virtual de ejecución WebAssembly con aislamiento de memoria y límite de combustible (T14.1).

use super::parser::{read_i32_leb128, read_u32_leb128, WasmModule};
use std::collections::HashMap;

pub const PAGE_SIZE: usize = 65536; // 64 KiB
pub const MAX_LINEAR_MEMORY_BYTES: usize = 64 * 1024 * 1024; // Cuota máxima de 64 MB

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    MemoryOutOfBounds {
        requested: usize,
        limit: usize,
    },
    FuelExhausted {
        consumed: u64,
    },
    StackUnderflow,
    DivisionByZero,
    UnknownOpcode(u8),
    FunctionNotFound(String),
    HostError(String),
    /// Módulo con una estructura interna inconsistente: bytecode que termina a
    /// mitad de una instrucción, o un índice de función/tipo que no
    /// corresponde a ninguna entrada real (T31.9). A diferencia de
    /// `HostError` —reservado a fallos de una importación de host concreta—
    /// esto cubre cualquier operando del propio módulo que el parser dejó
    /// pasar mal formado y que el intérprete no puede ejecutar con
    /// seguridad.
    MalformedModule(String),
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trap::Unreachable => write!(f, "Instrucción inalcanzable (unreachable)"),
            Trap::MemoryOutOfBounds { requested, limit } => {
                write!(
                    f,
                    "Violación de aislamiento de memoria: acceso a offset {} fuera del límite {}",
                    requested, limit
                )
            }
            Trap::FuelExhausted { consumed } => {
                write!(f, "Cuota de combustible (fuel) agotada tras {} instrucciones (prevención de bucle infinito)", consumed)
            }
            Trap::StackUnderflow => write!(f, "Subdesbordamiento de pila de operandos"),
            Trap::DivisionByZero => write!(f, "División por cero en instrucción aritmética"),
            Trap::UnknownOpcode(op) => {
                write!(f, "Opcode de WebAssembly no implementado: 0x{:02x}", op)
            }
            Trap::FunctionNotFound(name) => {
                write!(f, "Función exportada «{}» no encontrada en el módulo", name)
            }
            Trap::HostError(msg) => write!(f, "Error en importación de host: {}", msg),
            Trap::MalformedModule(msg) => write!(f, "Módulo WebAssembly malformado: {}", msg),
        }
    }
}

impl std::error::Error for Trap {}

#[derive(Debug, Clone, Default)]
pub struct HostEnv {
    pub logs: Vec<(String, String)>,
    pub output: Vec<u8>,
    pub params: HashMap<String, String>,
}

pub struct WasmInstance {
    pub module: WasmModule,
    pub memory: Vec<u8>,
    pub fuel_limit: u64,
    pub fuel_consumed: u64,
    pub host_env: HostEnv,
}

impl WasmInstance {
    pub fn new(
        module: WasmModule,
        fuel_limit: u64,
        params: HashMap<String, String>,
    ) -> Result<Self, Trap> {
        let initial_pages = (module.initial_memory_pages as usize).max(1);
        let memory_size = initial_pages * PAGE_SIZE;

        if memory_size > MAX_LINEAR_MEMORY_BYTES {
            return Err(Trap::MemoryOutOfBounds {
                requested: memory_size,
                limit: MAX_LINEAR_MEMORY_BYTES,
            });
        }

        let memory = vec![0u8; memory_size];
        let host_env = HostEnv {
            logs: Vec::new(),
            output: Vec::new(),
            params,
        };

        Ok(WasmInstance {
            module,
            memory,
            fuel_limit,
            fuel_consumed: 0,
            host_env,
        })
    }

    #[inline]
    pub fn consume_fuel(&mut self, amount: u64) -> Result<(), Trap> {
        // T31.9: `+=` desborda en silencio tras suficientes iteraciones y da
        // la vuelta a cero, burlando el propio límite que este contador
        // existe para hacer cumplir. `saturating_add` deja el contador
        // clavado en `u64::MAX` en vez de envolver — el límite siempre se
        // dispara, nunca se elude.
        self.fuel_consumed = self.fuel_consumed.saturating_add(amount);
        if self.fuel_consumed > self.fuel_limit {
            return Err(Trap::FuelExhausted {
                consumed: self.fuel_consumed,
            });
        }
        Ok(())
    }

    pub fn read_memory(&self, ptr: usize, len: usize) -> Result<&[u8], Trap> {
        // T31.9: `ptr + len` desborda en compilación release cuando `ptr`
        // viene del propio plugin (por ejemplo, cerca de `usize::MAX`); la
        // suma da la vuelta a un valor pequeño, la comprobación de límite
        // pasa por error y el corte `self.memory[ptr..ptr+len]` entra en
        // pánico. `checked_add` hace que ese desbordamiento sea en sí mismo
        // una violación de límites, no una que sortear.
        let end = ptr.checked_add(len).ok_or(Trap::MemoryOutOfBounds {
            requested: usize::MAX,
            limit: self.memory.len(),
        })?;
        if end > self.memory.len() {
            return Err(Trap::MemoryOutOfBounds {
                requested: end,
                limit: self.memory.len(),
            });
        }
        Ok(&self.memory[ptr..end])
    }

    pub fn write_memory(&mut self, ptr: usize, data: &[u8]) -> Result<(), Trap> {
        // T31.9: mismo desbordamiento que en `read_memory`, con la misma
        // corrección.
        let end = ptr.checked_add(data.len()).ok_or(Trap::MemoryOutOfBounds {
            requested: usize::MAX,
            limit: self.memory.len(),
        })?;
        if end > self.memory.len() {
            return Err(Trap::MemoryOutOfBounds {
                requested: end,
                limit: self.memory.len(),
            });
        }
        self.memory[ptr..end].copy_from_slice(data);
        Ok(())
    }

    /// Combina una dirección base (de la pila, por tanto potencialmente
    /// cualquier `usize` tras el `as` de un `i32` negativo) con el
    /// desplazamiento inmediato de una instrucción `i32.load`/`i32.store*`
    /// sin poder desbordar en silencio (T31.9). Sin esto, `base + offset`
    /// puede dar la vuelta a una dirección pequeña que sí cae dentro de la
    /// memoria: no un pánico, sino un acceso a un desplazamiento distinto
    /// del que el plugin pidió.
    fn checked_address(&self, base: usize, offset: usize) -> Result<usize, Trap> {
        base.checked_add(offset).ok_or(Trap::MemoryOutOfBounds {
            requested: usize::MAX,
            limit: self.memory.len(),
        })
    }

    pub fn execute_export(&mut self, name: &str, args: &[i32]) -> Result<Option<i32>, Trap> {
        let export = self
            .module
            .exports
            .get(name)
            .ok_or_else(|| Trap::FunctionNotFound(name.to_string()))?;

        if export.kind != 0 {
            return Err(Trap::HostError(format!(
                "El export «{}» no es una función",
                name
            )));
        }

        let func_idx = export.index as usize;
        self.call_function(func_idx, args)
    }

    fn call_function(&mut self, func_idx: usize, args: &[i32]) -> Result<Option<i32>, Trap> {
        let num_imports = self.module.imports.len();
        if func_idx < num_imports {
            // Llamada a host import
            return self.call_host_import(func_idx, args);
        }

        let internal_idx = func_idx - num_imports;
        let (local_decls, code) = match self.module.bodies.get(internal_idx) {
            Some(b) => (b.locals.clone(), b.code.clone()),
            None => {
                return Err(Trap::HostError(format!(
                    "Cuerpo de función no encontrado para índice {}",
                    func_idx
                )));
            }
        };

        // T31.9: `internal_idx` ya se validó contra `bodies` (arriba), no
        // contra `functions` — un módulo malformado con secciones de
        // longitudes distintas (más cuerpos de código que declaraciones de
        // función) hacía panicar aquí con un índice fuera de rango.
        // `type_idx`, a su vez, viene sin validar del propio bytecode del
        // módulo y puede no corresponder a ningún tipo real.
        let type_idx = *self.module.functions.get(internal_idx).ok_or_else(|| {
            Trap::MalformedModule(format!(
                "índice de función {} sin declaración de tipo correspondiente (secciones de función/código desalineadas)",
                func_idx
            ))
        })? as usize;
        let func_type = self.module.types.get(type_idx).ok_or_else(|| {
            Trap::MalformedModule(format!("índice de tipo {} fuera de rango", type_idx))
        })?;

        // Configuración de variables locales: primero argumentos, luego variables locales
        let mut locals = Vec::with_capacity(func_type.params.len() + 16);
        for i in 0..func_type.params.len() {
            if i < args.len() {
                locals.push(args[i]);
            } else {
                locals.push(0);
            }
        }
        for &(count, _) in &local_decls {
            locals.extend(std::iter::repeat_n(0, count as usize));
        }

        let mut stack: Vec<i32> = Vec::with_capacity(64);
        let mut pc = 0;
        let mut control_stack: Vec<usize> = Vec::new(); // guarda start_pc para loops

        while pc < code.len() {
            self.consume_fuel(1)?;

            let op = code[pc];
            pc += 1;

            match op {
                0x00 => return Err(Trap::Unreachable),
                0x01 => {} // nop
                0x02 => {
                    // block
                    // T31.9: indexar `code[pc]` directamente entra en pánico
                    // si el bytecode termina justo tras el opcode `block`
                    // (un cuerpo de función truncado/malformado); `.get(pc)`
                    // lo convierte en un `Trap`, no en un cuelgue del host.
                    let _block_type = *code.get(pc).ok_or_else(|| {
                        Trap::MalformedModule("bytecode truncado tras opcode `block` (falta el byte de tipo de bloque)".into())
                    })?;
                    pc += 1;
                    control_stack.push(usize::MAX); // Marca de block
                }
                0x03 => {
                    // loop
                    let _block_type = *code.get(pc).ok_or_else(|| {
                        Trap::MalformedModule("bytecode truncado tras opcode `loop` (falta el byte de tipo de bloque)".into())
                    })?;
                    pc += 1;
                    control_stack.push(pc); // Marca de loop con start_pc
                }
                0x0b => {
                    // end
                    if control_stack.pop().is_none() {
                        break;
                    }
                }
                0x0c => {
                    // br
                    let (label_idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    if let Some(&frame) = control_stack.iter().rev().nth(label_idx as usize) {
                        if frame != usize::MAX {
                            pc = frame;
                            continue;
                        }
                    }
                    let _ = len;
                    break;
                }
                0x0d => {
                    // br_if
                    let (label_idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let cond = stack.pop().ok_or(Trap::StackUnderflow)?;
                    if cond != 0 {
                        if let Some(&frame) = control_stack.iter().rev().nth(label_idx as usize) {
                            if frame != usize::MAX {
                                pc = frame;
                                continue;
                            }
                        }
                        break;
                    }
                }
                0x0f => {
                    // return
                    break;
                }
                0x10 => {
                    // call
                    let (target_idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;

                    // Extraer argumentos necesarios para la función. T31.9:
                    // `target_idx` es un operando de bytecode sin validar —
                    // una instrucción `call` a un índice inventado (mayor
                    // que imports + funciones reales) hacía panicar el
                    // indexado directo en `imports`/`functions`/`types`.
                    let type_idx = if (target_idx as usize) < self.module.imports.len() {
                        self.module.imports[target_idx as usize].type_index as usize
                    } else {
                        let f_idx = target_idx as usize - self.module.imports.len();
                        *self.module.functions.get(f_idx).ok_or_else(|| {
                            Trap::MalformedModule(format!(
                                "llamada a índice de función inexistente: {}",
                                target_idx
                            ))
                        })? as usize
                    };

                    let func_type = self.module.types.get(type_idx).ok_or_else(|| {
                        Trap::MalformedModule(format!("índice de tipo {} fuera de rango", type_idx))
                    })?;
                    let mut call_args = Vec::with_capacity(func_type.params.len());
                    for _ in 0..func_type.params.len() {
                        call_args.push(stack.pop().ok_or(Trap::StackUnderflow)?);
                    }
                    call_args.reverse();

                    let ret = self.call_function(target_idx as usize, &call_args)?;
                    if let Some(val) = ret {
                        stack.push(val);
                    }
                }
                0x1a => {
                    // drop
                    stack.pop().ok_or(Trap::StackUnderflow)?;
                }
                0x20 => {
                    // local.get
                    let (idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = *locals.get(idx as usize).ok_or(Trap::StackUnderflow)?;
                    stack.push(val);
                }
                0x21 => {
                    // local.set
                    let (idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    if (idx as usize) < locals.len() {
                        locals[idx as usize] = val;
                    } else {
                        return Err(Trap::HostError(format!(
                            "Índice de variable local inválido: {}",
                            idx
                        )));
                    }
                }
                0x22 => {
                    // local.tee
                    let (idx, len) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = *stack.last().ok_or(Trap::StackUnderflow)?;
                    if (idx as usize) < locals.len() {
                        locals[idx as usize] = val;
                    }
                }
                0x28 => {
                    // i32.load
                    let (_align, len1) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = self.checked_address(base, offset as usize)?;
                    let bytes = self.read_memory(addr, 4)?;
                    let word: [u8; 4] = bytes.try_into().map_err(|_| {
                        Trap::HostError("i32.load: memory slice was not 4 bytes".into())
                    })?;
                    let val = i32::from_le_bytes(word);
                    stack.push(val);
                }
                0x36 => {
                    // i32.store
                    let (_align, len1) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = self.checked_address(base, offset as usize)?;
                    self.write_memory(addr, &val.to_le_bytes())?;
                }
                0x3a => {
                    // i32.store8
                    let (_align, len1) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) =
                        read_u32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let val = (stack.pop().ok_or(Trap::StackUnderflow)? & 0xff) as u8;
                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = self.checked_address(base, offset as usize)?;
                    self.write_memory(addr, &[val])?;
                }
                0x41 => {
                    // i32.const
                    let (val, len) =
                        read_i32_leb128(&code[pc..]).map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    stack.push(val);
                }
                0x45 => {
                    // i32.eqz
                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if val == 0 { 1 } else { 0 });
                }
                0x46 => {
                    // i32.eq
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a == b { 1 } else { 0 });
                }
                0x47 => {
                    // i32.ne
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a != b { 1 } else { 0 });
                }
                0x48 => {
                    // i32.lt_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a < b { 1 } else { 0 });
                }
                0x4a => {
                    // i32.gt_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a > b { 1 } else { 0 });
                }
                0x4c => {
                    // i32.le_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a <= b { 1 } else { 0 });
                }
                0x4e => {
                    // i32.ge_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a >= b { 1 } else { 0 });
                }
                0x6a => {
                    // i32.add
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_add(b));
                }
                0x6b => {
                    // i32.sub
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_sub(b));
                }
                0x6c => {
                    // i32.mul
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_mul(b));
                }
                0x6d => {
                    // i32.div_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    if b == 0 {
                        return Err(Trap::DivisionByZero);
                    }
                    stack.push(a.wrapping_div(b));
                }
                0x71 => {
                    // i32.and
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a & b);
                }
                0x72 => {
                    // i32.or
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a | b);
                }
                0x73 => {
                    // i32.xor
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a ^ b);
                }
                other => return Err(Trap::UnknownOpcode(other)),
            }
        }

        Ok(stack.pop())
    }

    fn call_host_import(&mut self, import_idx: usize, args: &[i32]) -> Result<Option<i32>, Trap> {
        let import = &self.module.imports[import_idx];

        match (import.module.as_str(), import.field.as_str()) {
            ("antos", "log") => {
                // antos.log(level: i32, ptr: i32, len: i32)
                if args.len() < 3 {
                    return Err(Trap::HostError(
                        "Argumentos insuficientes para antos.log".into(),
                    ));
                }
                let level_num = args[0];
                let ptr = args[1] as usize;
                let len = args[2] as usize;

                let bytes = self.read_memory(ptr, len)?;
                let msg = String::from_utf8_lossy(bytes).to_string();
                let level = match level_num {
                    1 => "ERROR",
                    2 => "WARN",
                    _ => "INFO",
                };
                self.host_env.logs.push((level.to_string(), msg));
                Ok(None)
            }
            ("antos", "read_param") => {
                // antos.read_param(key_ptr: i32, key_len: i32, out_ptr: i32, out_max_len: i32) -> bytes_written
                if args.len() < 4 {
                    return Err(Trap::HostError(
                        "Argumentos insuficientes para antos.read_param".into(),
                    ));
                }
                let key_ptr = args[0] as usize;
                let key_len = args[1] as usize;
                let out_ptr = args[2] as usize;
                let out_max_len = args[3] as usize;

                let key_bytes = self.read_memory(key_ptr, key_len)?;
                let key = String::from_utf8_lossy(key_bytes).to_string();

                let maybe_val = self
                    .host_env
                    .params
                    .get(&key)
                    .map(|v| v.as_bytes().to_vec());
                if let Some(val_bytes) = maybe_val {
                    let to_write = val_bytes.len().min(out_max_len);
                    self.write_memory(out_ptr, &val_bytes[..to_write])?;
                    Ok(Some(to_write as i32))
                } else {
                    Ok(Some(-1))
                }
            }
            ("antos", "write_output") => {
                // antos.write_output(ptr: i32, len: i32)
                if args.len() < 2 {
                    return Err(Trap::HostError(
                        "Argumentos insuficientes para antos.write_output".into(),
                    ));
                }
                let ptr = args[0] as usize;
                let len = args[1] as usize;

                let bytes = self.read_memory(ptr, len)?.to_vec();
                self.host_env.output.extend_from_slice(&bytes);
                Ok(None)
            }
            (m, f) => Err(Trap::HostError(format!(
                "Función importada no soportada: {}.{}",
                m, f
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::wasm::parser::{FuncBody, FuncType};
    use std::collections::HashMap;

    /// Construye un `WasmModule` mínimo directamente en memoria, sin pasar
    /// por `WasmModule::from_bytes` — estos tests apuntan a la aritmética y
    /// a los índices del *intérprete* (`vm.rs`), no al decodificador binario
    /// (`parser.rs`, fuera del alcance de T31.9), así que construir la
    /// estructura a mano permite fabricar desalineaciones entre secciones
    /// (más cuerpos de código que funciones declaradas, tipos inexistentes)
    /// que un binario bien formado nunca produciría pero que un módulo
    /// corrupto u hostil sí podría.
    fn minimal_module(
        types: Vec<FuncType>,
        functions: Vec<u32>,
        bodies: Vec<FuncBody>,
    ) -> WasmModule {
        WasmModule {
            types,
            imports: Vec::new(),
            functions,
            exports: HashMap::new(),
            initial_memory_pages: 1,
            max_memory_pages: 1024,
            bodies,
        }
    }

    #[test]
    fn test_read_memory_near_usize_max_traps_instead_of_overflowing() {
        // Criterio de aceptación literal: `read_memory(usize::MAX, 16)`
        // compilado en release obtiene `Trap::MemoryOutOfBounds`, no un
        // pánico por desbordamiento de `ptr + len`.
        let module = minimal_module(vec![], vec![], vec![]);
        let instance = WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");
        let limit = instance.memory.len();

        match instance.read_memory(usize::MAX, 16) {
            Err(Trap::MemoryOutOfBounds {
                requested,
                limit: got_limit,
            }) => {
                assert_eq!(requested, usize::MAX);
                assert_eq!(got_limit, limit);
            }
            other => panic!("Esperado Trap::MemoryOutOfBounds, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_write_memory_near_usize_max_traps_instead_of_overflowing() {
        // Criterio de aceptación literal: `write_memory(usize::MAX, &[0; 16])`
        // obtiene `Trap::MemoryOutOfBounds`, no un pánico.
        let module = minimal_module(vec![], vec![], vec![]);
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");
        let limit = instance.memory.len();

        match instance.write_memory(usize::MAX, &[0; 16]) {
            Err(Trap::MemoryOutOfBounds {
                requested,
                limit: got_limit,
            }) => {
                assert_eq!(requested, usize::MAX);
                assert_eq!(got_limit, limit);
            }
            other => panic!("Esperado Trap::MemoryOutOfBounds, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_consume_fuel_saturates_instead_of_wrapping_around_to_zero() {
        // Criterio de aceptación literal: consumir combustible hasta
        // desbordar el acumulador obtiene `Trap::FuelExhausted`. La
        // propiedad que importa no es solo "el primer disparo funciona" —
        // es que el acumulador se queda clavado en `u64::MAX` en vez de dar
        // la vuelta a un número pequeño que reabriría la cuota.
        let module = minimal_module(vec![], vec![], vec![]);
        let mut instance = WasmInstance::new(module, u64::MAX, HashMap::new()).expect("instance");
        instance.fuel_consumed = u64::MAX - 5;

        assert!(instance.consume_fuel(10).is_ok());
        assert_eq!(
            instance.fuel_consumed,
            u64::MAX,
            "el acumulador debe saturarse, no envolverse"
        );

        // Con un límite alcanzable, el acumulador saturado sigue disparando
        // el `Trap` en vez de que el wraparound lo haya burlado.
        instance.fuel_limit = 100;
        match instance.consume_fuel(1) {
            Err(Trap::FuelExhausted { consumed }) => assert_eq!(consumed, u64::MAX),
            other => panic!("Esperado Trap::FuelExhausted, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_div_s_by_zero_traps_without_panicking() {
        // Criterio de aceptación literal: un módulo con `i32.div_s` de
        // divisor cero produce un `Trap`, no un pánico.
        let module = minimal_module(
            vec![FuncType {
                params: vec![],
                returns: vec![],
            }],
            vec![0],
            vec![FuncBody {
                locals: vec![],
                // i32.const 5; i32.const 0; i32.div_s; end
                code: vec![0x41, 0x05, 0x41, 0x00, 0x6d, 0x0b],
            }],
        );
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");
        assert_eq!(instance.call_function(0, &[]), Err(Trap::DivisionByZero));
    }

    #[test]
    fn test_malformed_bytecode_corpus_never_panics_only_traps() {
        // Criterio de aceptación literal: el corpus de módulos malformados
        // se ejecuta en `cargo test` sin ningún pánico. Cada entrada es
        // bytecode que, antes de T31.9, hacía panicar al intérprete
        // (indexado directo sin comprobar límites) o producía un resultado
        // silenciosamente incorrecto (suma que desborda y da la vuelta a
        // una dirección válida).
        let type0 = FuncType {
            params: vec![],
            returns: vec![],
        };
        let malformed_bodies: Vec<Vec<u8>> = vec![
            vec![0x02], // block truncado: falta el byte de tipo de bloque
            vec![0x03], // loop truncado: falta el byte de tipo de bloque
            vec![0x10, 0xff, 0xff, 0xff, 0xff, 0x0f], // call a un índice de función astronómico
            vec![0x20], // local.get truncado: falta el índice LEB128
            vec![0x28, 0x02], // i32.load truncado: falta el offset LEB128
            vec![0x41, 0x7f, 0x28, 0x02, 0x08, 0x0b], // i32.const -1; i32.load offset=8: base+offset desbordaría sin checked_add
        ];

        for code in malformed_bodies {
            let module = minimal_module(
                vec![type0.clone()],
                vec![0],
                vec![FuncBody {
                    locals: vec![],
                    code: code.clone(),
                }],
            );
            let mut instance =
                WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                instance.call_function(0, &[])
            }));
            assert!(
                outcome.is_ok(),
                "call_function entró en pánico con bytecode malformado: {:?}",
                code
            );
            assert!(
                outcome.expect("verificado arriba").is_err(),
                "se esperaba un Trap (no un resultado exitoso) para bytecode malformado: {:?}",
                code
            );
        }
    }

    #[test]
    fn test_function_body_without_matching_function_declaration_traps() {
        // Sección de código con más cuerpos que la sección de función
        // declara — una desalineación que un módulo corrupto u hostil
        // puede producir y que el parser no descarta. Antes de T31.9,
        // `self.module.functions[internal_idx]` panicaba con el índice
        // fuera de rango.
        let module = minimal_module(
            vec![FuncType {
                params: vec![],
                returns: vec![],
            }],
            vec![0], // solo declara 1 función...
            vec![
                FuncBody {
                    locals: vec![],
                    code: vec![0x0b],
                },
                FuncBody {
                    locals: vec![],
                    code: vec![0x0b],
                }, // ...pero hay 2 cuerpos de código
            ],
        );
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");
        match instance.call_function(1, &[]) {
            Err(Trap::MalformedModule(_)) => {}
            other => panic!("Esperado Trap::MalformedModule, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_function_type_index_out_of_range_traps() {
        // La declaración de función apunta a un índice de tipo (5) que no
        // existe: `types` está vacío. Antes de T31.9,
        // `self.module.types[type_idx]` panicaba.
        let module = minimal_module(
            vec![],
            vec![5],
            vec![FuncBody {
                locals: vec![],
                code: vec![0x0b],
            }],
        );
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new()).expect("instance");
        match instance.call_function(0, &[]) {
            Err(Trap::MalformedModule(_)) => {}
            other => panic!("Esperado Trap::MalformedModule, obtenido: {:?}", other),
        }
    }
}
