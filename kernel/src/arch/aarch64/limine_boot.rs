//! Limine boot protocol entry point for AArch64 (T27.1, `--features limine`).
//!
//! `arch::aarch64::entry::_start` — the direct-QEMU-boot path (`-kernel`,
//! no bootloader) — earns its keep by handling a CPU that could be at EL1,
//! EL2 or EL3 with the MMU off and nothing else set up. None of that applies
//! under Limine: base revisions up to 5 guarantee EL1 already, `SCTLR_EL1.M`
//! is already set, `TTBR1_EL1` already maps this kernel at its higher-half
//! link address, and `sp` already points at a bootloader-provided ≥64 KiB
//! stack — so this entry point is a plain Rust function, no hand-written
//! assembly landing pad required.
//!
//! What it *does* still have to do by hand: install a `TTBR0_EL1` mapping
//! for the peripherals (GIC, PL011, VirtIO) and the user-space ELF window
//! that `kmain_arm64` expects to reach at their usual low physical
//! addresses — Limine leaves `TTBR0_EL1` "unspecified, free for the kernel
//! to use", it does not map any of that for us. See
//! `mmu::init_ttbr0_under_limine` for exactly why that has to be a
//! different function from the direct-boot path's `mmu::init`, not just a
//! different call site.

use crate::limine;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Limine hands off in "EL1t" (`SPSel` = 0, so `sp` is really `SP_EL0`)
    // rather than the "EL1h" (`SP_EL1`) mode this kernel assumes throughout
    // — `arch::aarch64::entry`, the direct-QEMU-boot path, sets this
    // explicitly for exactly this reason, but starts from MMU-off, so it
    // never had to preserve a live stack across the switch the way this
    // does. Left unfixed, the very first thing that writes `sp_el0` for its
    // own purposes (the EL0 self-test's `enter_user_mode`, setting up the
    // *user* stack) silently overwrites the kernel's own active stack
    // pointer instead, since `sp` and `sp_el0` are the same physical
    // register while `SPSel` = 0 — found from a QEMU exception trace
    // ("Current EL with SP0 (Synchronous)") right at that call. This has to
    // be the very first thing this function does, before anything else
    // (even the base revision check below) risks touching the stack.
    unsafe {
        core::arch::asm!(
            "mov x9, sp",
            "msr spsel, #1",
            "mov sp, x9",
            out("x9") _,
            options(nomem, nostack),
        );
    }

    if limine::BASE_REVISION[2] != 0 {
        panic!("limine: unsupported base revision (bootloader too old?)");
    }

    // Must run before anything else touches a peripheral: `kmain_arm64`'s
    // very first line reads/writes the PL011 UART over MMIO, and under
    // Limine the MMU is already on with `TTBR0_EL1` "unspecified" — without
    // this, that first touch faults with no vector table installed yet
    // (`exceptions::init()` hasn't run) and the machine hangs silently.
    crate::arch::aarch64::mmu::init_ttbr0_under_limine();

    // Enable FP/SIMD (NEON) at EL1/EL0 — guaranteed already done for us only
    // from base revision 6 onward; harmless to redo if it was.
    unsafe {
        let mut cpacr: u64;
        core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr, options(nomem, nostack));
        cpacr |= 0b11 << 20;
        core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr, options(nomem, nostack));
        core::arch::asm!("isb", options(nomem, nostack));
    }

    let dtb_response = limine::DTB_REQUEST.response;
    let dtb_ptr = if dtb_response.is_null() {
        0
    } else {
        unsafe { (*dtb_response).dtb_ptr as u64 }
    };

    crate::kmain_arm64(dtb_ptr, true)
}
