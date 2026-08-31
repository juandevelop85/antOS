//! GDT y TSS.
//!
//! En modo largo la segmentación está casi muerta: los segmentos ya no
//! trocean la memoria, eso lo hace la paginación. Pero la GDT no desaparece,
//! porque sigue siendo el único sitio donde se puede describir un **TSS**.
//!
//! ¿Y para qué queremos un TSS si en 64 bits ya no hay cambio de tarea por
//! hardware? Por una cosa: la **IST** (Interrupt Stack Table). Permite decirle
//! a la CPU «cuando salte ESTA excepción, cambia a esta otra pila». Y eso
//! importa porque:
//!
//! Si la pila del kernel se desborda, la CPU intenta apilar el marco de la
//! excepción de página... en la pila desbordada. Eso falla, lo que provoca un
//! doble fallo, que intenta apilar su marco en la misma pila rota, lo que
//! provoca un **triple fallo**: la CPU se rinde y reinicia la máquina. Sin
//! mensaje, sin rastro.
//!
//! Con una IST, el manejador de doble fallo aterriza en una pila limpia y
//! puede contarte lo que ha pasado. Es la diferencia entre depurar y adivinar.

use crate::sync::InitOnly;

/// Índice dentro de la IST. La tabla tiene 7 huecos; usamos el primero.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Selectores: el índice en la GDT multiplicado por 8, más el nivel de
/// privilegio (0, kernel) en los dos bits bajos.
pub const CODE_SELECTOR: u16 = 8; // índice 1
const DATA_SELECTOR: u16 = 16; // índice 2
const TSS_SELECTOR: u16 = 24; // índice 3

/// Segmento de código de 64 bits, anillo 0.
/// Presente, tipo código ejecutable, bit L (long mode) activo.
const CODE_SEGMENT: u64 = 0x00AF_9B00_0000_FFFF;
/// Segmento de datos. En modo largo la CPU casi lo ignora, pero algunas
/// instrucciones siguen exigiendo que SS sea un descriptor válido.
const DATA_SEGMENT: u64 = 0x00CF_9300_0000_FFFF;

const STACK_SIZE: usize = 4096 * 5;

/// Alineada a 16 porque el ABI de x86-64 lo exige en las llamadas.
#[repr(C, align(16))]
struct Stack([u8; STACK_SIZE]);

static DOUBLE_FAULT_STACK: InitOnly<Stack> = InitOnly::new(Stack([0; STACK_SIZE]));

/// El TSS de 64 bits: ya no guarda registros de una tarea, solo punteros de
/// pila. `packed(4)` porque el formato del hardware no está alineado a 8.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
struct Tss {
    reserved_0: u32,
    /// Pilas para cuando se entra al anillo 0 desde un anillo menos
    /// privilegiado. Las necesitaremos en la fase de espacio de usuario.
    privilege_stack_table: [u64; 3],
    reserved_1: u64,
    /// Las siete pilas de la IST.
    interrupt_stack_table: [u64; 7],
    reserved_2: u64,
    reserved_3: u16,
    iomap_base: u16,
}

static TSS: InitOnly<Tss> = InitOnly::new(Tss {
    reserved_0: 0,
    privilege_stack_table: [0; 3],
    reserved_1: 0,
    interrupt_stack_table: [0; 7],
    reserved_2: 0,
    reserved_3: 0,
    iomap_base: 0,
});

/// Nulo, código, datos, y el TSS — que ocupa DOS huecos, porque un descriptor
/// de sistema en modo largo mide 16 bytes en vez de 8.
static GDT: InitOnly<[u64; 5]> = InitOnly::new([0; 5]);

/// Lo que esperan `lgdt` y `lidt`: dos bytes de límite seguidos de ocho de
/// dirección. `packed(2)` para que no se cuele relleno entre los dos campos.
/// La IDT usa exactamente el mismo formato, de ahí que sea compartido.
#[repr(C, packed(2))]
pub(crate) struct DescriptorTablePointer {
    pub limit: u16,
    pub base: u64,
}

pub fn init() {
    // SAFETY: se llama una vez durante el arranque, antes de que existan
    // interrupciones o segundos núcleos.
    unsafe {
        let tss = TSS.get_mut();

        // La pila crece hacia abajo, así que a la CPU se le da el final.
        let stack = DOUBLE_FAULT_STACK.get() as *const Stack as u64;
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack + STACK_SIZE as u64;

        // Apuntar el mapa de permisos de E/S más allá del propio TSS es la
        // forma canónica de decir «no hay mapa»: todo puerto queda prohibido
        // para el espacio de usuario.
        tss.iomap_base = core::mem::size_of::<Tss>() as u16;

        let gdt = GDT.get_mut();
        gdt[0] = 0; // el descriptor nulo es obligatorio
        gdt[1] = CODE_SEGMENT;
        gdt[2] = DATA_SEGMENT;
        let (low, high) = tss_descriptor(tss as *const Tss as u64);
        gdt[3] = low;
        gdt[4] = high;

        load(gdt);
    }
}

/// Construye el descriptor de 16 bytes que describe al TSS.
///
/// El formato es un fósil: la dirección base viene troceada en tres campos no
/// contiguos porque el descriptor creció por parches desde el 80286.
fn tss_descriptor(base: u64) -> (u64, u64) {
    let limit = (core::mem::size_of::<Tss>() - 1) as u64;

    let low = limit & 0xFFFF
        | (base & 0x00FF_FFFF) << 16
        | 0x89 << 40                       // presente, tipo = TSS de 64 bits disponible
        | ((limit >> 16) & 0xF) << 48
        | ((base >> 24) & 0xFF) << 56;

    (low, base >> 32)
}

/// # Safety
/// `gdt` debe contener descriptores válidos y vivir para siempre: la CPU se
/// queda con un puntero a esta memoria.
unsafe fn load(gdt: &'static [u64; 5]) {
    let pointer = DescriptorTablePointer {
        limit: (core::mem::size_of_val(gdt) - 1) as u16,
        base: gdt.as_ptr() as u64,
    };

    unsafe {
        core::arch::asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));

        // Recargar CS. No existe `mov cs, reg`: el único modo de cambiar el
        // segmento de código es un salto o retorno lejano. Aquí se falsifica
        // un retorno — se apila el selector, se apila la dirección de vuelta,
        // y `retfq` salta a la instrucción siguiente ya con el CS nuevo.
        //
        // Sin esto, CS seguiría apuntando a la GDT del bootloader, que en
        // cualquier momento puede ser reutilizada como memoria libre.
        core::arch::asm!(
            "push {selector}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq", // `lretq` en sintaxis AT&T; aquí escribimos Intel
            "2:",
            selector = in(reg) CODE_SELECTOR as u64,
            tmp = lateout(reg) _,
            options(preserves_flags),
        );

        // Los segmentos de datos sí admiten un `mov` normal.
        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov ss, {0:x}",
            in(reg) DATA_SELECTOR,
            options(nostack, preserves_flags)
        );

        // Y finalmente cargar el registro de tarea, que es lo que activa la
        // IST. Hasta esta instrucción, el TSS es una estructura decorativa.
        core::arch::asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack, preserves_flags));
    }
}
