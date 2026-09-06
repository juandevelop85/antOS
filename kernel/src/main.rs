// antOS - kernel x86_64
//
// Fase 5: espacio de usuario. Anillo 3, llamadas al sistema con syscall/sysret
// y un ELF cargado en tiempo de ejecución — código que NO PUEDE tocar el
// kernel, y la MMU encargándose de que así sea.

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

// El crate `alloc` trae Box, Vec, String y compañía. No forma parte de core,
// pero tampoco necesita sistema operativo: solo un #[global_allocator].
extern crate alloc;

#[allow(dead_code)]
mod allocator;
pub mod arch;
#[cfg(target_arch = "x86_64")]
pub mod console;
#[cfg(target_arch = "x86_64")]
pub mod drivers;
pub mod fs;
#[allow(dead_code)]
mod elf;
#[allow(dead_code)]
mod memory;
#[allow(dead_code)]
mod sync;
pub mod ipc;
pub mod syscall;
#[allow(dead_code)]
mod task;

#[cfg(target_arch = "x86_64")]
pub static EMBEDDED_INITRD: &[u8] = include_bytes!(env!("INITRD_TAR"));

#[cfg(target_arch = "x86_64")]
pub use arch::current::{gdt, interrupts, port, serial, userspace};

#[cfg(target_arch = "aarch64")]
pub use arch::current::{entry, exceptions, mmu, pl011, serial};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(target_arch = "x86_64")]
use bootloader_api::config::{BootloaderConfig, Mapping};
#[cfg(target_arch = "x86_64")]
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
#[cfg(target_arch = "x86_64")]
use bootloader_api::{entry_point, BootInfo};
#[cfg(target_arch = "x86_64")]
use task::Task;
use core::fmt::Write;
use core::panic::PanicInfo;

#[cfg(target_arch = "aarch64")]
#[link_section = ".bss.heap"]
static mut AARCH64_HEAP: [u8; 512 * 1024] = [0; 512 * 1024];

#[cfg(target_arch = "x86_64")]
const CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

#[cfg(target_arch = "x86_64")]
entry_point!(kernel_main, config = &CONFIG);

#[cfg(target_arch = "aarch64")]
pub fn kmain_arm64(dtb_ptr: u64) -> ! {
    arch::aarch64::SERIAL.lock().init();

    println!();
    println!("antOS · kernel AArch64");
    println!("═══════════════════════");

    println!("arranque");
    println!("  arquitectura AArch64 (ARM 64-bit)");
    println!("  dtb          apuntado en {dtb_ptr:#x}");

    println!();
    println!("excepciones");
    arch::aarch64::exceptions::init();
    println!("  vbar_el1     cargada · 16 vectores atendidos");

    arch::aarch64::exceptions::trigger_breakpoint();
    println!("  breakpoint   manejado y ejecución reanudada");

    println!();
    println!("memoria virtual (MMU)");
    arch::aarch64::mmu::init();
    println!("  mmu          activa · gránulos de 4 KiB, tablas L0/L1/L2");
    println!("  caches       d-cache e i-cache habilitadas");

    // Inicializar asignador dinámico de memoria sobre RAM mapeada por la MMU
    unsafe {
        allocator::init(core::ptr::addr_of_mut!(AARCH64_HEAP) as usize, 512 * 1024);
    }
    let boxed = Box::new(42u64);
    let mut numbers = Vec::new();
    for i in 1..=5 {
        numbers.push(i);
    }
    let text = String::from("antOS heap dinámico en AArch64");
    println!("  asignador    {} B usados · Box({boxed}), Vec({numbers:?}), String('{}')",
        allocator::used(), text);

    println!();
    println!("controlador de interrupciones y temporizador");
    arch::aarch64::gic::init();
    println!("  gicv2        distribuidor y cpu interface activos");

    arch::aarch64::timer::init();
    println!("  temporizador virtual configurado a 100 Hz (IRQ 27)");

    arch::aarch64::exceptions::enable_irq();
    println!("  daif         irq habilitadas · recibiendo pulsos...");

    // Esperar 5 pulsos de reloj para certificar la entrega de interrupciones
    let start_ticks = arch::aarch64::timer::ticks();
    while arch::aarch64::timer::ticks() < start_ticks + 5 {
        arch::aarch64::exceptions::wait_for_interrupt();
    }
    println!("  ticks        {} pulsos de temporizador verificados con éxito", arch::aarch64::timer::ticks());

    println!();
    println!("espacio de usuario (EL0) y llamadas al sistema (SVC)");
    let (user_entry, user_sp, arg0, arg1) = unsafe {
        arch::aarch64::syscall::setup_test_userspace()
    };
    println!("  transición   saltando a EL0 en {user_entry:#x} con sp {user_sp:#x}");
    let exit_code = unsafe {
        arch::aarch64::syscall::enter_user_mode(user_entry, user_sp, arg0, arg1)
    };
    println!("  retorno      el programa EL0 finalizó limpiamente con código de salida {exit_code}");

    println!();
    println!("sistema operativo listo (AArch64 bare metal)");

    halt_loop()
}

