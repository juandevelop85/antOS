//! Acceso al espacio de E/S de x86.
//!
//! x86 tiene 65536 puertos en un espacio de direcciones **separado** del de
//! memoria, al que no se llega con punteros: solo con las instrucciones `in`
//! y `out`. No hay forma de expresar eso en Rust, de ahí el ensamblador.

/// # Safety
/// Escribe en un puerto de E/S: el efecto depende del hardware que haya ahí.
pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// # Safety
/// Leer un puerto puede tener efectos secundarios en el dispositivo.
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

/// Una escritura a un puerto que no hace nada, solo perder tiempo.
///
/// El PIC 8259 es de 1976 y necesita unos microsegundos entre comandos: una
/// CPU moderna le habla más rápido de lo que puede escuchar. El puerto 0x80
/// lo usaba el POST de la BIOS y escribir en él es inofensivo.
pub unsafe fn io_wait() {
    outb(0x80, 0);
}
