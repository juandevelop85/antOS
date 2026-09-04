//! Global Heap Allocator for antOS Userspace backed by SYS_MMAP (T23.5).

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::syscall;

const CHUNK_SIZE: usize = 64 * 1024; // Request 64 KiB per mmap expansion

/// A bump allocator that dynamically requests memory pages from the kernel via `SYS_MMAP`.
pub struct UserHeapAllocator {
    current: AtomicUsize,
    limit: AtomicUsize,
}

impl UserHeapAllocator {
    pub const fn new() -> Self {
        Self {
            current: AtomicUsize::new(0),
            limit: AtomicUsize::new(0),
        }
    }
}

unsafe impl GlobalAlloc for UserHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        loop {
            let cur = self.current.load(Ordering::SeqCst);
            let lim = self.limit.load(Ordering::SeqCst);

            // Calculate aligned start address
            let aligned = (cur + align - 1) & !(align - 1);
            let new_cur = aligned + size;

            if cur != 0 && new_cur <= lim {
                if self
                    .current
                    .compare_exchange_weak(cur, new_cur, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return aligned as *mut u8;
                }
            } else {
                // Out of memory in current chunk: request new pages from kernel via SYS_MMAP
                let request_size = ((size + align).max(CHUNK_SIZE) + 4095) & !4095;
                match syscall::mmap(request_size) {
                    Ok(ptr) => {
                        let new_base = ptr as usize;
                        let new_lim = new_base + request_size;
                        let new_aligned = (new_base + align - 1) & !(align - 1);
                        let final_cur = new_aligned + size;

                        self.limit.store(new_lim, Ordering::SeqCst);
                        self.current.store(final_cur, Ordering::SeqCst);
                        return new_aligned as *mut u8;
                    }
                    Err(_) => return core::ptr::null_mut(),
                }
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator does not reclaim individual allocations
    }
}
