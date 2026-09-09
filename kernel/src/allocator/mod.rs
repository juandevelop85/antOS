//! El heap: donde nacen `Box`, `Vec` y `String`.
//!
//! Rust no trae asignador en `core`. Para usar el crate `alloc` hay que
//! proporcionar uno marcado con `#[global_allocator]`, y a partir de ese
//! momento todo el ecosistema de colecciones de Rust funciona igual que en
//! cualquier programa normal.
//!
//! Aquí hay dos, y se puede elegir al compilar:
//!
//! ```bash
//! cargo build                     # lista enlazada (por defecto)
//! cargo build --features bump     # asignador de puntero
//! ```
//!
//! El segundo existe para poder *medir* por qué no basta, no como adorno.

pub mod bump;
pub mod linked_list;

use crate::sync::{SpinGuard, SpinLock};

/// `GlobalAlloc` recibe `&self`, no `&mut self`: el asignador es un `static`
/// compartido. La mutabilidad tiene que venir de dentro, y con cerrojo —
/// porque una interrupción puede pedir memoria en cualquier momento.
///
/// Nuestro `SpinLock` apaga las interrupciones mientras está tomado (Fase 2),
/// así que asignar dentro de un manejador es seguro.
pub struct Locked<A> {
    inner: SpinLock<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: SpinLock::new(inner),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_, A> {
        self.inner.lock()
    }
}

/// Redondea hacia arriba hasta el múltiplo de `align`.
/// `align` siempre es potencia de dos, así que la resta de uno da la máscara.
pub fn align_up(address: usize, align: usize) -> usize {
    (address + align - 1) & !(align - 1)
}

#[cfg(feature = "bump")]
#[global_allocator]
static ALLOCATOR: Locked<bump::BumpAllocator> = Locked::new(bump::BumpAllocator::new());

#[cfg(not(feature = "bump"))]
#[global_allocator]
static ALLOCATOR: Locked<linked_list::LinkedListAllocator> =
    Locked::new(linked_list::LinkedListAllocator::new());

/// # Safety
/// El rango debe estar mapeado y no usarse para nada más. Solo una vez.
pub unsafe fn init(heap_start: usize, heap_size: usize) {
    unsafe { ALLOCATOR.lock().init(heap_start, heap_size) };
}

pub fn used() -> usize {
    ALLOCATOR.lock().used()
}

pub fn name() -> &'static str {
    if cfg!(feature = "bump") {
        "puntero (bump)"
    } else {
        "lista enlazada"
    }
}
