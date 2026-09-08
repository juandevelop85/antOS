//! IDT, exceptions, and hardware interrupts (T23.1 — APIC migration).
//!
//! The kernel reacts to hardware events through the Interrupt Descriptor Table
//! (IDT).  Vectors 0-31 are reserved by the CPU for exceptions; vectors 32+
//! are for hardware.
//!
//! ## PIC → APIC migration
//!
//! Until T23.1 the PIC 8259 drove interrupts.  Now the Local APIC provides
//! the timer, and the PIC is fully masked.  Keyboard interrupts still arrive
//! through the PIC's IRQ1 path during boot but will migrate to the I/O APIC
//! in a future ticket.
//!
//! ## `extern "x86-interrupt"`
//!
//! An interrupt can fire between any two instructions, so the handler must
//! save and restore **all** registers and exit with `iretq`.  This calling
//! convention asks the compiler to emit that prologue/epilogue.

use crate::gdt::{self, DescriptorTablePointer};
use crate::port::{inb, outb};
use crate::println;
use crate::sync::InitOnly;

/// The frame pushed by the CPU before jumping to a handler.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// A single IDT entry: 16 bytes with the handler address split into three
/// non-contiguous fields.
#[repr(C)]
#[derive(Clone, Copy)]
struct Entry {
    offset_low: u16,
    selector: u16,
    /// Bits 0-2: IST index (0 = use current stack).
    ist: u8,
    /// 0x8E = present, DPL 0, interrupt gate.
    flags: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl Entry {
    const fn missing() -> Self {
        Entry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }
}

static IDT: InitOnly<[Entry; 256]> = InitOnly::new([Entry::missing(); 256]);

/// `ist` is 1-based: 0 means "don't switch stacks".
fn set(idt: &mut [Entry; 256], vector: usize, handler: usize, ist: u8) {
    let addr = handler as u64;
    idt[vector] = Entry {
        offset_low: addr as u16,
        selector: gdt::CODE_SELECTOR,
        ist,
        flags: 0x8E,
        offset_mid: (addr >> 16) as u16,
        offset_high: (addr >> 32) as u32,
        reserved: 0,
    };
}

// ─────────────────────────────────────────────── Vectors

/// APIC timer and legacy PIC timer share this vector.
pub const TIMER_VECTOR: u8 = 32;
/// Keyboard interrupt (IRQ1 via legacy PIC or I/O APIC in the future).
pub const KEYBOARD_VECTOR: u8 = 33;
/// PS/2 mouse interrupt (IRQ12 → PIC2 line 4 → remapped base 40 + 4).
pub const MOUSE_VECTOR: u8 = 44;
/// LAPIC spurious interrupt — required by the APIC specification.
pub const SPURIOUS_VECTOR: u8 = 0xFF;

pub fn init() {
    // SAFETY: called once during boot with interrupts still disabled.
    unsafe {
        let idt = IDT.get_mut();

        // CPU exceptions (vectors 0-31).
        set(idt, 0, divide_error as *const () as usize, 0);
        set(idt, 3, breakpoint as *const () as usize, 0);
        set(idt, 6, invalid_opcode as *const () as usize, 0);
        set(idt, 8, double_fault as *const () as usize, gdt::DOUBLE_FAULT_IST_INDEX as u8 + 1);
        set(idt, 13, general_protection as *const () as usize, 0);
        set(idt, 14, page_fault as *const () as usize, 0);

        // Hardware interrupts.
        set(idt, TIMER_VECTOR as usize, timer_interrupt_entry as *const () as usize, 0);
        set(idt, KEYBOARD_VECTOR as usize, keyboard as *const () as usize, 0);
        set(idt, MOUSE_VECTOR as usize, mouse as *const () as usize, 0);
        set(idt, SPURIOUS_VECTOR as usize, spurious as *const () as usize, 0);

        let pointer = DescriptorTablePointer {
            limit: (core::mem::size_of_val(idt) - 1) as u16,
            base: idt.as_ptr() as u64,
        };
        core::arch::asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
}

/// Enables maskable interrupts (STI).
pub fn enable() {
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
}

/// Disables maskable interrupts (CLI).
pub fn disable() {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
}

/// Triggers a software breakpoint for debugging (`int3`).
pub fn trigger_breakpoint() {
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
}

// ─────────────────────────────────────────────── Legacy PIC 8259

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const END_OF_INTERRUPT: u8 = 0x20;

/// Initializes and remaps the PIC 8259.
///
/// **Legacy path** — kept for keyboard IRQ during early boot before I/O APIC
/// migration.  The APIC timer replaces the PIC timer.
pub fn init_pic() {
    unsafe {
        // ICW1: start initialization, expect ICW4.
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // ICW2: base vector offset.
        outb(PIC1_DATA, TIMER_VECTOR);
        io_wait();
        outb(PIC2_DATA, TIMER_VECTOR + 8);
        io_wait();

        // ICW3: cascading (slave on line 2).
        outb(PIC1_DATA, 1 << 2);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();

        // ICW4: 8086 mode.
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // Mask: allow keyboard (IRQ1), the cascade line (IRQ2) and — via the
        // slave — the PS/2 mouse (IRQ12). Timer (IRQ0) is handled by the LAPIC.
        outb(PIC1_DATA, 0b1111_1001); // IRQ1 (keyboard) + IRQ2 (cascade) unmasked
        outb(PIC2_DATA, 0b1110_1111); // IRQ12 (mouse) unmasked
    }
}

/// Sends End-of-Interrupt to the PIC.
///
/// # Safety
/// Must only be called from the corresponding interrupt handler.
unsafe fn pic_end_of_interrupt(vector: u8) {
    unsafe {
        if vector >= TIMER_VECTOR + 8 {
            outb(PIC2_COMMAND, END_OF_INTERRUPT);
        }
        outb(PIC1_COMMAND, END_OF_INTERRUPT);
    }
}

fn io_wait() {
    unsafe { outb(0x80, 0) };
}

// ─────────────────────────────────────────────── Exceptions

fn from_user(frame: &InterruptStackFrame) -> bool {
    frame.code_segment & 3 == 3
}

fn fault(frame: &InterruptStackFrame, description: core::fmt::Arguments) -> ! {
    if from_user(frame) {
        println!();
        println!("  ✋ userspace fault: {description}");
        println!("     killed by kernel · system continues");
        crate::userspace::return_to_kernel(0xdead);
    }
    panic!("{description}");
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    println!("  exception · breakpoint at {:#x}", frame.instruction_pointer);
}

extern "x86-interrupt" fn divide_error(frame: InterruptStackFrame) {
    fault(&frame, format_args!("divide by zero at {:#x}", frame.instruction_pointer));
}

extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    fault(&frame, format_args!("invalid opcode at {:#x}", frame.instruction_pointer));
}

extern "x86-interrupt" fn general_protection(frame: InterruptStackFrame, error_code: u64) {
    fault(
        &frame,
        format_args!(
            "general protection fault at {:#x} (error {error_code:#x})",
            frame.instruction_pointer
        ),
    );
}

extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, error_code: u64) {
    let address: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) address, options(nomem, nostack)) };

    fault(
        &frame,
        format_args!(
            "tried to access {address:#x} from {:#x} · MMU stopped it (code {error_code:#b})",
            frame.instruction_pointer
        ),
    );
}

extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, _error_code: u64) -> ! {
    panic!("DOUBLE FAULT at {:#x}", frame.instruction_pointer);
}

// ─────────────────────────────────────────────── Hardware interrupts

/// Naked assembly entry point for the periodic timer interrupt (vector 32).
///
/// Saves all general-purpose registers to form a complete `CpuContext` on the stack,
/// passes a pointer to the context to `timer_interrupt_handler` for potential
/// task preemption / context switching, and then executes `iretq` to resume the
/// chosen thread.
#[unsafe(naked)]
pub unsafe extern "C" fn timer_interrupt_entry() {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rbx",
        "push rbp",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rsp",
        "call {handler}",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        handler = sym timer_interrupt_handler,
    );
}

extern "C" fn timer_interrupt_handler(ctx: &mut crate::task::pcb::CpuContext) {
    // 1. Advance async timer ticks and notify waiting async futures
    crate::task::timer::tick();

    // 1b. Service the USB host controller (HID reports) — the xHCI stack is
    // polled, not MSI-driven, on x86_64 (T28.9).
    crate::drivers::usb::poll();

    // 2. Scheduler preemption hook: evaluates quantum and switches context if needed
    crate::task::scheduler::on_timer_tick(ctx);

    // 3. Send End-of-Interrupt to LAPIC or fallback PIC
    if crate::arch::x86_64::apic::is_initialized() {
        crate::arch::x86_64::apic::eoi();
    } else {
        unsafe { pic_end_of_interrupt(TIMER_VECTOR) };
    }
}

extern "x86-interrupt" fn keyboard(_frame: InterruptStackFrame) {
    let scancode = unsafe { inb(0x60) };
    crate::task::keyboard::add_scancode(scancode);

    // Keyboard still comes through PIC until I/O APIC migration.
    unsafe { pic_end_of_interrupt(KEYBOARD_VECTOR) };
}

/// PS/2 mouse interrupt (IRQ12, through the slave PIC).
extern "x86-interrupt" fn mouse(_frame: InterruptStackFrame) {
    let byte = unsafe { inb(0x60) };
    crate::drivers::ps2::handle_byte(byte);
    // IRQ12 lives on the slave PIC: EOI both.
    unsafe { pic_end_of_interrupt(MOUSE_VECTOR) };
}

/// Spurious interrupt handler — required by the LAPIC specification.
/// Must NOT send EOI (Intel SDM Vol. 3A, §10.9).
extern "x86-interrupt" fn spurious(_frame: InterruptStackFrame) {
    // Intentionally empty: spurious interrupts are a normal LAPIC artifact.
}
