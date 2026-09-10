//! Arnés de tests `no_std` para el kernel (T31.16).
//!
//! El arnés de `#[test]` por defecto de Rust vive en el crate `test`, que
//! depende de `std` y no existe para `x86_64-unknown-none` /
//! `aarch64-unknown-none` — de ahí el `error[E0463]: can't find crate for
//! 'test'` al intentar `cargo test` sobre este crate sin más. La salida que
//! ofrece el propio compilador es `#[feature(custom_test_frameworks)]`: en
//! vez de delegar en el crate `test`, el compilador recolecta todos los
//! `#[test] fn` del crate en un slice y lo pasa a la función que
//! `#![test_runner(...)]` señale en `main.rs` — [`test_runner`], aquí.
//!
//! ## Por qué no hay un `TestResult` con conteo de fallos
//!
//! `Cargo.toml` fija `panic = "abort"` (obligatorio sin `std`: sin él, un
//! panic no tiene tabla de excepciones que desenrollar). Eso significa que
//! un test que entra en pánico no puede devolver el control a este bucle
//! como haría el arnés de `std` (que captura el panic por hilo y sigue con
//! el siguiente test) — se lleva por delante el binario entero, tests
//! restantes incluidos. El manejador de panics en modo test (`main.rs`,
//! `#[cfg(test)]`) imprime `KERNEL_TEST_RESULT: FAIL` antes de detener la
//! máquina, así que un fallo se ve igual de claro que si este bucle hubiera
//! terminado y contado los fallos él mismo — solo que los tests posteriores
//! al que entró en pánico no llegan a ejecutarse en esa tanda. Es la misma
//! limitación, y el mismo motivo, documentados en cualquier kernel Rust que
//! use `custom_test_frameworks` (el propio tutorial de referencia, blog_os,
//! lo señala igual). Corregir cada test que entre en pánico —el objetivo
//! del Alcance Técnico punto 4 del ticket— es lo que hace que esto deje de
//! importar en la práctica.

/// Cualquier `fn()` cuenta como test: no se necesita más para calificar, y
/// las 98 funciones `#[test]` que ya existían en el árbol antes de este
/// ticket tienen todas esa firma (ninguna devuelve `Result` ni nada más).
pub trait Testable {
    fn run(&self);
}

impl<T: Fn()> Testable for T {
    fn run(&self) {
        let name = core::any::type_name::<T>();
        crate::print!("{name} ... ");
        self();
        crate::println!("ok");
    }
}

/// La función que `#![test_runner(crate::test_framework::test_runner)]`
/// señala en `main.rs`. En modo test, `reexport_test_harness_main =
/// "test_main"` hace que el compilador genere `test_main()` — llamado
/// desde el punto de `kernel_main`/`kmain_arm64` donde se sustituye el
/// arranque normal (demos, controladores) por «ejecutar los tests y
/// parar» — que a su vez llama aquí con la lista completa de `#[test] fn`
/// recolectada del crate.
pub fn test_runner(tests: &[&dyn Testable]) {
    crate::println!();
    crate::println!("KERNEL_TEST_RUNNER: ejecutando {} tests", tests.len());
    for test in tests {
        test.run();
    }
    crate::println!("KERNEL_TEST_RESULT: PASS ({} tests)", tests.len());
}
