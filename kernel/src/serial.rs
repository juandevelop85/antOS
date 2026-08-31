//! Puerto serie 16550: nuestro primer instrumento.
//!
//! Hasta ahora estábamos ciegos. Si algo fallaba, la pantalla se quedaba
//! negra y no había forma de saber por qué. Esto lo arregla, y por eso va
//! antes que las interrupciones o la paginación: sin poder imprimir, depurar
//! cualquiera de las dos es a oscuras.
//!
//! El UART es el chip que convierte bytes en la señal serie. QEMU lo emula y
//! redirige a la terminal con `-serial stdio`, así que un `println!` aquí sale
//! en tu shell.
//!
//! ## Por qué esto necesita ensamblador
//!
//! El framebuffer de la Fase 0 era memoria: escribías un byte y aparecía un
//! píxel. El UART no. x86 tiene un **espacio de direcciones de E/S separado**,
//! con sus propios 65536 puertos, al que no se llega con punteros. Solo con
//! las instrucciones `in` y `out`. No hay forma de expresar eso en Rust.

use crate::sync::SpinLock;
use core::fmt::{self, Write};

/// COM1. Los cuatro puertos serie de un PC están en direcciones fijas desde
/// los años 80: 0x3F8, 0x2F8, 0x3E8, 0x2E8.
const COM1: u16 = 0x3F8;

// Registros, como desplazamiento sobre la base. Los nombres son los del chip.
const DATA: u16 = 0; // byte a enviar o recibir
const INT_ENABLE: u16 = 1; // qué interrupciones queremos
const FIFO_CTRL: u16 = 2; // control de las colas
const LINE_CTRL: u16 = 3; // formato: bits, paridad, parada
const MODEM_CTRL: u16 = 4; // líneas de control
const LINE_STATUS: u16 = 5; // ¿puedo escribir ya?

/// Bit 5 de LINE_STATUS: el registro de transmisión está vacío.
const TRANSMITTER_READY: u8 = 1 << 5;

/// Bit 7 de LINE_CTRL. Con él activo, los dos primeros registros dejan de ser
/// datos e interrupciones y pasan a ser el divisor de velocidad. Es un truco
/// de la época para meter más registros en menos direcciones.
const DLAB: u8 = 1 << 7;

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    /// # Safety
    /// `base` debe ser la dirección de un UART 16550 de verdad. Escribir en
    /// puertos al azar puede reconfigurar hardware arbitrario.
    pub const unsafe fn new(base: u16) -> Self {
        SerialPort { base }
    }

    pub fn init(&mut self) {
        unsafe {
            // Nada de interrupciones: en esta fase escribimos girando en
            // espera, que es lo más simple que funciona.
            outb(self.base + INT_ENABLE, 0x00);

            // Velocidad. El UART divide un reloj de 115200 Hz; divisor 3 son
            // 38400 baudios. A QEMU le da igual, pero el hardware real no.
            outb(self.base + LINE_CTRL, DLAB);
            outb(self.base + DATA, 0x03); // divisor, byte bajo
            outb(self.base + INT_ENABLE, 0x00); // divisor, byte alto

            // 8 bits de datos, sin paridad, 1 bit de parada — el "8N1" de
            // toda la vida. Escribir esto también apaga DLAB.
            outb(self.base + LINE_CTRL, 0x03);

            // Colas activadas y vaciadas, con umbral de 14 bytes.
            outb(self.base + FIFO_CTRL, 0xC7);

            // DTR y RTS activos: le decimos al otro extremo que estamos aquí.
            outb(self.base + MODEM_CTRL, 0x03);
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            // El UART transmite mucho más despacio que la CPU. Si escribimos
            // sin mirar, pisamos el byte anterior y se pierde. Hay que
            // esperar a que avise de que su registro está libre.
            while inb(self.base + LINE_STATUS) & TRANSMITTER_READY == 0 {
                core::hint::spin_loop();
            }
            outb(self.base + DATA, byte);
        }
    }
}

/// Implementar este único método nos da TODO el formateo de Rust —`{}`,
/// `{:#x}`, `{:?}`, ancho, relleno— sin heap y sin asignar nada. El
/// formateador va llamando a `write_str` con trozos ya resueltos.
impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // Una terminal espera retorno de carro antes del salto de línea;
            // Rust solo emite '\n'.
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// # Safety
/// Escribe en un puerto de E/S: el efecto depende del hardware que haya ahí.
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// # Safety
/// Leer un puerto puede tener efectos secundarios en el dispositivo.
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

/// SAFETY del `unsafe`: COM1 está en 0x3F8 en cualquier PC.
pub static SERIAL: SpinLock<SerialPort> = SpinLock::new(unsafe { SerialPort::new(COM1) });

/// Un puerto SIN cerrojo, solo para el manejador de panic.
///
/// Si el panic ocurre dentro de la sección crítica —por ejemplo, mientras se
/// está imprimiendo— el cerrojo ya está tomado. Un manejador que intentara
/// tomarlo giraría para siempre y nos quedaríamos sin ver el mensaje, que es
/// justo cuando más falta hace. Saltarse el cerrojo puede entrelazar la
/// salida, pero eso es infinitamente mejor que colgarse.
pub fn emergency() -> SerialPort {
    // SAFETY: COM1 ya fue inicializado en el arranque. Y si la máquina está
    // entrando en panic, la carrera de datos es el menor de los problemas.
    unsafe { SerialPort::new(COM1) }
}

/// Detalle de implementación de las macros. No llamar directamente.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // `.unwrap()` es correcto aquí: nuestro write_str no puede fallar.
    SERIAL.lock().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
