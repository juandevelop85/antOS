//! Global circular input event queue for antOS kernel.
//!
//! Stores hardware events (keyboard, mouse, touchpad) arriving via interrupts
//! or driver polling loops with zero allocation and lock-protected synchronization.

use core::sync::atomic::{AtomicU64, Ordering};

use super::InputEvent;
use crate::sync::SpinLock;

const QUEUE_CAPACITY: usize = 256;

/// Monotonic count of every input event ever enqueued (keyboard, mouse, all
/// transports). Used by the peripheral smoke test (T28.10) to confirm the
/// kernel received injected input, and surfaced through `SYS_SYSINFO`.
static TOTAL_EVENTS: AtomicU64 = AtomicU64::new(0);

/// Fixed-size circular buffer for kernel input events.
pub struct InputEventQueue {
    buffer: [Option<InputEvent>; QUEUE_CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    dropped: usize,
}

impl InputEventQueue {
    pub const fn new() -> Self {
        Self {
            buffer: [None; QUEUE_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            dropped: 0,
        }
    }

    /// Enqueues an event. If full, drops it without blocking or allocating.
    pub fn push(&mut self, event: InputEvent) -> bool {
        if self.count >= QUEUE_CAPACITY {
            self.dropped = self.dropped.wrapping_add(1);
            return false;
        }
        self.buffer[self.tail] = Some(event);
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
        self.count += 1;
        true
    }

    /// Dequeues the oldest input event, if any.
    pub fn pop(&mut self) -> Option<InputEvent> {
        if self.count == 0 {
            return None;
        }
        let event = self.buffer[self.head].take();
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.count -= 1;
        event
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    pub fn dropped_count(&self) -> usize {
        self.dropped
    }
}

/// Global system input event queue.
pub static GLOBAL_INPUT_QUEUE: SpinLock<InputEventQueue> = SpinLock::new(InputEventQueue::new());

/// Enqueues an input event into the global queue.
pub fn push_event(event: InputEvent) -> bool {
    TOTAL_EVENTS.fetch_add(1, Ordering::Relaxed);
    GLOBAL_INPUT_QUEUE.lock().push(event)
}

/// Total number of input events enqueued since boot, across every device and
/// transport.
pub fn total_events() -> u64 {
    TOTAL_EVENTS.load(Ordering::Relaxed)
}

/// Pops an input event from the global queue.
pub fn pop_event() -> Option<InputEvent> {
    GLOBAL_INPUT_QUEUE.lock().pop()
}

/// Returns true if there are unhandled input events pending.
pub fn has_events() -> bool {
    !GLOBAL_INPUT_QUEUE.lock().is_empty()
}
