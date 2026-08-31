// syso - kernel x86_64
//
// Fase 0: arrancar y demostrar que nuestro código se ejecuta de verdad
// pintando el framebuffer que nos entrega el bootloader.

// Sin biblioteca estándar: `std` asume que ya existe un SO debajo (hilos,
// ficheros, heap, sockets). Aquí ese SO somos nosotros, así que solo
// disponemos de `core`: tipos primitivos, Option/Result, iteradores...
#![no_std]
// Sin `main`: el `main` de Rust lo llama un runtime de arranque (crt0) que
// aquí no existe. Nuestro punto de entrada lo define la macro entry_point!.
#![no_main]

use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;

// Declara el símbolo que el bootloader buscará y saltará a él, y además
// comprueba en tiempo de compilación que la firma es la correcta:
//     fn(&'static mut BootInfo) -> !
// El `-> !` no es decorativo: no hay nadie a quien retornar. Si esta función
// hiciera `ret`, la CPU saltaría a una dirección basura.
entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // El framebuffer es Optional porque el firmware podría no dárnoslo.
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        paint_gradient(framebuffer.buffer_mut(), info);
    }

    halt_loop()
}

/// Pinta un degradado sobre el framebuffer.
///
/// El framebuffer es memoria plana mapeada al dispositivo de vídeo: escribir
/// un byte ahí cambia un píxel en pantalla. No hay driver ni llamada al
/// sistema de por medio.
fn paint_gradient(buffer: &mut [u8], info: FrameBufferInfo) {
    for y in 0..info.height {
        for x in 0..info.width {
            // `stride` son los píxeles por fila EN MEMORIA, que puede ser
            // mayor que `width` por alineación. Usar width aquí sería un bug
            // clásico: la imagen saldría inclinada.
            let pixel_offset = (y * info.stride + x) * info.bytes_per_pixel;

            let r = (x * 255 / info.width) as u8;
            let g = (y * 255 / info.height) as u8;
            let b = 0x80;

            // El orden de los canales depende del firmware, no es fijo.
            let (c0, c1, c2) = match info.pixel_format {
                PixelFormat::Rgb => (r, g, b),
                PixelFormat::Bgr => (b, g, r),
                // Escala de grises: luminancia aproximada.
                PixelFormat::U8 => {
                    let gray = ((r as u16 + g as u16 + b as u16) / 3) as u8;
                    (gray, gray, gray)
                }
                // Formato desconocido: mejor no escribir basura en la pantalla.
                _ => return,
            };

            buffer[pixel_offset] = c0;
            buffer[pixel_offset + 1] = c1;
            buffer[pixel_offset + 2] = c2;
        }
    }
}

/// Detiene la CPU de forma permanente.
///
/// `hlt` la duerme hasta la siguiente interrupción; el bucle la vuelve a
/// dormir si alguna la despierta. Un `loop {}` a secas también "funcionaría",
/// pero dejaría un núcleo al 100% girando en vacío.
fn halt_loop() -> ! {
    loop {
        // SAFETY: hlt no toca memoria; solo detiene la CPU hasta una IRQ.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Rust exige este símbolo cuando no hay `std`: es lo que se ejecuta tras un
/// panic. Sin SO no hay a quién reportar ni proceso que matar, así que de
/// momento paramos la máquina. En la Fase 1 lo haremos imprimir el mensaje.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt_loop()
}
