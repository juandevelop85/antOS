// syso - kernel x86_64
//
// Fase 2: la máquina deja de ejecutar una línea recta y empieza a reaccionar.
// GDT y TSS, tabla de interrupciones, el PIC remapeado, y los dos primeros
// manejadores de hardware: temporizador y teclado.

#![no_std]
#![no_main]
// La razón concreta por la que este proyecto usa nightly: sin esta convención
// de llamada habría que escribir a mano el prólogo y el epílogo de cada
// manejador de interrupción en ensamblador.
#![feature(abi_x86_interrupt)]

mod gdt;
mod interrupts;
mod port;
mod serial;
mod sync;

use bootloader_api::info::{FrameBufferInfo, MemoryRegionKind, PixelFormat};
use bootloader_api::{entry_point, BootInfo};
use core::fmt::Write;
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::SERIAL.lock().init();

    println!();
    println!("syso · kernel x86_64");
    println!("═══════════════════════");

    // El bootloader nos entrega esto y desaparece. Es todo lo que sabemos
    // de la máquina, así que conviene mirarlo antes de dar nada por hecho.
    report_memory(boot_info);
    report_framebuffer(boot_info);

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        paint_gradient(framebuffer.buffer_mut(), info);
    }

    println!();
    println!("interrupciones");

    // El orden importa: la IDT necesita el selector de código de NUESTRA GDT,
    // y el manejador de doble fallo necesita que el TSS ya esté cargado.
    gdt::init();
    println!("  gdt          cargada, con pila propia para el doble fallo");

    interrupts::init();
    println!("  idt          cargada, 8 vectores atendidos");

    // Prueba de que una excepción puede manejarse y CONTINUAR: `int3` salta
    // al manejador de breakpoint, que imprime y retorna aquí mismo.
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    println!("  breakpoint   manejado y ejecución reanudada");

    interrupts::init_pic();
    interrupts::enable();
    println!("  pic          remapeado a 32.. · temporizador y teclado activos");

    println!();
    println!("el kernel queda a la espera de interrupciones");
    halt_loop()
}

fn report_memory(boot_info: &BootInfo) {
    let mut usable = 0u64;
    let mut usable_regions = 0usize;

    for region in boot_info.memory_regions.iter() {
        if region.kind == MemoryRegionKind::Usable {
            usable += region.end - region.start;
            usable_regions += 1;
        }
    }

    println!();
    println!("memoria");
    println!(
        "  utilizable   {} MiB en {} regiones (de {} en total)",
        usable / (1024 * 1024),
        usable_regions,
        boot_info.memory_regions.len()
    );
    println!(
        "  kernel       {:#x} · {} KiB",
        boot_info.kernel_addr,
        boot_info.kernel_len / 1024
    );

    // Sin esto no podemos leer una tabla de páginas: conocemos su dirección
    // física, pero la CPU ya solo entiende direcciones virtuales. Que salga
    // "sin mapear" es lo que habrá que resolver en la Fase 3.
    match boot_info.physical_memory_offset.into_option() {
        Some(offset) => println!("  memoria física mapeada en {offset:#x}"),
        None => println!("  memoria física sin mapear (lo necesitará la Fase 3)"),
    }
}

fn report_framebuffer(boot_info: &BootInfo) {
    let Some(framebuffer) = boot_info.framebuffer.as_ref() else {
        println!();
        println!("framebuffer   no disponible");
        return;
    };
    let info = framebuffer.info();

    println!();
    println!("framebuffer");
    println!("  resolución   {}x{}", info.width, info.height);
    println!(
        "  formato      {:?} · {} bytes por píxel",
        info.pixel_format, info.bytes_per_pixel
    );
    // stride != width significa que cada fila tiene relleno al final. Es el
    // detalle que torcería la imagen si lo ignoráramos al pintar.
    println!(
        "  stride       {} píxeles ({} de relleno por fila)",
        info.stride,
        info.stride - info.width
    );
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

/// Duerme la CPU entre interrupciones.
///
/// En la Fase 1 esto era un final. Ahora ya no: `hlt` despierta con cada
/// interrupción, se atiende el manejador, y se vuelve a dormir. La máquina
/// está viva sin quemar un núcleo girando en vacío.
fn halt_loop() -> ! {
    loop {
        // SAFETY: hlt no toca memoria; solo detiene la CPU hasta una IRQ.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Lo que se ejecuta tras un panic. Ya no se limita a parar la máquina:
/// ahora dice qué pasó y dónde, que es la mitad del trabajo de depurar.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Sin cerrojo a propósito: si el panic ocurrió imprimiendo, el cerrojo
    // está tomado y esperarlo nos dejaría colgados justo cuando más falta
    // hace ver el mensaje.
    let mut serial = serial::emergency();

    let _ = writeln!(serial, "\n╔══════════════════════════════");
    let _ = writeln!(serial, "║ PANIC DEL KERNEL");
    if let Some(location) = info.location() {
        let _ = writeln!(
            serial,
            "║ en {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }
    let _ = writeln!(serial, "║ {}", info.message());
    let _ = writeln!(serial, "╚══════════════════════════════");

    halt_loop()
}
