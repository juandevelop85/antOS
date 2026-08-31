//! Un cerrojo para un mundo sin sistema operativo.
//!
//! Rust exige que todo `static` sea `Sync`: si algo es alcanzable desde
//! cualquier hilo, tiene que ser seguro tocarlo desde cualquier hilo. El
//! puerto serie es estado global compartido, así que necesita un cerrojo.
//!
//! `static mut` parecería el atajo, y es una trampa: nada impide dos accesos
//! simultáneos, el compilador no comprueba nada, y desde la edición 2024
//! tomar una referencia a uno es directamente un error. El atajo no existe.
//!
//! Un `Mutex` de verdad duerme al hilo que no consigue entrar y le pide al
//! planificador que despierte a otro. Aquí no hay planificador — el SO somos
//! nosotros y aún no lo hemos escrito. Lo único que puede hacer quien no
//! entra es **girar en vacío** hasta que el cerrojo se libere.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    /// La única forma de obtener mutabilidad interior. `UnsafeCell` es el
    /// ladrillo con el que están hechos Cell, RefCell y Mutex: le dice al
    /// compilador «aquí puede haber aliasing mutable, no asumas nada».
    value: UnsafeCell<T>,
}

/// La promesa que hace posible usarlo en un `static`.
///
/// Decimos al compilador que compartir un `SpinLock<T>` entre hilos es
/// seguro. Es cierto **porque** `lock()` garantiza que solo un hilo tiene la
/// referencia interior a la vez. Si ese razonamiento fuera falso, esta línea
/// sería una mentira que el compilador no puede detectar.
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    /// `const fn` para poder construirlo en un `static`: se inicializa en
    /// tiempo de compilación, sin código de arranque que lo prepare.
    pub const fn new(value: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        // EL ARREGLO ANUNCIADO EN LA FASE 1.
        //
        // Sin esto, una interrupción que salte mientras sostenemos el cerrojo
        // y cuyo manejador intente tomarlo cuelga la máquina para siempre: el
        // manejador gira esperando, y el código interrumpido —el único que
        // puede liberarlo— no volverá a ejecutarse nunca.
        //
        // Apagar las interrupciones ANTES de intentar entrar hace imposible
        // esa situación. Apagarlas después de entrar no serviría: la ventana
        // entre una cosa y otra es justo donde ocurre el fallo.
        let restore_interrupts = interrupts_enabled();
        disable_interrupts();

        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Girar leyendo, no intercambiando: un `compare_exchange` en
            // bucle martillea la línea de caché y hace más lento justo al
            // que tiene el cerrojo y queremos que termine cuanto antes.
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinGuard { lock: self, restore_interrupts }
    }
}

/// Mientras vive, el cerrojo está tomado. Al caer, lo suelta.
///
/// Es el mismo patrón que `MutexGuard` de `std`: el sistema de tipos impide
/// olvidarse de liberar, porque liberar no es algo que tú llames.
pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
    /// Si al entrar las interrupciones estaban activas, hay que volver a
    /// activarlas al salir — y solo entonces.
    restore_interrupts: bool,
}

impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: tenemos el cerrojo, así que nadie más tiene una referencia.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: igual, y además `&mut self` garantiza exclusividad aquí.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        // `Release` empareja con el `Acquire` de lock(): todo lo que
        // escribimos dentro de la sección crítica queda visible para el
        // siguiente que entre. Sin esto, el procesador podría reordenar
        // nuestras escrituras por detrás de la liberación del cerrojo.
        self.lock.locked.store(false, Ordering::Release);

        // Reactivar DESPUÉS de soltar. Al revés dejaría una ventana en la que
        // una interrupción encontraría el cerrojo todavía tomado.
        if self.restore_interrupts {
            enable_interrupts();
        }
    }
}

/// El bit 9 de RFLAGS (IF) dice si la CPU atiende interrupciones.
fn interrupts_enabled() -> bool {
    let flags: u64;
    // SAFETY: solo lee el registro de banderas a través de la pila.
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
    }
    flags & (1 << 9) != 0
}

fn disable_interrupts() {
    // SAFETY: `cli` solo baja IF. No preserves_flags, precisamente porque
    // modificar las banderas es lo único que hace.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
}

fn enable_interrupts() {
    // SAFETY: `sti` sube IF. Solo se llama para restaurar un estado previo.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
}

/// Estado global que solo se toca durante la inicialización.
///
/// La GDT y la IDT tienen que vivir para siempre —la CPU guarda punteros a
/// ellas— y se escriben una sola vez, antes de que existan interrupciones o
/// segundos núcleos. Un cerrojo ahí sería peso muerto; `static mut`, la
/// trampa de siempre. Esto es el término medio: sin cerrojo, pero con el
/// `unsafe` visible en cada acceso para que la promesa quede escrita.
pub struct InitOnly<T> {
    value: UnsafeCell<T>,
}

/// Igual que arriba: la promesa es que solo se toca durante el arranque.
unsafe impl<T: Send> Sync for InitOnly<T> {}

impl<T> InitOnly<T> {
    pub const fn new(value: T) -> Self {
        InitOnly { value: UnsafeCell::new(value) }
    }

    /// # Safety
    /// Solo durante la inicialización, y solo desde un sitio a la vez.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut(&self) -> &'static mut T {
        unsafe { &mut *self.value.get() }
    }

    /// # Safety
    /// El valor debe estar ya inicializado y no volver a mutarse.
    pub unsafe fn get(&self) -> &'static T {
        unsafe { &*self.value.get() }
    }
}
