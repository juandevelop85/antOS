//! Máquina virtual de ejecución WebAssembly con aislamiento de memoria y límite de combustible (T14.1).

use super::parser::{read_i32_leb128, read_u32_leb128, WasmModule};
use std::collections::HashMap;

pub const PAGE_SIZE: usize = 65536; // 64 KiB
pub const MAX_LINEAR_MEMORY_BYTES: usize = 64 * 1024 * 1024; // Cuota máxima de 64 MB

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    MemoryOutOfBounds { requested: usize, limit: usize },
    FuelExhausted { consumed: u64 },
    StackUnderflow,
    DivisionByZero,
    UnknownOpcode(u8),
    FunctionNotFound(String),
    HostError(String),
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trap::Unreachable => write!(f, "Instrucción inalcanzable (unreachable)"),
            Trap::MemoryOutOfBounds { requested, limit } => {
                write!(f, "Violación de aislamiento de memoria: acceso a offset {} fuera del límite {}", requested, limit)
            }
            Trap::FuelExhausted { consumed } => {
                write!(f, "Cuota de combustible (fuel) agotada tras {} instrucciones (prevención de bucle infinito)", consumed)
            }
            Trap::StackUnderflow => write!(f, "Subdesbordamiento de pila de operandos"),
            Trap::DivisionByZero => write!(f, "División por cero en instrucción aritmética"),
            Trap::UnknownOpcode(op) => write!(f, "Opcode de WebAssembly no implementado: 0x{:02x}", op),
            Trap::FunctionNotFound(name) => write!(f, "Función exportada «{}» no encontrada en el módulo", name),
            Trap::HostError(msg) => write!(f, "Error en importación de host: {}", msg),
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
    pub fn new(module: WasmModule, fuel_limit: u64, params: HashMap<String, String>) -> Result<Self, Trap> {
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
        self.fuel_consumed += amount;
        if self.fuel_consumed > self.fuel_limit {
            return Err(Trap::FuelExhausted {
                consumed: self.fuel_consumed,
            });
        }
        Ok(())
    }

    pub fn read_memory(&self, ptr: usize, len: usize) -> Result<&[u8], Trap> {
        if ptr + len > self.memory.len() {
            return Err(Trap::MemoryOutOfBounds {
                requested: ptr + len,
                limit: self.memory.len(),
            });
        }
        Ok(&self.memory[ptr..ptr + len])
    }

    pub fn write_memory(&mut self, ptr: usize, data: &[u8]) -> Result<(), Trap> {
        if ptr + data.len() > self.memory.len() {
            return Err(Trap::MemoryOutOfBounds {
                requested: ptr + data.len(),
                limit: self.memory.len(),
            });
        }
        self.memory[ptr..ptr + data.len()].copy_from_slice(data);
        Ok(())
    }

    pub fn execute_export(&mut self, name: &str, args: &[i32]) -> Result<Option<i32>, Trap> {
        let export = self.module.exports.get(name).ok_or_else(|| {
            Trap::FunctionNotFound(name.to_string())
        })?;

        if export.kind != 0 {
            return Err(Trap::HostError(format!("El export «{}» no es una función", name)));
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
                return Err(Trap::HostError(format!("Cuerpo de función no encontrado para índice {}", func_idx)));
            }
        };

        let type_idx = self.module.functions[internal_idx] as usize;
        let func_type = &self.module.types[type_idx];

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
            for _ in 0..count {
                locals.push(0);
            }
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
                0x02 => { // block
                    let _block_type = code[pc];
                    pc += 1;
                    control_stack.push(usize::MAX); // Marca de block
                }
                0x03 => { // loop
                    let _block_type = code[pc];
                    pc += 1;
                    control_stack.push(pc); // Marca de loop con start_pc
                }
                0x0b => { // end
                    if control_stack.pop().is_none() {
                        break;
                    }
                }
                0x0c => { // br
                    let (label_idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    if let Some(&frame) = control_stack.iter().rev().nth(label_idx as usize) {
                        if frame != usize::MAX {
                            pc = frame;
                            continue;
                        }
                    }
                    let _ = len;
                    break;
                }
                0x0d => { // br_if
                    let (label_idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
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
                0x0f => { // return
                    break;
                }
                0x10 => { // call
                    let (target_idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;

                    // Extraer argumentos necesarios para la función
                    let type_idx = if (target_idx as usize) < self.module.imports.len() {
                        self.module.imports[target_idx as usize].type_index as usize
                    } else {
                        let f_idx = target_idx as usize - self.module.imports.len();
                        self.module.functions[f_idx] as usize
                    };

                    let func_type = &self.module.types[type_idx];
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
                0x1a => { // drop
                    stack.pop().ok_or(Trap::StackUnderflow)?;
                }
                0x20 => { // local.get
                    let (idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = *locals.get(idx as usize).ok_or(Trap::StackUnderflow)?;
                    stack.push(val);
                }
                0x21 => { // local.set
                    let (idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    if (idx as usize) < locals.len() {
                        locals[idx as usize] = val;
                    } else {
                        return Err(Trap::HostError(format!("Índice de variable local inválido: {}", idx)));
                    }
                }
                0x22 => { // local.tee
                    let (idx, len) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    let val = *stack.last().ok_or(Trap::StackUnderflow)?;
                    if (idx as usize) < locals.len() {
                        locals[idx as usize] = val;
                    }
                }
                0x28 => { // i32.load
                    let (_align, len1) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = base + offset as usize;
                    let bytes = self.read_memory(addr, 4)?;
                    let val = i32::from_le_bytes(bytes.try_into().unwrap());
                    stack.push(val);
                }
                0x36 => { // i32.store
                    let (_align, len1) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = base + offset as usize;
                    self.write_memory(addr, &val.to_le_bytes())?;
                }
                0x3a => { // i32.store8
                    let (_align, len1) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len1;
                    let (offset, len2) = read_u32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len2;

                    let val = (stack.pop().ok_or(Trap::StackUnderflow)? & 0xff) as u8;
                    let base = stack.pop().ok_or(Trap::StackUnderflow)? as usize;
                    let addr = base + offset as usize;
                    self.write_memory(addr, &[val])?;
                }
                0x41 => { // i32.const
                    let (val, len) = read_i32_leb128(&code[pc..])
                        .map_err(|e| Trap::HostError(e.to_string()))?;
                    pc += len;
                    stack.push(val);
                }
                0x45 => { // i32.eqz
                    let val = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if val == 0 { 1 } else { 0 });
                }
                0x46 => { // i32.eq
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a == b { 1 } else { 0 });
                }
                0x47 => { // i32.ne
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a != b { 1 } else { 0 });
                }
                0x48 => { // i32.lt_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a < b { 1 } else { 0 });
                }
                0x4a => { // i32.gt_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a > b { 1 } else { 0 });
                }
                0x4c => { // i32.le_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a <= b { 1 } else { 0 });
                }
                0x4e => { // i32.ge_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(if a >= b { 1 } else { 0 });
                }
                0x6a => { // i32.add
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_add(b));
                }
                0x6b => { // i32.sub
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_sub(b));
                }
                0x6c => { // i32.mul
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a.wrapping_mul(b));
                }
                0x6d => { // i32.div_s
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    if b == 0 {
                        return Err(Trap::DivisionByZero);
                    }
                    stack.push(a.wrapping_div(b));
                }
                0x71 => { // i32.and
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a & b);
                }
                0x72 => { // i32.or
                    let b = stack.pop().ok_or(Trap::StackUnderflow)?;
                    let a = stack.pop().ok_or(Trap::StackUnderflow)?;
                    stack.push(a | b);
                }
                0x73 => { // i32.xor
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
                    return Err(Trap::HostError("Argumentos insuficientes para antos.log".into()));
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
                    return Err(Trap::HostError("Argumentos insuficientes para antos.read_param".into()));
                }
                let key_ptr = args[0] as usize;
                let key_len = args[1] as usize;
                let out_ptr = args[2] as usize;
                let out_max_len = args[3] as usize;

                let key_bytes = self.read_memory(key_ptr, key_len)?;
                let key = String::from_utf8_lossy(key_bytes).to_string();

                let maybe_val = self.host_env.params.get(&key).map(|v| v.as_bytes().to_vec());
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
                    return Err(Trap::HostError("Argumentos insuficientes para antos.write_output".into()));
                }
                let ptr = args[0] as usize;
                let len = args[1] as usize;

                let bytes = self.read_memory(ptr, len)?.to_vec();
                self.host_env.output.extend_from_slice(&bytes);
                Ok(None)
            }
            (m, f) => Err(Trap::HostError(format!("Función importada no soportada: {}.{}", m, f))),
        }
    }
}
