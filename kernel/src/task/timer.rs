//! Dormir una tarea hasta que pasen N ticks del temporizador.

use crate::sync::SpinLock;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll, Waker};

/// The LAPIC timer runs at 100 Hz, so 100 ticks = 1 second.
pub const TICKS_PER_SECOND: u64 = 100;

static TICKS: AtomicU64 = AtomicU64::new(0);
static WAKER: SpinLock<Option<Waker>> = SpinLock::new(None);
static WAKE_AT: AtomicU64 = AtomicU64::new(u64::MAX);

/// Se llama DESDE el manejador del temporizador. Barato a propósito.
pub fn tick() {
    let now = TICKS.fetch_add(1, Ordering::Relaxed) + 1;

    if now < WAKE_AT.load(Ordering::Relaxed) {
        return;
    }
    WAKE_AT.store(u64::MAX, Ordering::Relaxed);

    let waker = WAKER.lock().take();
    if let Some(waker) = waker {
        waker.wake();
    }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// LIMITACIÓN CONOCIDA: solo una tarea puede dormir a la vez. La segunda
/// pisaría el waker de la primera, que no despertaría jamás.
///
/// Lo correcto es una lista de temporizadores ordenada por vencimiento. Se
/// deja así porque con una sola tarea durmiendo el mecanismo se ve entero, y
/// una cola de temporizadores es un capítulo aparte.
pub fn sleep(ticks_to_wait: u64) -> Sleep {
    Sleep {
        deadline: TICKS.load(Ordering::Relaxed) + ticks_to_wait,
    }
}

pub struct Sleep {
    deadline: u64,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if TICKS.load(Ordering::Relaxed) >= self.deadline {
            return Poll::Ready(());
        }

        *WAKER.lock() = Some(context.waker().clone());
        WAKE_AT.store(self.deadline, Ordering::Relaxed);

        // La misma carrera que en el teclado: el tick pudo llegar entre la
        // comprobación y el registro.
        if TICKS.load(Ordering::Relaxed) >= self.deadline {
            WAKE_AT.store(u64::MAX, Ordering::Relaxed);
            *WAKER.lock() = None;
            return Poll::Ready(());
        }
        Poll::Pending
    }
}
