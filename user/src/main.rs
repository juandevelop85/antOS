//! Un programa de espacio de usuario para syso.
//!
//! Corre en el anillo 3. No puede leer la memoria del kernel, ni hablar con
//! el hardware, ni ejecutar instrucciones privilegiadas. Para pedir cualquier
//! cosa tiene que hacer una llamada al sistema.
//!
//! Fíjate en que NO enlaza con el kernel: es un ELF aparte. Su único punto de
//! contacto con syso son los números de las llamadas y la instrucción
//! `syscall`. Esa es exactamente la frontera que define un sistema operativo.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

const SYS_WRITE: u64 = 0;
const SYS_EXIT: u64 = 1;

/// La convención de llamada de syso:
///   RAX = número de llamada · RDI, RSI, RDX = argumentos · RAX = resultado
///
/// RCX y R11 no se pueden usar para argumentos: la instrucción `syscall` los
/// pisa para guardar la dirección de retorno y las banderas.
unsafe fn syscall(number: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rcx") _,
            lateout("r11") _,
            clobber_abi("sysv64"),
        );
    }
    result
}

fn write(message: &str) {
    unsafe { syscall(SYS_WRITE, message.as_ptr() as u64, message.len() as u64, 0) };
}

fn exit(code: u64) -> ! {
    unsafe { syscall(SYS_EXIT, code, 0, 0) };
    // El kernel no devuelve el control, pero el compilador no lo sabe.
    loop {}
}

/// El kernel pasa el modo en RDI al saltar aquí.
#[no_mangle]
pub extern "C" fn _start(mode: u64) -> ! {
    write("hola desde el anillo 3, escribiendo por una llamada al sistema\n");

    if mode == 1 {
        write("voy a intentar leer memoria del kernel; deberia matarme\n");
        // Donde el bootloader cargó el kernel. Desde el anillo 3 esa página
        // no tiene el bit de usuario, así que la MMU debe impedirlo — sin que
        // el kernel tenga que comprobar nada.
        //
        // Ojo con el número: en x86-64 solo son canónicas las direcciones
        // hasta 0x7FFF_FFFF_FFFF. Un cero de más y no obtienes un fallo de
        // página sino uno de protección general, que es otra excepción.
        let kernel_address = 0x100_0000_0000u64 as *const u64;
        let stolen = unsafe { core::ptr::read_volatile(kernel_address) };

        // Si esto llega a ejecutarse, la protección no funciona.
        write("LA PROTECCION HA FALLADO: he podido leer el kernel\n");
        exit(stolen);
    }

    write("termino limpiamente con exit(0)\n");
    exit(0)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1)
}
