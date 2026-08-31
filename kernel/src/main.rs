// syso - kernel x86_64
//
// Fase 5: espacio de usuario. Anillo 3, llamadas al sistema con syscall/sysret
// y un ELF cargado en tiempo de ejecución — código que NO PUEDE tocar el
// kernel, y la MMU encargándose de que así sea.

#![no_std]
#![no_main]
// La razón concreta por la que este proyecto usa nightly: sin esta convención
// de llamada habría que escribir a mano el prólogo y el epílogo de cada
// manejador de interrupción en ensamblador.
#![feature(abi_x86_interrupt)]

// El crate `alloc` trae Box, Vec, String y compañía. No forma parte de core,
// pero tampoco necesita sistema operativo: solo un #[global_allocator].
extern crate alloc;

mod allocator;
mod elf;
mod gdt;
mod interrupts;
mod memory;
mod port;
mod serial;
mod sync;
mod task;
mod userspace;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::info::{FrameBufferInfo, MemoryRegionKind, MemoryRegions, PixelFormat};
use bootloader_api::{entry_point, BootInfo};
use task::Task;
use core::fmt::Write;
use core::panic::PanicInfo;

/// Le pide al bootloader que mapee TODA la memoria física en un rango virtual
/// contiguo antes de saltar aquí.
///
/// Sin esto no hay forma de leer una tabla de páginas: sus entradas guardan
/// direcciones físicas, y la CPU con paginación activa solo entiende
/// virtuales. Es el `physical_memory_offset` que las fases 1 y 2 llevaban
/// reportando como «sin mapear».
const CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &CONFIG);

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
    println!("memoria virtual");

    let physical_offset = boot_info
        .physical_memory_offset
        .into_option()
        .expect("el bootloader debía mapear la memoria física");
    let regions: &'static MemoryRegions = &boot_info.memory_regions;

    // SAFETY: el offset lo ha puesto el propio bootloader, y las regiones que
    // marca como utilizables lo son.
    let mut mapper = unsafe { memory::Mapper::new(physical_offset) };
    let mut frames = unsafe { memory::FrameAllocator::new(regions) };
    println!("  física       mapeada en {physical_offset:#x}");

    // Traducir una dirección real, para ver el mecanismo en funcionamiento en
    // vez de creérselo.
    let en_la_pila = 0u64;
    let virtual_address = &en_la_pila as *const u64 as u64;
    match mapper.translate(virtual_address) {
        Some(physical) => {
            println!("  traducción   {virtual_address:#x} → física {physical:#x}")
        }
        None => println!("  traducción   {virtual_address:#x} no está mapeada (?)"),
    }

    unsafe { memory::init_heap(&mut mapper, &mut frames) }.expect("no pude mapear el heap");
    // SAFETY: el rango acaba de mapearse y nadie más lo usa.
    unsafe { allocator::init(memory::HEAP_START as usize, memory::HEAP_SIZE) };
    println!("  asignador    {}", allocator::name());
    println!("  marcos       {} entregados", frames.frames_handed_out());

    heap_demo();
    reuse_test();

    println!();
    println!("espacio de usuario");

    userspace::init();
    println!("  syscall      habilitado · el anillo 3 ya tiene por dónde entrar");

    // El programa va incrustado en el binario del kernel. En un sistema con
    // disco lo leería un cargador; mientras no lo haya, viaja dentro.
    let image = include_bytes!(env!("USER_BINARY"));
    println!("  programa     ELF de {} KiB incrustado", image.len() / 1024);

    // SAFETY: el ELF lo hemos compilado nosotros en el mismo repositorio.
    let entry = unsafe { elf::load(image, &mut mapper, &mut frames) }
        .expect("no pude cargar el programa de usuario");
    // SAFETY: el rango de pila no lo usa nadie más.
    let user_stack = unsafe { userspace::map_user_stack(&mut mapper, &mut frames) }
        .expect("no pude mapear la pila de usuario");
    println!("  cargado      entrada {entry:#x} · pila {user_stack:#x}");

    println!();
    println!("  ─── ejecución limpia ───");
    // SAFETY: entrada y pila están mapeadas con el bit de usuario.
    let code = unsafe { userspace::enter(entry, user_stack, 0) };
    println!("  el programa terminó con código {code}");

    println!();
    println!("  ─── ahora intenta leer el kernel ───");
    let code = unsafe { userspace::enter(entry, user_stack, 1) };
    println!("  el kernel recuperó el control · código {code:#x}");

    println!();
    println!("multitarea cooperativa");

    let mut executor = task::executor::Executor::new();
    executor.spawn(Task::new(example_task()));
    executor.spawn(Task::new(heartbeat_task()));
    executor.spawn(Task::new(task::keyboard::keyboard_task()));
    println!("  3 tareas encoladas · el ejecutor toma el control");
    println!();

    // No vuelve: a partir de aquí el kernel es su bucle de eventos.
    executor.run()
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

fn heap_demo() {
    println!();
    println!("el heap funciona");

    let boxed = Box::new(42u64);
    let numbers: Vec<u64> = (1..=100).collect();
    let total: u64 = numbers.iter().sum();
    let text = String::from("Box, Vec y String ya existen dentro de syso");

    println!("  box          {boxed}");
    println!("  vec          {} elementos, suma {total}", numbers.len());
    println!("  string       «{text}»");
    println!("  heap usado   {} B", allocator::used());
}

/// Mantiene viva UNA asignación mientras hace y deshace muchas otras.
///
/// Es exactamente el patrón que el asignador de puntero no sobrevive: su
/// `dealloc` solo recupera memoria cuando NADA queda vivo, así que el ancla
/// basta para que no se recupere nunca un solo byte.
fn reuse_test() {
    println!();
    println!("prueba de reutilización · 5000 ciclos con un ancla viva");

    let ancla = Box::new(0u64);

    for cycle in 1..=5000u32 {
        let bloque: Vec<u64> = (0..32).collect();
        core::hint::black_box(&bloque);
        if cycle % 1000 == 0 {
            println!("  ciclo {cycle:>4}   heap usado {} B", allocator::used());
        }
    }

    core::hint::black_box(&ancla);
    println!("  5000 ciclos completados sin agotar el heap");
}

async fn suma(a: u32, b: u32) -> u32 {
    a + b
}

/// La tarea más tonta posible, solo para ver que una `async fn` que espera a
/// otra funciona igual que en cualquier programa de Rust.
async fn example_task() {
    println!("  tarea ejemplo · 40 + 2 = {}", suma(40, 2).await);
}

/// Late cinco veces y termina. Sirve para ver dos cosas: que una tarea puede
/// dormir sin bloquear a las demás, y que al acabar el ejecutor la retira.
async fn heartbeat_task() {
    for beat in 1..=5u32 {
        task::timer::sleep(task::timer::TICKS_PER_SECOND).await;
        println!("  latido {beat} · tick {}", task::timer::ticks());
    }
    println!("  la tarea de latido ha terminado · el ejecutor la retira");
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
    // Callar las interrupciones antes de nada. Se vio en la Fase 3: tras un
    // panic el temporizador seguía latiendo tan tranquilo, escribiendo por
    // encima del informe del fallo. Un kernel que ha entrado en panic no
    // tiene por qué seguir atendiendo hardware.
    //
    // SAFETY: `cli` solo baja IF, y a partir de aquí no se vuelve.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };

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
