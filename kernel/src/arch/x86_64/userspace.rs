//! Anillo 3 y llamadas al sistema.
//!
//! Hasta aquí todo el código corría en anillo 0, con permiso para hacer
//! cualquier cosa. Un programa de usuario corre en **anillo 3**: no puede
//! leer la memoria del kernel, ni hablar con puertos de E/S, ni ejecutar
//! instrucciones privilegiadas. Para pedir algo tiene que cruzar la frontera
//! por el único sitio que le dejamos abierto: la instrucción `syscall`.
//!
//! ## Por qué `syscall` y no una interrupción
//!
//! Se puede entrar al kernel con `int 0x80`, y así se hacía. Pero una
//! interrupción consulta la IDT, cambia de pila usando el TSS y apila cinco
//! valores: cientos de ciclos. `syscall` no hace nada de eso — guarda el RIP
//! de retorno en RCX y las banderas en R11, carga CS y SS desde un registro
//! de configuración, y salta. **No cambia de pila**: eso lo tiene que hacer
//! el kernel a mano, y es lo primero que hace nuestro punto de entrada.

use crate::gdt;
use crate::memory::{FrameAllocator, Mapper, PAGE_SIZE, PRESENT, USER, WRITABLE};
use crate::{print, println};
use core::sync::atomic::{AtomicU64, Ordering};

// Registros de configuración del modelo (MSR). No son registros normales: se
// leen y escriben con `rdmsr`/`wrmsr` indicando su número.
const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;

const SYS_WRITE: u64 = 0;
const SYS_EXIT: u64 = 1;

/// Dónde empieza la pila del programa de usuario, y cuántas páginas ocupa.
const USER_STACK_TOP: u64 = 0x7000_0000;
const USER_STACK_PAGES: u64 = 4;

/// Pila del kernel a la que salta el punto de entrada de `syscall`.
static SYSCALL_STACK_TOP: AtomicU64 = AtomicU64::new(0);
/// Dónde se guarda la pila del usuario mientras corre la llamada.
static USER_RSP: AtomicU64 = AtomicU64::new(0);

// Estado del kernel para poder volver cuando el programa muera. Es lo mismo
// que hace un planificador de verdad al devolver el control: guardar dónde
// estabas antes de ceder la CPU.
static KERNEL_RSP: AtomicU64 = AtomicU64::new(0);
static KERNEL_RIP: AtomicU64 = AtomicU64::new(0);
static EXIT_CODE: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    SYSCALL_STACK_TOP.store(gdt::syscall_stack_top(), Ordering::Relaxed);

    // SAFETY: los MSR son los documentados por Intel y los valores derivan de
    // nuestra propia GDT.
    unsafe {
        // Bit 0 de EFER (SCE): sin él, `syscall` es una instrucción inválida.
        write_msr(IA32_EFER, read_msr(IA32_EFER) | 1);

        // STAR guarda los selectores: los bits 47:32 los usa `syscall` para
        // entrar al kernel, y los 63:48 los usa `sysret` para volver.
        let star = ((gdt::SYSRET_BASE as u64) << 48) | ((gdt::CODE_SELECTOR as u64) << 32);
        write_msr(IA32_STAR, star);

        // A dónde salta `syscall`.
        write_msr(IA32_LSTAR, syscall_entry as *const () as u64);

        // Qué banderas se apagan al entrar. IF es la importante: sin apagarla
        // podría saltar una interrupción con RSP apuntando todavía a la pila
        // del usuario, y el kernel correría sobre memoria que el usuario
        // controla. DF se apaga porque el ABI lo da por hecho.
        write_msr(IA32_FMASK, (1 << 9) | (1 << 10));
    }
}

/// Reserva y mapea la pila del programa de usuario.
///
/// # Safety
/// Solo una vez, y el rango no debe pisar nada del kernel.
pub unsafe fn map_user_stack(
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    for page in 0..USER_STACK_PAGES {
        let address = USER_STACK_TOP - (page + 1) * PAGE_SIZE;
        let frame = allocator.allocate().ok_or("sin marcos para la pila")?;
        unsafe { mapper.map(address, frame, PRESENT | WRITABLE | USER, allocator)? };
    }
    Ok(USER_STACK_TOP)
}