#[cfg(target_arch = "x86_64")]
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::SERIAL.lock().init();

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer_len = framebuffer.buffer_mut().len();
        let buffer_ptr = framebuffer.buffer_mut().as_mut_ptr();
        unsafe {
            console::init(buffer_ptr, buffer_len, info);
        }
    }

    println!();
    println!("\x1b[1;36mantOS\x1b[0m · \x1b[1;32mkernel x86_64\x1b[0m");
    println!("═══════════════════════");

    // El bootloader nos entrega esto y desaparece. Es todo lo que sabemos
    // de la máquina, así que conviene mirarlo antes de dar nada por hecho.
    report_memory(boot_info);
    report_framebuffer(boot_info);

    println!();
    println!("\x1b[1;33minterrupts\x1b[0m");

    // Order matters: IDT needs our GDT's code selector, and the double fault
    // handler needs the TSS already loaded.
    gdt::init();
    println!("  gdt          loaded, with dedicated double-fault stack");

    interrupts::init();
    println!("  idt          loaded, 9 vectors wired (incl. spurious 0xFF)");

    // Prove that an exception can be handled and CONTINUE: `int3` jumps to
    // the breakpoint handler, which prints and returns right here.
    interrupts::trigger_breakpoint();
    println!("  breakpoint   handled, execution resumed");

    // Remap the PIC with only keyboard (IRQ1) unmasked. The timer (IRQ0) is
    // now driven by the LAPIC, not the PIC.
    interrupts::init_pic();
    println!("  pic 8259     remapped, keyboard-only (timer via LAPIC)");

    println!();
    println!("virtual memory");

    let physical_offset = boot_info
        .physical_memory_offset
        .into_option()
        .expect("bootloader must map physical memory");
    let regions: &'static MemoryRegions = &boot_info.memory_regions;

    // SAFETY: offset set by the bootloader, and usable regions are valid.
    let mut mapper = unsafe { memory::Mapper::new(physical_offset) };
    let mut frames = unsafe { memory::FrameAllocator::new(regions) };
    println!("  physical     mapped at {physical_offset:#x}");

    let stack_probe = 0u64;
    let virtual_address = &stack_probe as *const u64 as u64;
    match mapper.translate(virtual_address) {
        Some(physical) => {
            println!("  translate    {virtual_address:#x} → physical {physical:#x}")
        }
        None => println!("  translate    {virtual_address:#x} not mapped (?)"),
    }

    unsafe { memory::init_heap(&mut mapper, &mut frames) }.expect("could not map heap");
    // SAFETY: the range was just mapped and nobody else uses it.
    unsafe { allocator::init(memory::HEAP_START as usize, memory::HEAP_SIZE) };
    println!("  allocator    {}", allocator::name());
    println!("  frames       {} handed out", frames.frames_handed_out());

    heap_demo();
    reuse_test();

    // ── APIC initialization (requires mapper + allocator) ────────────────
    println!();
    println!("\x1b[1;32mapic\x1b[0m");

    // SAFETY: called once, mapper and allocator are valid, LAPIC address
    // comes from the CPU's own MSR.
    match unsafe { crate::arch::x86_64::apic::init(&mut mapper, &mut frames, 100) } {
        Ok(()) => println!("  status       LAPIC active, PIC timer replaced"),
        Err(e) => {
            println!("  warning      APIC init failed: {e}");
            println!("               falling back to PIC timer");
        }
    }

    interrupts::enable();
    println!("  interrupts   enabled");

    // ── Secondary Storage (VirtIO-blk) and VFS (T23.4) ────────────────────
    println!();
    println!("\x1b[1;36malmacenamiento secundario (VirtIO-blk) y VFS (T23.4)\x1b[0m");

    let pci_devices = drivers::pci::scan_pci_bus();
    let mut virtio_blk_found = false;

    for dev in &pci_devices {
        if dev.vendor_id == drivers::virtio_blk::VIRTIO_VENDOR_ID
            && (dev.device_id == drivers::virtio_blk::VIRTIO_DEV_BLOCK_LEGACY
                || dev.device_id == drivers::virtio_blk::VIRTIO_DEV_BLOCK_MODERN
                || dev.subsystem_device_id == 2)
        {
            if let Some(phys0) = frames.allocate_contiguous(4) {
                let virt_ptr = (phys0 + physical_offset) as *mut u8;
                match unsafe { drivers::virtio_blk::VirtioBlock::init(dev, virt_ptr, phys0) } {
                    Ok(blk) => {
                        let cap_mb = blk.capacity_bytes() / (1024 * 1024);
                        let sectors = blk.capacity_sectors();
                        println!(
                            "  virtio-blk   PCI {}:{}.{} · {} sectores ({} MiB)",
                            dev.bus, dev.slot, dev.func, sectors, cap_mb
                        );
                        *drivers::virtio_blk::BLOCK_DEVICE.lock() = Some(blk);
                        drivers::storage::register_virtio_device("/dev/vda", sectors, 512, "VirtIO Block Device");
                        virtio_blk_found = true;
                    }
                    Err(e) => {
                        println!("  advertencia  fallo al inicializar virtio-blk: {:?}", e);
                    }
                }
            }
            break;
        }
    }

    // Detect and initialize physical storage controllers (AHCI / NVMe) (T24.3)
    drivers::storage::detect_and_init_storage(&pci_devices, &mut mapper, &mut frames, physical_offset);

    if !virtio_blk_found {
        println!("  virtio-blk   no detectado en bus PCI · usando ramdisk en memoria");
    }

    if let Ok(ramdisk) = fs::ramdisk::Ramdisk::new(EMBEDDED_INITRD) {
        println!("  initramfs    Live Ramdisk detectado ({} KiB, {} entradas, init: {})",
            ramdisk.size() / 1024, ramdisk.entry_count(), ramdisk.has_init());
    }

    // Mount root filesystem: prefer block device if available, else embedded initrd
    let root_fs = if virtio_blk_found {
        match fs::tarfs::TarFs::from_block_device() {
            Ok(tfs) => {
                println!("  tarfs        montado desde dispositivo de bloque VirtIO (/dev/vda)");
                Some(tfs)
            }
            Err(_) => {
                println!("  tarfs        fallback a ramdisk en memoria");
                fs::tarfs::TarFs::from_memory(EMBEDDED_INITRD).ok()
            }
        }
    } else {
        fs::tarfs::TarFs::from_memory(EMBEDDED_INITRD).ok()
    };

    if let Some(tfs) = root_fs {
        println!("  vfs          raíz montada · {} ficheros indexados:", tfs.entry_count());
        for entry in tfs.all_entries() {
            let kind = if entry.is_dir { "DIR " } else { "FILE" };
            println!("    [{kind}] {:<18} ({:>6} B)", entry.path, entry.size);
        }
        fs::vfs::mount_root(tfs);
    } else {
        panic!("no se pudo montar el sistema de ficheros raíz (VFS)");
    }

    if let Ok(conf_str) = fs::vfs::read_to_string("/etc/antos.conf") {
        println!("  config       /etc/antos.conf cargado ({} B)", conf_str.len());
    }

    println!();
    println!("\x1b[1;35mespacio de usuario (cargador dinámico VFS)\x1b[0m");

    userspace::init();
    println!("  syscall      habilitado · el anillo 3 ya tiene por dónde entrar");

    // Desacoplamiento de include_bytes!: cargar dinámicamente desde /bin/init en VFS
    let init_bytes = fs::vfs::read_all("/bin/init")
        .expect("no pude leer /bin/init desde el VFS");
    println!("  cargador     ELF dinámico de {} KiB cargado desde VFS (/bin/init)", init_bytes.len() / 1024);

    // Verificar carga de segundo ejecutable ELF (/bin/worker) desde VFS
    let worker_bytes = fs::vfs::read_all("/bin/worker")
        .expect("no pude leer /bin/worker desde el VFS");
    println!("  segundo ELF  {} KiB verificado desde VFS (/bin/worker)", worker_bytes.len() / 1024);

    // SAFETY: el ELF lo hemos compilado nosotros y cargado desde el VFS
    let entry = unsafe { elf::load(&init_bytes, &mut mapper, &mut frames) }
        .expect("no pude cargar el programa de usuario desde VFS");
    // SAFETY: el rango de pila no lo usa nadie más.
    let user_stack = unsafe { userspace::map_user_stack(&mut mapper, &mut frames) }
        .expect("no pude mapear la pila de usuario");
    println!("  cargado      entrada {entry:#x} · pila {user_stack:#x}");

    // Separate user stack for process 2 (0x6000_0000)
    let user_stack_2 = unsafe {
        userspace::map_user_stack_at(0x6000_0000, &mut mapper, &mut frames)
    }.expect("no pude mapear pila para proceso 2");

    // Initialize global memory controller for dynamic syscalls (mmap, munmap, spawn)
    memory::init_memory_controller(mapper, frames);

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
    println!("\x1b[1;36mmultitarea preemptiva (T23.2)\x1b[0m");

    task::scheduler::init();
    println!(
        "  scheduler    Round-Robin activo · quantum {} ticks ({} ms)",
        task::scheduler::DEFAULT_QUANTUM_TICKS,
        task::scheduler::DEFAULT_QUANTUM_TICKS * 10
    );

    let cr3_root: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3_root, options(nomem, nostack)) };

    // Spawn Process 1: background worker (mode 2)
    let (pid1, tid1) = task::scheduler::spawn_process(
        "worker-pulse",
        cr3_root,
        entry,
        user_stack,
        0, // auto-allocate kernel stack
        true,
        2, // mode 2
    );
    println!("  proceso 1    spawned PID {pid1} (TID {tid1}) · worker concurrente");

    // Spawn Process 2: preemptible compute worker (mode 3)
    let (pid2, tid2) = task::scheduler::spawn_process(
        "worker-preempt",
        cr3_root,
        entry,
        user_stack_2,
        0, // auto-allocate kernel stack
        true,
        3, // mode 3
    );
    println!("  proceso 2    spawned PID {pid2} (TID {tid2}) · worker preemptivo");

    let metrics = task::scheduler::metrics();
    println!(
        "  metricas     {} procesos, {} hilos en cola de listos",
        metrics.total_processes, metrics.ready_threads
    );

    println!();
    println!("\x1b[1;33mmultitarea cooperativa\x1b[0m");

    let mut executor = task::executor::Executor::new();
    executor.spawn(Task::new(example_task()));
    executor.spawn(Task::new(heartbeat_task()));
    executor.spawn(Task::new(task::keyboard::keyboard_task()));
    println!("  3 tareas encoladas · el ejecutor toma el control");
    println!();

    // No vuelve: a partir de aquí el kernel es su bucle de eventos.
    executor.run()
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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
    let (cols, rows) = if let Some(guard) = console::CONSOLE.lock().as_ref() {
        guard.dimensions()
    } else {
        (0, 0)
    };
    println!(
        "  consola      {}x{} caracteres (fuente 8x16 bitmap, ANSI activo)",
        cols, rows
    );
}

