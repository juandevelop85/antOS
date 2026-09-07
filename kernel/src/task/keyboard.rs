//! El teclado deja de imprimir desde la interrupción.
//!
//! En la Fase 2, el manejador decodificaba e imprimía. Estaba mal por dos
//! razones: un manejador debe durar lo mínimo —mientras corre, el resto del
//! sistema está parado— y así el teclado no podía alimentar a nada.
//!
//! Ahora el manejador solo empuja el scancode a una cola y despierta a quien
//! esperaba. Todo el trabajo real pasa a una tarea normal, interrumpible.

use crate::sync::SpinLock;
use crate::{print, println};
use alloc::string::String;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

const CAPACITY: usize = 128;

/// Cola circular de tamaño fijo.
///
/// Sin asignar memoria a propósito: `push` corre dentro del manejador de
/// interrupción, y llamar allí al asignador global es pedir problemas.
struct ScancodeQueue {
    buffer: [u8; CAPACITY],
    head: usize,
    tail: usize,
    dropped: u64,
}

impl ScancodeQueue {
    const fn new() -> Self {
        ScancodeQueue { buffer: [0; CAPACITY], head: 0, tail: 0, dropped: 0 }
    }

    /// Devuelve si el scancode entró. Con la cola llena se descarta: preferimos
    /// perder una pulsación a bloquear una interrupción.
    fn push(&mut self, scancode: u8) -> bool {
        let next = (self.tail + 1) % CAPACITY;
        if next == self.head {
            self.dropped += 1;
            return false;
        }
        self.buffer[self.tail] = scancode;
        self.tail = next;
        true
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            return None;
        }
        let scancode = self.buffer[self.head];
        self.head = (self.head + 1) % CAPACITY;
        Some(scancode)
    }
}

static SCANCODES: SpinLock<ScancodeQueue> = SpinLock::new(ScancodeQueue::new());
static WAKER: SpinLock<Option<Waker>> = SpinLock::new(None);

/// Se llama DESDE el manejador de interrupción del teclado.
///
/// No asigna, no imprime y no se queda con ningún cerrojo mientras despierta.
pub fn add_scancode(scancode: u8) {
    if let Some(event) = crate::input::decode_ps2_set1(scancode) {
        crate::input::push_event(event);
    }
    if !SCANCODES.lock().push(scancode) {
        return;
    }
    // Clonar y soltar el cerrojo antes de despertar: `wake` toma otro cerrojo
    // (el de la cola del ejecutor), y anidar cerrojos es como se construyen
    // los bloqueos mutuos.
    let waker = WAKER.lock().clone();
    if let Some(waker) = waker {
        waker.wake();
    }
}

pub struct NextScancode;

impl Future for NextScancode {
    type Output = u8;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<u8> {
        // Camino rápido: si ya hay algo, ni siquiera hace falta registrar nada.
        if let Some(scancode) = SCANCODES.lock().pop() {
            return Poll::Ready(scancode);
        }

        *WAKER.lock() = Some(context.waker().clone());

        // Y ahora se vuelve a mirar. No es paranoia: entre la comprobación de
        // arriba y el registro del waker pudo saltar la interrupción, ver el
        // hueco vacío y no despertar a nadie. Sin esta segunda mirada, esa
        // pulsación se pierde y la tarea duerme para siempre.
        match SCANCODES.lock().pop() {
            Some(scancode) => {
                *WAKER.lock() = None;
                Poll::Ready(scancode)
            }
            None => Poll::Pending,
        }
    }
}

/// Tabla del "scancode set 1". Un 0 es una tecla sin carácter (shift, control).
const SCANCODE_SET1: &[u8] =
    b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";

fn decode(scancode: u8) -> Option<u8> {
    match SCANCODE_SET1.get(scancode as usize) {
        Some(&0) | None => None,
        Some(&character) => Some(character),
    }
}

/// Lee teclas para siempre y va montando líneas.
///
/// Fíjate en que es una `async fn` corriente: sin cerrojos, sin pensar en
/// interrupciones, y con estado propio (`line`) que sobrevive a cada `.await`
/// sin necesidad de una pila para ella sola.
/// Superseded as the interactive entry point by the userspace shell
/// (T26.5, `antos-init` mode 4, driven via `SYS_READ`/`input::drain_ascii`).
/// Kept unspawned but intact: it is still the clearest, dependency-free demo
/// of the async keyboard queue for anyone extending the kernel-side executor.
#[allow(dead_code)]
pub async fn keyboard_task() {
    let mut line = String::new();

    loop {
        let scancode = NextScancode.await;

        // El bit 7 marca que la tecla se ha soltado.
        if scancode & 0x80 != 0 {
            continue;
        }

        match decode(scancode) {
            Some(b'\n') => {
                println!();
                println!("  línea · «{line}»");
                line.clear();
            }
            Some(b'\x08') => {
                line.pop();
            }
            Some(character) => {
                line.push(character as char);
                print!("{}", character as char);
            }
            None => {}
        }
    }
}
