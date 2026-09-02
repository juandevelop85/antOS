//! WebAssembly binary decoding and module representation (T14.1).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d]; // \0asm
pub const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

impl ValType {
    pub fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            0x7f => Ok(ValType::I32),
            0x7e => Ok(ValType::I64),
            0x7d => Ok(ValType::F32),
            0x7c => Ok(ValType::F64),
            _ => bail!("Tipo de valor WASM no soportado: 0x{:02x}", byte),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub returns: Vec<ValType>,
}

#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub module: String,
    pub field: String,
    pub type_index: u32,
}

#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub name: String,
    pub kind: u8, // 0 = func, 1 = table, 2 = memory, 3 = global
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct FuncBody {
    pub locals: Vec<(u32, ValType)>,
    pub code: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct WasmModule {
    pub types: Vec<FuncType>,
    pub imports: Vec<ImportEntry>,
    pub functions: Vec<u32>, // indices to types
    pub exports: HashMap<String, ExportEntry>,
    pub initial_memory_pages: u32,
    pub max_memory_pages: u32,
    pub bodies: Vec<FuncBody>,
}

impl WasmModule {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 {
            bail!("Binario WASM demasiado corto (longitud: {} bytes)", bytes.len());
        }

        if &bytes[0..4] != WASM_MAGIC {
            bail!("Número mágico de WebAssembly inválido");
        }
        if &bytes[4..8] != WASM_VERSION {
            bail!("Versión de WebAssembly no soportada (esperada v1)");
        }

        let mut offset = 8;
        let mut types = Vec::new();
        let mut imports = Vec::new();
        let mut functions = Vec::new();
        let mut exports = HashMap::new();
        let mut initial_memory_pages = 1;
        let mut max_memory_pages = 1024; // 64 MB (64KB * 1024)
        let mut bodies = Vec::new();

        while offset < bytes.len() {
            let section_id = bytes[offset];
            offset += 1;
            let (section_len, leb_len) = read_u32_leb128(&bytes[offset..])
                .context("Error leyendo longitud de sección WASM")?;
            offset += leb_len;

            let section_data = &bytes[offset..offset + section_len as usize];
            offset += section_len as usize;

            match section_id {
                1 => {
                    // Type section
                    types = parse_type_section(section_data)?;
                }
                2 => {
                    // Import section
                    imports = parse_import_section(section_data)?;
                }
                3 => {
                    // Function section
                    functions = parse_function_section(section_data)?;
                }
                5 => {
                    // Memory section
                    let (init, max) = parse_memory_section(section_data)?;
                    initial_memory_pages = init;
                    max_memory_pages = max;
                }
                7 => {
                    // Export section
                    exports = parse_export_section(section_data)?;
                }
                10 => {
                    // Code section
                    bodies = parse_code_section(section_data)?;
                }
                _ => {
                    // Secciones opcionales o datos personalizados
                }
            }
        }

        Ok(WasmModule {
            types,
            imports,
            functions,
            exports,
            initial_memory_pages,
            max_memory_pages,
            bodies,
        })
    }
}

pub fn read_u32_leb128(bytes: &[u8]) -> Result<(u32, usize)> {
    let mut result = 0u32;
    let mut shift = 0;
    let mut count = 0;

    for &byte in bytes {
        count += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        if (byte & 0x80) == 0 {
            return Ok((result, count));
        }
        shift += 7;
        if shift >= 35 {
            bail!("Entero LEB128 desbordado (> 32 bits)");
        }
    }
    bail!("Secuencia LEB128 truncada");
}

pub fn read_i32_leb128(bytes: &[u8]) -> Result<(i32, usize)> {
    let mut result = 0i32;
    let mut shift = 0;
    let mut count = 0;
    let mut byte = 0u8;

    for &b in bytes {
        byte = b;
        count += 1;
        result |= ((byte & 0x7f) as i32) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            break;
        }
        if shift >= 35 {
            bail!("Entero signado LEB128 desbordado (> 32 bits)");
        }
    }

    if shift < 32 && (byte & 0x40) != 0 {
        result |= !0 << shift;
    }

    Ok((result, count))
}

fn parse_type_section(bytes: &[u8]) -> Result<Vec<FuncType>> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let mut types = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if bytes[offset] != 0x60 {
            bail!("Encabezado de función inválido en sección de tipos (esperado 0x60)");
        }
        offset += 1;

        let (param_count, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let mut params = Vec::with_capacity(param_count as usize);
        for _ in 0..param_count {
            params.push(ValType::from_byte(bytes[offset])?);
            offset += 1;
        }

        let (ret_count, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let mut returns = Vec::with_capacity(ret_count as usize);
        for _ in 0..ret_count {
            returns.push(ValType::from_byte(bytes[offset])?);
            offset += 1;
        }

        types.push(FuncType { params, returns });
    }
    Ok(types)
}

fn parse_import_section(bytes: &[u8]) -> Result<Vec<ImportEntry>> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let mut imports = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (mod_len, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let module = String::from_utf8(bytes[offset..offset + mod_len as usize].to_vec())
            .context("Nombre de módulo de importación inválido UTF-8")?;
        offset += mod_len as usize;

        let (field_len, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let field = String::from_utf8(bytes[offset..offset + field_len as usize].to_vec())
            .context("Nombre de campo de importación inválido UTF-8")?;
        offset += field_len as usize;

        let kind = bytes[offset];
        offset += 1;
        if kind != 0 {
            bail!("Solo se soportan importaciones de funciones (kind 0)");
        }

        let (type_index, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;

        imports.push(ImportEntry {
            module,
            field,
            type_index,
        });
    }
    Ok(imports)
}

fn parse_function_section(bytes: &[u8]) -> Result<Vec<u32>> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let mut functions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (type_idx, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        functions.push(type_idx);
    }
    Ok(functions)
}

fn parse_memory_section(bytes: &[u8]) -> Result<(u32, u32)> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    if count == 0 {
        return Ok((1, 1024));
    }

    let flags = bytes[offset];
    offset += 1;

    let (initial, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let max = if flags & 0x01 != 0 {
        let (m, _) = read_u32_leb128(&bytes[offset..])?;
        m
    } else {
        1024
    };

    Ok((initial, max))
}

fn parse_export_section(bytes: &[u8]) -> Result<HashMap<String, ExportEntry>> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let mut exports = HashMap::with_capacity(count as usize);
    for _ in 0..count {
        let (name_len, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let name = String::from_utf8(bytes[offset..offset + name_len as usize].to_vec())
            .context("Nombre de exportación inválido UTF-8")?;
        offset += name_len as usize;

        let kind = bytes[offset];
        offset += 1;

        let (index, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;

        exports.insert(name.clone(), ExportEntry { name, kind, index });
    }
    Ok(exports)
}

fn parse_code_section(bytes: &[u8]) -> Result<Vec<FuncBody>> {
    let mut offset = 0;
    let (count, len) = read_u32_leb128(&bytes[offset..])?;
    offset += len;

    let mut bodies = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (body_size, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;
        let end_body = offset + body_size as usize;

        let (local_decl_count, len) = read_u32_leb128(&bytes[offset..])?;
        offset += len;

        let mut locals = Vec::with_capacity(local_decl_count as usize);
        for _ in 0..local_decl_count {
            let (num, len) = read_u32_leb128(&bytes[offset..])?;
            offset += len;
            let val_type = ValType::from_byte(bytes[offset])?;
            offset += 1;
            locals.push((num, val_type));
        }

        let code = bytes[offset..end_body].to_vec();
        offset = end_body;

        bodies.push(FuncBody { locals, code });
    }
    Ok(bodies)
}
