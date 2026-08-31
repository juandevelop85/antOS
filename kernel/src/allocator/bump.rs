//! Asignador de puntero: la forma más simple que funciona.
//!
//! Guarda un puntero al primer byte libre y, en cada petición, lo empuja
//! hacia delante. Asignar son tres instrucciones y no hay metadatos.
//!
//! ## Y por qué no basta
//!
//! No sabe liberar. `dealloc` solo puede llevar la cuenta de cuántas
//! asignaciones siguen vivas, y devolver el puntero al principio cuando la
//! cuenta llega a cero. Si UNA sola asignación sobrevive —y en un kernel
//! siempre sobrevive alguna— el puntero no retrocede nunca y la memoria se
//! agota aunque se esté liberando todo lo demás.
//!
//! La Fase 3 lo mide en vez de contarlo.

use super::{align_up, Locked};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
    /// Cuántas asignaciones siguen vivas. Es lo único que se puede saber sin
    /// metadatos por bloque.
    allocations: usize,
}

// Solo se compila como asignador activo con --features bump; el resto del
// tiempo se queda aquí como referencia de lo que se mejora.
#[cfg_attr(not(feature = "bump"), allow(dead_code))]
impl BumpAllocator {
    pub const fn new() -> Self {
        BumpAllocator { heap_start: 0, heap_end: 0, next: 0, allocations: 0 }
    }

    /// # Safety
    /// El rango debe estar mapeado y ser de uso exclusivo del heap.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start + heap_size;
        self.next = heap_start;
    }

    pub fn used(&self) -> usize {
        self.next - self.heap_start
    }
}

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.lock();

        let start = align_up(bump.next, layout.align());
        let Some(end) = start.checked_add(layout.size()) else {
            return ptr::null_mut();
        };
        if end > bump.heap_end {
            // Devolver null hace que Rust llame al manejador de error de
            // asignación, que entra en panic. En nuestro caso, con mensaje.
            return ptr::null_mut();
        }

        bump.next = end;
        bump.allocations += 1;
        start as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut bump = self.lock();
        bump.allocations -= 1;

        // AQUÍ ESTÁ EL DEFECTO. Sin metadatos no se sabe dónde estaba ese
        // bloque ni si era el último, así que lo único que se puede hacer es
        // esperar a que no quede nada vivo. Una sola asignación de larga vida
        // impide para siempre recuperar un solo byte.
        if bump.allocations == 0 {
            bump.next = bump.heap_start;
        }
    }
}
