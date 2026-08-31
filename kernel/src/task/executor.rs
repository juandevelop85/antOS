//! El ejecutor: quien decide a qué tarea le toca.
//!
//! Guarda las tareas vivas y una cola de las que están listas. Sacar de la
//! cola, sondear, y si la tarea sigue pendiente, olvidarse de ella hasta que
//! alguien la despierte. Ese «alguien» suele ser un manejador de interrupción.

use super::{Task, TaskId};
use crate::sync::{disable_interrupts, enable_interrupts, SpinLock};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::task::Wake;
use core::task::{Context, Poll, Waker};

/// Cuántas tareas puede haber pendientes de despertar a la vez.
///
/// Se reserva de golpe al arrancar por un motivo concreto: encolar ocurre
/// dentro de un manejador de interrupción, y allí una reasignación de la cola
/// llamaría al asignador global en el peor momento posible.
const READY_CAPACITY: usize = 128;

pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    /// `Arc` porque cada waker se queda con una referencia a esta cola: es su
    /// único vínculo con el ejecutor.
    ready: Arc<SpinLock<VecDeque<TaskId>>>,
    /// Crear un `Waker` asigna memoria. Como se sondea la misma tarea miles de
    /// veces, se guarda el suyo en vez de rehacerlo en cada vuelta.
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            ready: Arc::new(SpinLock::new(VecDeque::with_capacity(READY_CAPACITY))),
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, task: Task) {
        let id = task.id;
        if self.tasks.insert(id, task).is_some() {
            panic!("identificador de tarea repetido: {id:?}");
        }
        self.ready.lock().push_back(id);
    }

    pub fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }

    fn run_ready_tasks(&mut self) {
        loop {
            // El cerrojo se suelta ANTES de sondear. Sondear con él tomado
            // impediría que la propia tarea se reencolase.
            let next = self.ready.lock().pop_front();
            let Some(task_id) = next else { return };

            let ready = self.ready.clone();
            let Some(task) = self.tasks.get_mut(&task_id) else {
                // Despertada después de terminar: no es un error, solo llegó
                // tarde el aviso.
                continue;
            };

            let waker = self
                .waker_cache
                .entry(task_id)
                .or_insert_with(|| Waker::from(Arc::new(TaskWaker { id: task_id, ready })));

            match task.poll(&mut Context::from_waker(waker)) {
                Poll::Ready(()) => {
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    /// Duerme la CPU cuando no hay nada que hacer.
    ///
    /// Aquí vive la carrera más sutil de todo el fichero. Sin cuidado:
    ///
    /// 1. se comprueba que la cola está vacía
    /// 2. **salta una interrupción y encola una tarea**
    /// 3. se ejecuta `hlt`
    ///
    /// La máquina se duerme con trabajo pendiente y no despierta hasta la
    /// siguiente interrupción — o nunca, si no vuelve a haberla.
    ///
    /// La solución es apagar las interrupciones antes de mirar, y usar el par
    /// `sti; hlt`: `sti` no surte efecto hasta DESPUÉS de la instrucción
    /// siguiente, así que ninguna interrupción puede colarse entre las dos.
    /// Ese retardo de una instrucción existe en x86 exactamente para esto.
    fn sleep_if_idle(&self) {
        disable_interrupts();

        if self.ready.lock().is_empty() {
            // SAFETY: el par es indivisible por diseño de la arquitectura.
            unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)) };
        } else {
            enable_interrupts();
        }
    }
}

struct TaskWaker {
    id: TaskId,
    ready: Arc<SpinLock<VecDeque<TaskId>>>,
}

/// Despertar es solo «vuelve a poner esta tarea en la cola».
///
/// Tiene que ser barato y no bloquear: lo llaman manejadores de interrupción.
impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.ready.lock().push_back(self.id);
    }
}
