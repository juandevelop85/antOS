/* Linker script for the AArch64 Limine boot path (T27.1).
 *
 * `linker.ld` places the kernel at 0x40080000 — a lower-half address that
 * works for direct-QEMU-boot (`-kernel`, no bootloader, MMU off at entry)
 * but that Limine rejects outright ("Lower half PHDRs are not allowed").
 *
 * The natural AArch64 "higher half" is anything TTBR1_EL1 covers —
 * 0xffff800000000000 and up, for the default 48-bit VA split — and that's
 * where this file originally placed the kernel. Limine's own source
 * disagrees: its lower-half check (`common/lib/elf.c`, `elf64_load`) uses a
 * single constant, `FIXED_HIGHER_HALF_OFFSET_64 = 0xffffffff80000000`,
 * shared verbatim by every architecture it supports — there is no
 * AArch64-specific threshold. 0xffffffff80000000 still falls inside
 * TTBR1_EL1's range for a 48-bit VA split (its top 16 bits are 0xffff), so
 * it works for both reasons at once: Limine's check and AArch64's own MMU.
 */
ENTRY(_start)

/* Limine loads each PT_LOAD segment with one fixed set of permissions and
 * refuses an ELF where two segments that need different permissions (R+X
 * text vs. R-only rodata vs. R+W data) land on the same page — `linker.ld`
 * never needed the `ALIGN(4096)` boundaries below because its segments are
 * loaded by hand (`elf::load_aarch64`) rather than by a bootloader that
 * enforces this.
 */
SECTIONS
{
    . = 0xffffffff80080000;

    .text : {
        *(.text._start)
        *(.text.vectors)
        *(.text .text.*)
    }

    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
    }

    /* The Limine request statics are plain immutable `static`s, so the
     * linker places them read-only regardless of section name — a
     * different permission class from `.data` (R+W), and one more page
     * boundary this script has to spell out by hand.
     */
    . = ALIGN(4096);
    .requests_start_marker : {
        *(.requests_start_marker)
    }

    .requests : {
        *(.requests)
    }

    .requests_end_marker : {
        *(.requests_end_marker)
    }

    . = ALIGN(4096);
    .bss : {
        *(.bss .bss.*)
        *(.bss.stack)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