#[cfg(target_arch = "x86_64")]
fn heap_demo() {
    println!();
    println!("el heap funciona");

    let boxed = Box::new(42u64);
    let numbers: Vec<u64> = (1..=100).collect();
    let total: u64 = numbers.iter().sum();
    let text = String::from("Box, Vec y String ya existen dentro de antOS");

    println!("  box          {boxed}");
    println!("  vec          {} elementos, suma {total}", numbers.len());
    println!("  string       «{text}»");
    println!("  heap usado   {} B", allocator::used());
}

/// Mantiene viva UNA asignación mientras hace y deshace muchas otras.
#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
async fn suma(a: u32, b: u32) -> u32 {
    a + b
}

/// La tarea más tonta posible, solo para ver que una `async fn` que espera a
/// otra funciona igual que en cualquier programa de Rust.
#[cfg(target_arch = "x86_64")]
async fn example_task() {
    println!("  tarea ejemplo · 40 + 2 = {}", suma(40, 2).await);
}

/// Late cinco veces y termina. Sirve para ver dos cosas: que una tarea puede
/// dormir sin bloquear a las demás, y que al acabar el ejecutor la retira.
#[cfg(target_arch = "x86_64")]
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
    use crate::arch::traits::ArchInterrupts;
    loop {
        crate::arch::current::Interrupts::halt();
    }
}

/// Lo que se ejecuta tras un panic. Ya no se limita a parar la máquina:
/// ahora dice qué pasó y dónde, que es la mitad del trabajo de depurar.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Callar las interrupciones antes de nada.
    use crate::arch::traits::ArchInterrupts;
    crate::arch::current::Interrupts::disable();

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

    #[cfg(target_arch = "x86_64")]
    if let Some(mut guard) = console::CONSOLE.try_lock() {
        if let Some(console) = guard.as_mut() {
            let _ = writeln!(console, "\n\x1b[1;31m╔══════════════════════════════\x1b[0m");
            let _ = writeln!(console, "\x1b[1;31m║ PANIC DEL KERNEL\x1b[0m");
            if let Some(location) = info.location() {
                let _ = writeln!(
                    console,
                    "\x1b[1;31m║ en {}:{}:{}\x1b[0m",
                    location.file(),
                    location.line(),
                    location.column()
                );
            }
            let _ = writeln!(console, "\x1b[1;31m║ {}\x1b[0m", info.message());
            let _ = writeln!(console, "\x1b[1;31m╚══════════════════════════════\x1b[0m");
        }
    }

    halt_loop()
}