/// Salta al anillo 3 y no vuelve hasta que el programa muera.
///
/// # Safety
/// `entry` y `stack_top` deben apuntar a páginas mapeadas con el bit de
/// usuario. El programa recibe control total de la CPU en anillo 3.
pub unsafe fn enter(entry: u64, stack_top: u64, mode: u64) -> u64 {
    unsafe {
        core::arch::asm!(
            // Los registros que el ABI obliga a preservar se guardan a mano:
            // el programa de usuario puede dejarlos como quiera.
            "push rbp",
            "push rbx",
            "push r12",
            "push r13",
            "push r14",
            "push r15",

            // Apuntar dónde volver. El programa no regresa con un `ret`:
            // muere, y el kernel se restaura desde aquí.
            "lea rax, [rip + 2f]",
            "mov [rip + {kernel_rip}], rax",
            "mov [rip + {kernel_rsp}], rsp",

            "mov rsp, {user_stack}",
            // `sysret` salta a RCX con las banderas de R11, en anillo 3.
            "sysretq",

            // Aquí aterriza `return_to_kernel`.
            "2:",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbx",
            "pop rbp",

            kernel_rip = sym KERNEL_RIP,
            kernel_rsp = sym KERNEL_RSP,
            user_stack = in(reg) stack_top,

            // RCX y R11 no son argumentos: `sysret` los interpreta como la
            // dirección de destino y las banderas. 0x202 deja IF activo, para
            // que el temporizador siga corriendo durante el programa.
            inlateout("rcx") entry => _,
            inlateout("r11") 0x202u64 => _,
            // Primer argumento de la función `_start` del programa.
            inlateout("rdi") mode => _,

            lateout("rax") _,
            lateout("rdx") _,
            lateout("rsi") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
        );
    }
    EXIT_CODE.load(Ordering::Relaxed)
}

/// Abandona el programa de usuario y devuelve el control al kernel.
///
/// Es lo que un SO llama «matar un proceso»: se descarta su estado sin más
/// ceremonia y se restaura el del kernel.
pub fn return_to_kernel(code: u64) -> ! {
    EXIT_CODE.store(code, Ordering::Relaxed);
    // SAFETY: KERNEL_RSP y KERNEL_RIP los dejó `enter` antes de saltar.
    unsafe {
        core::arch::asm!(
            "mov rsp, [rip + {kernel_rsp}]",
            "jmp qword ptr [rip + {kernel_rip}]",
            kernel_rsp = sym KERNEL_RSP,
            kernel_rip = sym KERNEL_RIP,
            options(noreturn)
        )
    }
}

/// El punto de entrada de `syscall`, en ensamblador puro.
///
/// Tiene que ser `naked` porque lo primero que hace —cambiar de pila— es
/// incompatible con cualquier prólogo que el compilador pudiera generar: al
/// entrar aquí, RSP todavía apunta a la pila del usuario.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Guardar la pila del usuario y cambiar a la del kernel. Hasta que
        // esto termina, el kernel está corriendo sobre memoria que el usuario
        // controla — es la ventana más delicada de todo el sistema.
        "mov [rip + {user_rsp}], rsp",
        "mov rsp, [rip + {syscall_stack}]",

        // RCX y R11 traen el retorno; `sysretq` los necesita intactos.
        "push rcx",
        "push r11",

        // Traducir nuestra convención (RAX = número, RDI/RSI/RDX = argumentos)
        // a la de C, que espera el primer argumento en RDI. Se mueve de
        // derecha a izquierda para no pisar lo que aún no se ha leído.
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {handler}",
        // El resultado vuelve en RAX, que es donde el usuario lo espera.

        "pop r11",
        "pop rcx",
        "mov rsp, [rip + {user_rsp}]",
        "sysretq",

        user_rsp = sym USER_RSP,
        syscall_stack = sym SYSCALL_STACK_TOP,
        handler = sym handle_syscall,
    )
}

extern "C" fn handle_syscall(number: u64, arg1: u64, arg2: u64, _arg3: u64) -> u64 {
    match number {
        SYS_WRITE => sys_write(arg1, arg2),
        SYS_EXIT => return_to_kernel(arg1),
        _ => {
            println!("  llamada al sistema desconocida: {number}");
            u64::MAX
        }
    }
}

/// El límite del espacio de usuario en x86-64: las direcciones canónicas
/// altas son del kernel.
const USER_LIMIT: u64 = 0x0000_8000_0000_0000;
const MAX_WRITE: u64 = 4096;

fn sys_write(pointer: u64, length: u64) -> u64 {
    // Todo puntero que venga del usuario es hostil hasta que se demuestre lo
    // contrario. Sin esta comprobación, el programa podría pedirle al kernel
    // que imprimiera memoria del kernel — el propio kernel sería su lector.
    if pointer == 0 || length > MAX_WRITE {
        return u64::MAX;
    }
    let Some(end) = pointer.checked_add(length) else {
        return u64::MAX;
    };
    if end > USER_LIMIT {
        return u64::MAX;
    }

    // LIMITACIÓN: se comprueba el RANGO, no que las páginas estén mapeadas.
    // Un puntero a una página ausente provocaría un fallo dentro del kernel.
    // Lo correcto es una copia que sepa fallar; queda para la fase de
    // llamadas al sistema de verdad.
    let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, length as usize) };

    match core::str::from_utf8(bytes) {
        Ok(text) => {
            print!("     [usuario] {text}");
            length
        }
        Err(_) => u64::MAX,
    }
}

/// # Safety
/// Leer un MSR inexistente provoca un fallo de protección general.
unsafe fn read_msr(msr: u32) -> u64 {
    let (high, low): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

/// # Safety
/// Escribir un MSR cambia el comportamiento de la CPU. Un valor inválido
/// puede tumbar la máquina.
unsafe fn write_msr(msr: u32, value: u64) {
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        );
    }
}
