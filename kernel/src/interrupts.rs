//! IDT, excepciones y las dos primeras interrupciones de hardware.
//!
//! Hasta ahora el kernel ejecutaba una línea recta. Aquí empieza a
//! **reaccionar**: la CPU puede interrumpir lo que estuviera haciendo, saltar
//! a una función nuestra, y volver como si nada.
//!
//! ## La IDT
//!
//! Una tabla de 256 entradas indexada por número de vector. Los vectores 0-31
//! los reserva la CPU para sus propias excepciones (división por cero, fallo
//! de página, doble fallo...); del 32 en adelante son para el hardware.
//!
//! ## Por qué `extern "x86-interrupt"`
//!
//! Una interrupción no es una llamada: puede saltar entre dos instrucciones
//! cualesquiera, así que el manejador tiene que devolver **todos** los
//! registros exactamente como estaban, y salir con `iretq` en vez de `ret`.
//! Escribir eso a mano exige ensamblador. Esta convención de llamada le pide
//! al compilador que lo genere, y es la razón concreta por la que este
//! proyecto usa nightly desde la Fase 0.

use crate::gdt::{self, DescriptorTablePointer};
use crate::port::{inb, io_wait, outb};
use crate::println;
use crate::sync::InitOnly;

/// El marco que la CPU apila antes de saltar al manejador. El orden es del
/// hardware, no nuestro.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// Una entrada de la IDT: 16 bytes con la dirección del manejador troceada en
/// tres campos no contiguos, otro fósil de la evolución de x86.
#[repr(C)]
#[derive(Clone, Copy)]
struct Entry {
    offset_low: u16,
    selector: u16,
    /// Bits 0-2: índice en la IST (0 = usar la pila actual).
    ist: u8,
    /// 0x8E = presente, DPL 0, puerta de interrupción.
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

/// `ist` es 1-based: 0 significa «no cambies de pila», así que la primera
/// entrada de la IST se pide con un 1.
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

pub fn init() {
    // SAFETY: se llama una vez durante el arranque, con las interrupciones
    // todavía apagadas.
    unsafe {
        let idt = IDT.get_mut();

        set(idt, 0, divide_error as *const () as usize, 0);
        set(idt, 3, breakpoint as *const () as usize, 0);
        set(idt, 6, invalid_opcode as *const () as usize, 0);
        // El único que corre en su propia pila: si llegamos aquí, la pila
        // normal puede estar destrozada.
        set(idt, 8, double_fault as *const () as usize, gdt::DOUBLE_FAULT_IST_INDEX as u8 + 1);
        set(idt, 13, general_protection as *const () as usize, 0);
        set(idt, 14, page_fault as *const () as usize, 0);
        set(idt, TIMER_VECTOR as usize, timer as *const () as usize, 0);
        set(idt, KEYBOARD_VECTOR as usize, keyboard as *const () as usize, 0);

        let pointer = DescriptorTablePointer {
            limit: (core::mem::size_of_val(idt) - 1) as u16,
            base: idt.as_ptr() as u64,
        };
        core::arch::asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
}

/// A partir de aquí la CPU puede interrumpirnos en cualquier instrucción.
pub fn enable() {
    // SAFETY: la IDT ya está cargada; sin ella, la primera interrupción
    // provocaría un triple fallo.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
}

// ------------------------------------------------------------- excepciones

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    // Este manejador RETORNA, y ahí está la gracia: la ejecución sigue en la
    // instrucción siguiente como si nada hubiera pasado. Es la base de
    // cualquier depurador.
    println!("  excepción · breakpoint en {:#x}", frame.instruction_pointer);
}

extern "x86-interrupt" fn divide_error(frame: InterruptStackFrame) {
    panic!("división por cero en {:#x}", frame.instruction_pointer);
}

extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    panic!("instrucción inválida en {:#x}", frame.instruction_pointer);
}

extern "x86-interrupt" fn general_protection(frame: InterruptStackFrame, error_code: u64) {
    panic!(
        "fallo de protección general en {:#x} (código {error_code:#x})",
        frame.instruction_pointer
    );
}

extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, error_code: u64) {
    // CR2 guarda la dirección que se intentó acceder. Es el dato más útil de
    // toda la excepción, y solo está ahí.
    let address: u64;
    // SAFETY: leer CR2 no tiene efectos secundarios.
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) address, options(nomem, nostack)) };

    panic!(
        "fallo de página al acceder a {address:#x} desde {:#x} (código {error_code:#b})",
        frame.instruction_pointer
    );
}

/// Un doble fallo es «falló el manejo de un fallo». Si este manejador también
/// fallara vendría el triple fallo, que no es una excepción sino un reinicio
/// de la máquina — sin mensaje y sin rastro. Por eso corre en su propia pila.
extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, _error_code: u64) -> ! {
    panic!("DOBLE FALLO en {:#x}", frame.instruction_pointer);
}

// ---------------------------------------------------------- PIC 8259 y IRQs

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const END_OF_INTERRUPT: u8 = 0x20;

/// Las IRQ del PIC llegan por defecto a los vectores 8-15, que la CPU ya usa
/// para sus excepciones — un doble fallo y una IRQ del disco serían el mismo
/// número. Hay que remapearlas por encima de 31.
pub const TIMER_VECTOR: u8 = 32;
pub const KEYBOARD_VECTOR: u8 = 33;

pub fn init_pic() {
    // SAFETY: la secuencia de inicialización del 8259 está fijada por su hoja
    // de datos; los puertos son los de un PC compatible.
    unsafe {
        // Empieza la inicialización (ICW1), avisando de que habrá ICW4.
        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        // ICW2: a partir de qué vector emite cada uno.
        outb(PIC1_DATA, TIMER_VECTOR);
        io_wait();
        outb(PIC2_DATA, TIMER_VECTOR + 8);
        io_wait();

        // ICW3: cómo están encadenados. El esclavo cuelga de la línea 2 del
        // maestro — un apaño de 1981 para pasar de 8 interrupciones a 15.
        outb(PIC1_DATA, 1 << 2);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();

        // ICW4: modo 8086.
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        // Máscaras: un bit a 1 silencia esa línea. Solo dejamos pasar el
        // temporizador (IRQ0) y el teclado (IRQ1); lo demás aún no sabemos
        // atenderlo, y una IRQ sin manejador es un fallo de protección.
        outb(PIC1_DATA, 0b1111_1100);
        outb(PIC2_DATA, 0b1111_1111);
    }
}

/// El PIC no vuelve a emitir esa línea hasta que se le confirma. Olvidarlo
/// es el bug clásico: todo funciona una vez y luego el silencio.
///
/// # Safety
/// Solo debe llamarse desde el manejador de la interrupción correspondiente.
unsafe fn end_of_interrupt(vector: u8) {
    unsafe {
        // Las líneas del esclavo hay que confirmárselas a los dos.
        if vector >= TIMER_VECTOR + 8 {
            outb(PIC2_COMMAND, END_OF_INTERRUPT);
        }
        outb(PIC1_COMMAND, END_OF_INTERRUPT);
    }
}

// Desde la Fase 4 los manejadores no trabajan: solo apuntan el dato y
// despiertan a la tarea que lo esperaba. Mientras un manejador corre, el
// resto del sistema está parado, así que cuanto menos haga, mejor.

extern "x86-interrupt" fn timer(_frame: InterruptStackFrame) {
    crate::task::timer::tick();

    // SAFETY: estamos dentro del manejador de esta misma interrupción.
    unsafe { end_of_interrupt(TIMER_VECTOR) };
}

extern "x86-interrupt" fn keyboard(_frame: InterruptStackFrame) {
    // SAFETY: 0x60 es el puerto de datos del controlador de teclado. Hay que
    // leerlo SIEMPRE: si no se vacía, el controlador no manda más scancodes.
    let scancode = unsafe { inb(0x60) };
    crate::task::keyboard::add_scancode(scancode);

    // SAFETY: estamos dentro del manejador de esta misma interrupción.
    unsafe { end_of_interrupt(KEYBOARD_VECTOR) };
}
