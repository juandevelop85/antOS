//! Multitarea cooperativa sobre `async`/`await`.
//!
//! Rust trae la mitad del trabajo hecho: `async fn` se compila a una máquina
//! de estados que implementa `Future`, y `.await` es un punto donde esa
//! máquina puede detenerse y ceder. Lo que el lenguaje NO trae es quién
//! decide a qué tarea le toca — eso es el ejecutor, y en un SO lo escribes tú.
//!
//! ## Cooperativa quiere decir que hay confianza
//!
//! Una tarea solo cede en un `.await`. Un bucle sin `.await` bloquea la
//! máquina entera, porque nadie puede quitarle el turno. A cambio no hace
//! falta ni una pila por tarea ni una línea de ensamblador: cambiar de tarea
//! es devolver de una función.
//!
//! La alternativa —hilos expulsivos con cambio de contexto— es lo que hace un
//! SO de verdad, y necesita guardar registros a mano y una pila por hilo.

pub mod executor;
pub mod keyboard;
pub mod pcb;
pub mod scheduler;
pub mod timer;

#[allow(unused_imports)]
pub use pcb::{CpuContext, ProcessControlBlock, ProcessState, ThreadControlBlock, ThreadState};
#[allow(unused_imports)]
pub use scheduler::Scheduler;

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        TaskId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Task {
    id: TaskId,
    /// `Pin` porque una máquina de estados `async` puede guardar referencias a
    /// su propio interior; moverla de sitio las dejaría apuntando a basura.
    /// `Box` porque su tamaño no se conoce hasta el momento de crearla.
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            id: TaskId::next(),
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}
