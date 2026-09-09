//! Motor de plugins y aislamiento WebAssembly (WASM / WASI) para antOS (T14.1).

pub mod parser;
pub mod vm;
pub mod plugin;

pub use parser::WasmModule;
pub use vm::{Trap, WasmInstance};
pub use plugin::{PluginManager, PluginManifest, DEFAULT_FUEL_LIMIT};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::collections::{BTreeMap, HashMap};

    /// Genera un módulo WebAssembly mínimo válido que exporta una función `add(a: i32, b: i32) -> i32`
    fn build_wasm_add_module() -> Vec<u8> {
        let mut wasm = Vec::new();
        // Magic + Version
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

        // Section 1: Type (1 tipo: (i32, i32) -> i32)
        // type: 0x60, 2 params [i32, i32], 1 return [i32]
        let type_sec = vec![
            0x01, // 1 type
            0x60, // func
            0x02, 0x7f, 0x7f, // params: 2 x i32
            0x01, 0x7f, // returns: 1 x i32
        ];
        wasm.push(0x01); // Section ID 1
        wasm.push(type_sec.len() as u8);
        wasm.extend_from_slice(&type_sec);

        // Section 3: Function (1 función de tipo 0)
        let func_sec = vec![0x01, 0x00];
        wasm.push(0x03); // Section ID 3
        wasm.push(func_sec.len() as u8);
        wasm.extend_from_slice(&func_sec);

        // Section 5: Memory (1 página inicial)
        let mem_sec = vec![0x01, 0x00, 0x01]; // 1 memory, flags=0, min=1
        wasm.push(0x05); // Section ID 5
        wasm.push(mem_sec.len() as u8);
        wasm.extend_from_slice(&mem_sec);

        // Section 7: Export (exporta func 0 con nombre "add")
        let export_sec = vec![
            0x01, // 1 export
            0x03, b'a', b'd', b'd', // name "add"
            0x00, // kind 0 = func
            0x00, // func index 0
        ];
        wasm.push(0x07); // Section ID 7
        wasm.push(export_sec.len() as u8);
        wasm.extend_from_slice(&export_sec);

        // Section 10: Code (cuerpo de la función)
        // body: 0 locals, local.get 0 (0x20, 0), local.get 1 (0x20, 1), i32.add (0x6a), end (0x0b)
        let body_bytes = vec![
            0x00, // 0 declarations of local vars
            0x20, 0x00, // local.get 0
            0x20, 0x01, // local.get 1
            0x6a,       // i32.add
            0x0b,       // end
        ];
        let mut code_sec = Vec::new();
        code_sec.push(0x01); // 1 func body
        code_sec.push(body_bytes.len() as u8);
        code_sec.extend_from_slice(&body_bytes);

        wasm.push(0x0a); // Section ID 10
        wasm.push(code_sec.len() as u8);
        wasm.extend_from_slice(&code_sec);

        wasm
    }

    /// Genera un módulo WebAssembly que contiene un bucle infinito para probar fuel metering
    fn build_wasm_infinite_loop_module() -> Vec<u8> {
        let mut wasm = Vec::new();
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

        // Section 1: Type (0 params -> 0 returns)
        let type_sec = vec![0x01, 0x60, 0x00, 0x00];
        wasm.push(0x01);
        wasm.push(type_sec.len() as u8);
        wasm.extend_from_slice(&type_sec);

        // Section 3: Function
        let func_sec = vec![0x01, 0x00];
        wasm.push(0x03);
        wasm.push(func_sec.len() as u8);
        wasm.extend_from_slice(&func_sec);

        // Section 7: Export
        let export_sec = vec![
            0x01,
            0x03, b'r', b'u', b'n',
            0x00,
            0x00,
        ];
        wasm.push(0x07);
        wasm.push(export_sec.len() as u8);
        wasm.extend_from_slice(&export_sec);

        // Section 10: Code
        // loop: 0x03, 0x40 (void), br 0 (0x0c, 0x00), end (0x0b), end (0x0b)
        let body_bytes = vec![
            0x00, // 0 locals
            0x03, 0x40, // loop void
            0x0c, 0x00, // br 0
            0x0b,       // end loop
            0x0b,       // end func
        ];
        let mut code_sec = Vec::new();
        code_sec.push(0x01);
        code_sec.push(body_bytes.len() as u8);
        code_sec.extend_from_slice(&body_bytes);

        wasm.push(0x0a);
        wasm.push(code_sec.len() as u8);
        wasm.extend_from_slice(&code_sec);

        wasm
    }

    /// Genera un módulo WebAssembly que escribe fuera de la memoria lineal para probar aislamiento
    fn build_wasm_memory_overflow_module() -> Vec<u8> {
        let mut wasm = Vec::new();
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

        let type_sec = vec![0x01, 0x60, 0x00, 0x00];
        wasm.push(0x01);
        wasm.push(type_sec.len() as u8);
        wasm.extend_from_slice(&type_sec);

        let func_sec = vec![0x01, 0x00];
        wasm.push(0x03);
        wasm.push(func_sec.len() as u8);
        wasm.extend_from_slice(&func_sec);

        let mem_sec = vec![0x01, 0x00, 0x01]; // 1 página = 64KB
        wasm.push(0x05);
        wasm.push(mem_sec.len() as u8);
        wasm.extend_from_slice(&mem_sec);

        let export_sec = vec![0x01, 0x04, b'o', b'v', b'e', b'r', 0x00, 0x00];
        wasm.push(0x07);
        wasm.push(export_sec.len() as u8);
        wasm.extend_from_slice(&export_sec);

        // i32.const 0x00100000 (1 MB, excede 64KB), i32.const 42, i32.store, end
        // 0x00100000 en LEB128: 0x80, 0x80, 0x40
        let body_bytes = vec![
            0x00,
            0x41, 0x80, 0x80, 0x40, // i32.const 1048576
            0x41, 0x2a,             // i32.const 42
            0x36, 0x02, 0x00,       // i32.store align=2 offset=0
            0x0b,
        ];
        let mut code_sec = Vec::new();
        code_sec.push(0x01);
        code_sec.push(body_bytes.len() as u8);
        code_sec.extend_from_slice(&body_bytes);

        wasm.push(0x0a);
        wasm.push(code_sec.len() as u8);
        wasm.extend_from_slice(&code_sec);

        wasm
    }

    /// Genera un módulo WebAssembly que invoca los imports antos.log y antos.write_output
    fn build_wasm_antos_host_module() -> Vec<u8> {
        let mut wasm = Vec::new();
        wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

        // Type 0: (i32, i32, i32) -> void  (para log)
        // Type 1: (i32, i32) -> void        (para write_output)
        // Type 2: () -> void                (para run)
        let type_sec = vec![
            0x03, // 3 types
            0x60, 0x03, 0x7f, 0x7f, 0x7f, 0x00, // type 0
            0x60, 0x02, 0x7f, 0x7f, 0x00,       // type 1
            0x60, 0x00, 0x00,                   // type 2
        ];
        wasm.push(0x01);
        wasm.push(type_sec.len() as u8);
        wasm.extend_from_slice(&type_sec);

        // Section 2: Imports (antos.log type 0, antos.write_output type 1)
        let import_sec = vec![
            0x02, // 2 imports
            // import 0: antos.log
            0x05, b'a', b'n', b't', b'o', b's',
            0x03, b'l', b'o', b'g',
            0x00, 0x00, // kind 0, type 0
            // import 1: antos.write_output
            0x05, b'a', b'n', b't', b'o', b's',
            0x0c, b'w', b'r', b'i', b't', b'e', b'_', b'o', b'u', b't', b'p', b'u', b't',
            0x00, 0x01, // kind 0, type 1
        ];
        wasm.push(0x02);
        wasm.push(import_sec.len() as u8);
        wasm.extend_from_slice(&import_sec);

        // Section 3: Functions (func 2 tiene type 2)
        let func_sec = vec![0x01, 0x02];
        wasm.push(0x03);
        wasm.push(func_sec.len() as u8);
        wasm.extend_from_slice(&func_sec);

        // Section 5: Memory
        let mem_sec = vec![0x01, 0x00, 0x01];
        wasm.push(0x05);
        wasm.push(mem_sec.len() as u8);
        wasm.extend_from_slice(&mem_sec);

        // Section 7: Export
        let export_sec = vec![0x01, 0x03, b'r', b'u', b'n', 0x00, 0x02]; // func index 2
        wasm.push(0x07);
        wasm.push(export_sec.len() as u8);
        wasm.extend_from_slice(&export_sec);

        // Section 10: Code
        // En memoria escribir bytes 'O', 'K', luego llamar a write_output(0, 2)
        // i32.const 0, i32.const 79 ('O'), i32.store8
        // i32.const 1, i32.const 75 ('K'), i32.store8
        // i32.const 0, i32.const 2, call 1 (write_output)
        // end
        let body_bytes = vec![
            0x00, // locals
            0x41, 0x00, 0x41, 0xcf, 0x00, 0x3a, 0x00, 0x00, // mem[0] = 'O' (79 en signed leb128: 0xcf, 0x00)
            0x41, 0x01, 0x41, 0xcb, 0x00, 0x3a, 0x00, 0x00, // mem[1] = 'K' (75 en signed leb128: 0xcb, 0x00)
            0x41, 0x00, 0x41, 0x02, 0x10, 0x01,             // call write_output(0, 2)
            0x0b,
        ];
        let mut code_sec = Vec::new();
        code_sec.push(0x01);
        code_sec.push(body_bytes.len() as u8);
        code_sec.extend_from_slice(&body_bytes);

        wasm.push(0x0a);
        wasm.push(code_sec.len() as u8);
        wasm.extend_from_slice(&code_sec);

        wasm
    }

    #[test]
    fn test_wasm_binary_parsing_and_arithmetic_execution() {
        let bytes = build_wasm_add_module();
        let module = WasmModule::from_bytes(&bytes).expect("parse wasm add module");
        assert_eq!(module.functions.len(), 1);
        assert!(module.exports.contains_key("add"));

        let mut instance = WasmInstance::new(module, 100_000, HashMap::new())
            .expect("instance initialization");

        let result = instance.execute_export("add", &[15, 27]).expect("execute add");
        assert_eq!(result, Some(42));
        assert!(instance.fuel_consumed > 0);
    }

    #[test]
    fn test_wasm_fuel_metering_prevents_infinite_loops() {
        let bytes = build_wasm_infinite_loop_module();
        let module = WasmModule::from_bytes(&bytes).expect("parse infinite loop module");
        let mut instance = WasmInstance::new(module, 50, HashMap::new())
            .expect("instance initialization");

        let result = instance.execute_export("run", &[]);
        match result {
            Err(Trap::FuelExhausted { consumed }) => {
                assert!(consumed >= 50);
            }
            other => panic!("Esperado Trap::FuelExhausted, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_wasm_memory_isolation_and_out_of_bounds() {
        let bytes = build_wasm_memory_overflow_module();
        let module = WasmModule::from_bytes(&bytes).expect("parse memory overflow module");
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new())
            .expect("instance initialization");

        let result = instance.execute_export("over", &[]);
        match result {
            Err(Trap::MemoryOutOfBounds { requested, limit }) => {
                assert!(requested > limit);
            }
            other => panic!("Esperado Trap::MemoryOutOfBounds, obtenido: {:?}", other),
        }
    }

    #[test]
    fn test_wasm_host_imports_and_output_capture() {
        let bytes = build_wasm_antos_host_module();
        let module = WasmModule::from_bytes(&bytes).expect("parse host module");
        let mut instance = WasmInstance::new(module, 100_000, HashMap::new())
            .expect("instance initialization");

        let result = instance.execute_export("run", &[]).expect("execute run");
        assert_eq!(result, None);
        assert_eq!(instance.host_env.output, b"OK");
    }

    #[test]
    fn test_plugin_lifecycle_manifest_install_and_run() {
        let tmp_root = std::env::temp_dir().join(format!("antos_plugin_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_root);
        let src_dir = tmp_root.join("src_plugin");
        let plugins_dir = tmp_root.join("plugins");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&plugins_dir).unwrap();

        // 1. Crear plugin.toml y plugin.wasm en src_dir
        let toml_content = r#"
[plugin]
name = "math-helper"
version = "1.2.0"
author = "antOS Team"
description = "Operaciones matemáticas de prueba"
entrypoint = "plugin.wasm"

[capabilities]
actions = ["add"]
"#;
        std::fs::write(src_dir.join("plugin.toml"), toml_content).unwrap();
        std::fs::write(src_dir.join("plugin.wasm"), build_wasm_add_module()).unwrap();

        // 2. Instalar plugin
        let installed = PluginManager::install_plugin(&plugins_dir, &src_dir)
            .expect("install plugin");
        assert_eq!(installed.name, "math-helper");
        assert_eq!(installed.version, "1.2.0");
        assert_eq!(installed.capabilities, vec!["add".to_string()]);

        // 3. Listar plugins
        let list = PluginManager::list_plugins(&plugins_dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "math-helper");

        // 4. Ejecutar plugin
        let params = BTreeMap::new();
        let res = PluginManager::run_plugin(&plugins_dir, "math-helper", "add", &params);
        assert!(res.success);
        assert!(res.fuel_consumed > 0);
        assert!(res.memory_allocated_bytes >= 65536);

        let _ = std::fs::remove_dir_all(&tmp_root);
    }
}
