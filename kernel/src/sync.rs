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
        SpinGuard { lock: self }
    }
}

/// Mientras vive, el cerrojo está tomado. Al caer, lo suelta.
///
/// Es el mismo patrón que `MutexGuard` de `std`: el sistema de tipos impide
/// olvidarse de liberar, porque liberar no es algo que tú llames.
pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
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
    }
}

// ADVERTENCIA para la Fase 2, cuando existan las interrupciones:
//
// Este cerrojo se bloquea a sí mismo si una interrupción salta mientras lo
// tenemos tomado y su manejador intenta tomarlo también. El manejador gira
// esperando a que se libere; el código interrumpido no puede continuar para
// liberarlo. La máquina se queda ahí para siempre.
//
// La solución habitual es deshabilitar interrupciones mientras se sostiene el
// cerrojo. Lo dejamos anotado aquí porque el bug aparecerá en cuanto tengamos
// un manejador de teclado que quiera imprimir.
