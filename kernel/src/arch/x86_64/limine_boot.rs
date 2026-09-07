//! Limine boot protocol entry point for x86_64 (T27.1, `--features limine`).
//!
//! Limine calls `_start` directly — long mode, paging already on, `rsp`
//! already pointing at a valid ≥64 KiB stack, GDT loaded, interrupts masked
//! (see the protocol's machine-state guarantees) — which is a completely
//! different contract from the `bootloader` crate's own `entry_point!` macro
//! (used by `run.sh`'s BIOS path), so this cannot share `_start` with it;
//! `build.rs` links only one of the two in, based on the `limine` feature.
//!
//! Rather than duplicate `kernel_main`'s ~250 lines of driver/VFS/scheduler
//! bring-up for a second boot convention, this module's only job is to turn
//! what Limine handed us into a real, valid `bootloader_api::BootInfo` and
//! call the exact same `kernel_main` the `bootloader` crate path already
//! calls. `BootInfo` is `#[non_exhaustive]` but ships a public constructor
//! and public fields precisely so embedders like this one can build one.

use crate::limine;
use bootloader_api::info::{BootInfo, MemoryRegion, MemoryRegionKind, MemoryRegions, Optional};

/// Same bound as `main.rs`'s own `MAX_MEMORY_REGIONS` — generous for any
/// real or emulated machine's memory map.
const MAX_LIMINE_REGIONS: usize = 128;

static mut REGION_STORAGE: [MemoryRegion; MAX_LIMINE_REGIONS] =
    [MemoryRegion::empty(); MAX_LIMINE_REGIONS];

/// Holds the synthesized `BootInfo` for `'static` lifetime: `_start` never
/// returns, so a stack local would live "forever" in practice too, but a
/// static avoids leaning on that informally.
static mut BOOT_INFO_STORAGE: Option<BootInfo> = None;

/// Converts Limine's memory map into `bootloader_api`'s own `MemoryRegion`
/// shape, copied into `REGION_STORAGE` — the same "fixed static buffer,
/// because the frame allocator has to exist before the heap does" pattern
/// `main.rs::regions_from_bootloader_api` uses for the `bootloader` crate's
/// map.
///
/// # Safety
/// Must run exactly once, before anything else touches `REGION_STORAGE`.
unsafe fn build_memory_regions() -> MemoryRegions {
    let response = limine::MEMMAP_REQUEST.response;
    if response.is_null() {
        panic!("limine: no memmap response (unsupported base revision?)");
    }
    let response = unsafe { &*response };
    let count = (response.entry_count as usize).min(MAX_LIMINE_REGIONS);
    let entries = unsafe { core::slice::from_raw_parts(response.entries, count) };

    let buf = unsafe { &mut *core::ptr::addr_of_mut!(REGION_STORAGE) };
    for (i, entry_ptr) in entries.iter().enumerate() {
        let entry = unsafe { &**entry_ptr };
        buf[i] = MemoryRegion {
            start: entry.base,
            end: entry.base + entry.length,
            kind: if entry.kind == limine::MEMMAP_USABLE {
                MemoryRegionKind::Usable
            } else {
                MemoryRegionKind::UnknownBios(entry.kind as u32)
            },
        };
    }
    MemoryRegions::from(&mut buf[..count])
}

/// The Limine boot protocol entry point. Exported literally as `_start`
/// (the symbol name the linker picks as the ELF entry point by default),
/// standing in for the `bootloader` crate's `entry_point!`-generated one.
///
/// # Safety
/// Only valid as the very first code to run after Limine hands off control —
/// it assumes Limine's documented x86_64 machine-state guarantees hold
/// (long mode, paging enabled, a valid stack already installed in `rsp`).
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Confirm Limine actually loaded us at the base revision we asked for —
    // it zeroes the third element on success; a non-zero value here means it
    // silently fell back to an older revision this code does not target.
    if limine::BASE_REVISION[2] != 0 {
        panic!("limine: unsupported base revision (bootloader too old?)");
    }

    let hhdm_response = limine::HHDM_REQUEST.response;
    if hhdm_response.is_null() {
        panic!("limine: no HHDM response (unsupported base revision?)");
    }
    let hhdm_offset = unsafe { (*hhdm_response).offset };

    // SAFETY: first and only call, before any other code touches these statics.
    let regions = unsafe { build_memory_regions() };
    let mut boot_info = BootInfo::new(regions);
    boot_info.physical_memory_offset = Optional::Some(hhdm_offset);
    // The framebuffer request is deliberately not wired in yet: `kernel_main`
    // only uses `boot_info.framebuffer` to light up the *graphical* console,
    // and `print!`/`println!` already fan out to the serial port
    // unconditionally (`arch::_print`), so boot output is fully visible over
    // serial without it. Follow-up work can plug
    // `limine::FRAMEBUFFER_REQUEST.response` in here the same way HHDM is
    // wired in above.
    boot_info.framebuffer = Optional::None;

    // SAFETY: first and only write to BOOT_INFO_STORAGE; the resulting
    // reference is handed to a function that runs until the machine halts,
    // so treating it as `'static` reflects how long it actually lives.
    unsafe {
        *core::ptr::addr_of_mut!(BOOT_INFO_STORAGE) = Some(boot_info);
        let boot_info = (&mut *core::ptr::addr_of_mut!(BOOT_INFO_STORAGE))
            .as_mut()
            .unwrap();
        crate::kernel_main(boot_info)
    }
}
