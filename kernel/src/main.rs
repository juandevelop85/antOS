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

pub mod acpi;
#[allow(dead_code)]
mod allocator;
pub mod arch;
pub mod console;
pub mod drivers;
#[allow(dead_code)]
mod elf;
pub mod fs;
pub mod input;
pub mod ipc;
#[cfg(feature = "limine")]
pub mod limine;
#[allow(dead_code)]
mod memory;
#[allow(dead_code)]
mod sync;
pub mod syscall;
#[allow(dead_code)]
mod task;
pub mod ui;

// Built by `build.rs` for whichever architecture the kernel itself targets
// (T26.5): both AArch64 and x86_64 mount this same way, via `fs::tarfs`.
pub static EMBEDDED_INITRD: &[u8] = include_bytes!(env!("INITRD_TAR"));

#[cfg(target_arch = "x86_64")]
pub use arch::current::{gdt, interrupts, port, serial, userspace};

#[cfg(all(target_arch = "aarch64", not(feature = "limine")))]
pub use arch::current::entry;
#[cfg(target_arch = "aarch64")]
pub use arch::current::{exceptions, mmu, pl011, serial};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(all(target_arch = "x86_64", not(feature = "limine")))]
use bootloader_api::config::{BootloaderConfig, Mapping};
#[cfg(all(target_arch = "x86_64", not(feature = "limine")))]
use bootloader_api::entry_point;
#[cfg(target_arch = "x86_64")]
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
#[cfg(target_arch = "x86_64")]
use bootloader_api::BootInfo;
use core::fmt::Write;
use core::panic::PanicInfo;
#[cfg(target_arch = "x86_64")]
use task::Task;

// 512 KiB was enough before T26.5: nothing in the AArch64 boot path held more
// than a few kilobytes of heap data at once. Loading a real userspace ELF
// through `fs::vfs::read_all` — the antos-init binary is ~1.6 MiB — needs
// room for that whole buffer plus the TarFs index and the usual Box/Vec/
// String demos, so this now matches x86_64's 4 MiB kernel heap (`memory::HEAP_SIZE`).
#[cfg(target_arch = "aarch64")]
const AARCH64_HEAP_SIZE: usize = 8 * 1024 * 1024;

#[cfg(target_arch = "aarch64")]
#[link_section = ".bss.heap"]
static mut AARCH64_HEAP: [u8; AARCH64_HEAP_SIZE] = [0; AARCH64_HEAP_SIZE];

// Under `--features limine` (T27.1), `arch::x86_64::limine_boot::_start`
// becomes the linked entry point instead — Limine's boot protocol is a
// different contract from the `bootloader` crate's own (see that module's
// docs), so the two `_start`s can never coexist in the same binary. Both
// ultimately call this same `kernel_main`.
#[cfg(all(target_arch = "x86_64", not(feature = "limine")))]
const CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

#[cfg(all(target_arch = "x86_64", not(feature = "limine")))]
entry_point!(kernel_main, config = &CONFIG);

/// `booted_via_limine`: when `true`, the MMU is already on and `TTBR1_EL1`
/// already maps this kernel — entered via `arch::aarch64::limine_boot`
/// (T27.1) instead of the direct-QEMU-boot path in `arch::aarch64::entry`.
/// The only thing that actually changes below is which `mmu` initializer
/// runs; see `mmu::init_ttbr0_under_limine`'s docs for why the other one
/// would crash here.
#[cfg(target_arch = "aarch64")]
pub fn kmain_arm64(dtb_ptr: u64, booted_via_limine: bool) -> ! {
    #[cfg(feature = "limine")]
    let mut limine_fb_active = false;
    #[cfg(feature = "limine")]
    if booted_via_limine {
        let fb_resp = crate::limine::FRAMEBUFFER_REQUEST.response;
        if !fb_resp.is_null() && unsafe { (*fb_resp).framebuffer_count } > 0 {
            let limine_fb = unsafe { *(*fb_resp).framebuffers };
            let fb = unsafe { &*limine_fb };
            let bpp = fb.bpp as usize;
            let bytes_per_pixel = (bpp / 8).max(1);
            let stride = fb.pitch as usize / bytes_per_pixel;
            let width = fb.width as usize;
            let height = fb.height as usize;
            let size = fb.pitch as usize * height;
            let format = if fb.red_mask_shift == 16 {
                bootloader_api::info::PixelFormat::Bgr
            } else {
                bootloader_api::info::PixelFormat::Rgb
            };
            unsafe {
                console::init_raw(
                    fb.address,
                    size,
                    width,
                    height,
                    stride,
                    bytes_per_pixel,
                    format,
                );
            }
            if let Some(c) = console::CONSOLE.lock().as_mut() {
                c.draw_header_banner(
                    "antOS · Limine UEFI (AArch64)",
                    "CPU: Cortex-A72 / VirtualBox",
                    "RAM: 8 MiB Heap",
                );
            }
            limine_fb_active = true;
        }
    }

    // Stash the firmware device-tree pointer first so even the earliest
    // discovery (PL011 base, T28.8) can consult it.
    arch::aarch64::dtb::set_dtb_base(dtb_ptr);

    arch::aarch64::SERIAL.lock().init();

    println!();
    println!("antOS · kernel AArch64");
    println!("═══════════════════════");

    let current_el: u64;
    unsafe {
        let raw_el: u64;
        core::arch::asm!("mrs {}, CurrentEL", out(reg) raw_el, options(nomem, nostack));
        current_el = (raw_el >> 2) & 0x3;
    }

    println!("arranque");
    println!("  arquitectura AArch64 (ARM 64-bit)");
    println!("  nivel        EL{} (supervisor)", current_el);
    println!("  dtb          apuntado en {dtb_ptr:#x}");
    if booted_via_limine {
        println!("  bootloader   Limine (protocolo UEFI real, T27.1)");
    }

    println!();
    println!("excepciones");
    arch::aarch64::exceptions::init();
    println!("  vbar_el1     cargada · 16 vectores atendidos");
    println!("  manejador    listo para excepciones síncronas e IRQs");

    println!();
    println!("memoria virtual (MMU)");
    if booted_via_limine {
        // Already installed by `limine_boot::_start`, *before* this function's
        // very first line (`SERIAL.lock().init()`) touches the PL011 UART's
        // MMIO registers — under Limine the MMU is on from the first
        // instruction, so that touch needs a working TTBR0 mapping to not
        // fault, and faulting this early (before `exceptions::init()` below
        // has installed a vector table) hangs the machine with no message
        // at all. Doing it here instead would be too late.
        println!("  mmu          TTBR1 de Limine intacta · TTBR0 propia instalada (T27.1)");
    } else {
        arch::aarch64::mmu::init();
        println!("  mmu          activa · gránulos de 4 KiB, tablas L0/L1/L2");
    }
    println!("  caches       d-cache e i-cache habilitadas");

    // Inicializar asignador dinámico de memoria sobre RAM mapeada por la MMU
    unsafe {
        allocator::init(
            core::ptr::addr_of_mut!(AARCH64_HEAP) as usize,
            AARCH64_HEAP_SIZE,
        );
    }
    let boxed = Box::new(42u64);
    let mut numbers = Vec::new();
    for i in 1..=5 {
        numbers.push(i);
    }
    let text = String::from("antOS heap dinámico en AArch64");
    println!(
        "  asignador    {} B usados · Box({boxed}), Vec({numbers:?}), String('{}')",
        allocator::used(),
        text
    );

    println!();
    println!("controlador gráfico y framebuffer (T26.1)");

    #[cfg(feature = "limine")]
    let mut graphical_fb_active = limine_fb_active;
    #[cfg(not(feature = "limine"))]
    let mut graphical_fb_active = false;

    #[cfg(feature = "limine")]
    if limine_fb_active {
        println!("  limine       framebuffer UEFI (GOP) activo vía protocolo Limine");
        // Snapshot the geometry, then drop the CONSOLE lock *before* println!
        // (which re-locks CONSOLE — holding it across the print self-deadlocks).
        let geom = console::CONSOLE.lock().as_ref().map(|c| {
            let fb = c.framebuffer();
            (
                fb.width(),
                fb.height(),
                fb.stride_pixels(),
                fb.bytes_per_pixel(),
            )
        });
        if let Some((w, h, stride_px, bpp)) = geom {
            println!(
                "  fb-geom      {}x{} · stride {} px · {} B/px{}",
                w,
                h,
                stride_px,
                bpp,
                if stride_px != w {
                    " (pitch con relleno)"
                } else {
                    ""
                }
            );
        }
    }

    // 1. Introspección del DTB para simple-framebuffer
    if !graphical_fb_active {
        if let Some(fb) = arch::aarch64::dtb::find_framebuffer(dtb_ptr) {
            println!("  dtb          nodo simple-framebuffer descubierto");
            println!(
                "  resolución   {}x{} · formato {:?} ({} bytes/px)",
                fb.width, fb.height, fb.format, fb.bytes_per_pixel
            );
            println!(
                "  memoria      base física {:#x} ({} KiB)",
                fb.phys_addr,
                fb.size / 1024
            );

            // T31.10: `map_framebuffer_range` ya no devuelve `Result` — nunca
            // tuvo un camino de error real, siempre mapeaba con éxito.
            let mapped_addr = arch::aarch64::mmu::map_framebuffer_range(fb.phys_addr, fb.size);
            println!(
                "  mmu          mapeado en {:#x} (Normal Non-Cacheable)",
                mapped_addr
            );
            unsafe {
                console::init_raw(
                    mapped_addr as *mut u8,
                    fb.size,
                    fb.width,
                    fb.height,
                    fb.stride,
                    fb.bytes_per_pixel,
                    fb.format,
                );
            }
            if let Some(c) = console::CONSOLE.lock().as_mut() {
                c.draw_header_banner(
                    "antOS · AArch64",
                    "CPU: Cortex-A72 (EL1)",
                    "RAM: 512 KiB Heap",
                );
            }
            println!("  consola      activa en pantalla gráfica y serie simultáneamente");
            graphical_fb_active = true;
        }
    }

    // 2. Si no hay simple-framebuffer en DTB, buscar dispositivo VirtIO-GPU MMIO
    if !graphical_fb_active {
        let virtio_gpu_base = arch::aarch64::dtb::find_virtio_gpu(dtb_ptr)
            .or_else(arch::aarch64::virtio_gpu::probe_virtio_gpu);

        if let Some(gpu_base) = virtio_gpu_base {
            println!(
                "  virtio-gpu   dispositivo MMIO detectado en {:#x}",
                gpu_base
            );
            match unsafe { arch::aarch64::virtio_gpu::VirtioGpu::init(gpu_base, 1024, 768) } {
                Ok(gpu) => {
                    let buf_ptr = gpu.buffer_ptr();
                    let buf_len = gpu.buffer_len();
                    let w = gpu.width();
                    let h = gpu.height();
                    let stride = gpu.stride();
                    *arch::aarch64::virtio_gpu::VIRTIO_GPU.lock() = Some(gpu);
                    unsafe {
                        console::init_raw(
                            buf_ptr,
                            buf_len,
                            w,
                            h,
                            stride,
                            4,
                            bootloader_api::info::PixelFormat::Bgr,
                        );
                    }
                    if let Some(c) = console::CONSOLE.lock().as_mut() {
                        c.draw_header_banner(
                            "antOS · VirtIO-GPU",
                            "CPU: Cortex-A72 (EL1)",
                            "RAM: 512 KiB Heap",
                        );
                    }
                    println!("  virtio-gpu   recurso 2D creado · escaneo 1024x768x32bpp enlazado");
                    println!("  consola      activa en monitor VirtIO-GPU y serie simultáneamente");
                    graphical_fb_active = true;
                }
                Err(_) => {
                    println!("  error        fallo al inicializar VirtIO-GPU MMIO");
                }
            }
        }
    }

    // 3. VirtIO-GPU sobre PCIe (virtio-gpu-pci, requiere el bus PCIe de T28.2)
    if !graphical_fb_active {
        if let Some(dev) = arch::aarch64::virtio_gpu_pci::probe() {
            println!(
                "  virtio-gpu   virtio-gpu-pci {:04x}:{:04x} en bus {} dev {}",
                dev.vendor_id, dev.device_id, dev.bus, dev.slot
            );
            match unsafe { arch::aarch64::virtio_gpu_pci::VirtioGpuPci::init(&dev, 1024, 768) } {
                Ok(gpu) => {
                    let (bp, bl, w, h, st) = (
                        gpu.buffer_ptr(),
                        gpu.buffer_len(),
                        gpu.width(),
                        gpu.height(),
                        gpu.stride(),
                    );
                    *arch::aarch64::virtio_gpu_pci::VIRTIO_GPU_PCI.lock() = Some(gpu);
                    unsafe {
                        console::init_raw(
                            bp,
                            bl,
                            w,
                            h,
                            st,
                            4,
                            bootloader_api::info::PixelFormat::Bgr,
                        );
                    }
                    if let Some(c) = console::CONSOLE.lock().as_mut() {
                        c.draw_header_banner(
                            "antOS · VirtIO-GPU PCIe",
                            "CPU: Cortex-A72 (EL1)",
                            "RAM: 512 KiB Heap",
                        );
                    }
                    println!(
                        "  consola      activa en virtio-gpu-pci ({}x{}) y serie",
                        w, h
                    );
                    graphical_fb_active = true;
                }
                Err(_) => println!("  error        fallo al inicializar virtio-gpu-pci"),
            }
        }
    }

    // 4. ramfb vía fw_cfg (QEMU/UTM sin UEFI ni virtio-gpu)
    if !graphical_fb_active {
        let fw_cfg_base =
            arch::aarch64::dtb::find_fw_cfg().unwrap_or(arch::aarch64::fw_cfg::FW_CFG_MMIO_DEFAULT);
        match unsafe { arch::aarch64::fw_cfg::init_ramfb(fw_cfg_base, 1024, 768) } {
            Some(fb) => {
                unsafe {
                    console::init_raw(
                        fb.ptr,
                        fb.len,
                        fb.width,
                        fb.height,
                        fb.stride_bytes / 4,
                        4,
                        bootloader_api::info::PixelFormat::Bgr,
                    );
                }
                if let Some(c) = console::CONSOLE.lock().as_mut() {
                    c.draw_header_banner(
                        "antOS · ramfb",
                        "CPU: Cortex-A72 (EL1)",
                        "RAM: 512 KiB Heap",
                    );
                }
                println!(
                    "  ramfb        fw_cfg en {:#x} · framebuffer {}x{} enlazado",
                    fw_cfg_base, fb.width, fb.height
                );
                println!("  consola      activa en ramfb y serie simultáneamente");
                graphical_fb_active = true;
            }
            None => println!(
                "  ramfb        no disponible (fw_cfg {:#x} sin etc/ramfb)",
                fw_cfg_base
            ),
        }
    }

    if graphical_fb_active {
        ui::init("AArch64 / Cortex-A72");
        println!("  compositor   desktop shell 2D listo (disponible con el comando 'desktop')");
    } else {
        println!("  framebuffer  cadena agotada (GOP → DTB → virtio-gpu MMIO → virtio-gpu-pci → ramfb) · modo headless / UART serie");
    }

    println!();
    println!("controlador de entrada y periféricos (T26.3 / T27.2)");
    let input_devs = drivers::virtio_input::probe_and_init_virtio_inputs();
    if input_devs > 0 {
        println!(
            "  virtio-input {} dispositivo(s) de entrada activos (teclado/ratón/tablet)",
            input_devs
        );
    } else {
        println!("  virtio-input no detectado (probando PCIe xHCI y consola serie)");
    }

    // Inicializar bus PCIe y controlador host USB 3.0 xHCI (T27.2 / T28.2)
    let ecam_base = drivers::pci::probe_ecam_base();
    let pci_scan = drivers::pci::scan_pci_bus();
    println!(
        "  pcie-ecam    ventana ECAM en {:#x} · {} función(es) PCI detectadas",
        ecam_base,
        pci_scan.len()
    );
    for d in &pci_scan {
        println!(
            "    pci  {:02x}:{:02x}.{}  {:04x}:{:04x}  clase {:02x}:{:02x}:{:02x}",
            d.bus, d.slot, d.func, d.vendor_id, d.device_id, d.class, d.subclass, d.prog_if
        );
    }
    drivers::usb::init();
    if let Some(xhci) = drivers::usb::XHCI.lock().as_ref() {
        println!(
            "  pcie-xhci    controlador USB 3.0 activo en bus {} dev {} fn {}",
            xhci.pci_device.bus, xhci.pci_device.slot, xhci.pci_device.func
        );
        let ports = xhci.inspect_ports();
        let connected_ports: alloc::vec::Vec<_> = ports.iter().filter(|p| p.connected).collect();
        println!(
            "  roothub      {} puertos totales · {} dispositivos conectados",
            ports.len(),
            connected_ports.len()
        );
        for p in connected_ports {
            println!(
                "    puerto {}  conectado · velocidad: {}",
                p.port_number, p.speed_name
            );
        }
        for dev in &xhci.devices {
            let dev_type = if dev.is_hub {
                "hub USB"
            } else if dev.is_keyboard {
                "teclado USB HID"
            } else if dev.is_mouse {
                "raton/tablet USB HID"
            } else {
                "dispositivo USB HID"
            };
            let proto = if dev.is_hub {
                ""
            } else if dev.use_generic {
                " [report descriptor]"
            } else {
                " [boot]"
            };
            println!(
                "    usb-hid    slot {} · puerto {} · ruta {:#x} · {}{} (EP {})",
                dev.slot_id,
                dev.port,
                dev.route_string,
                dev_type,
                proto,
                dev.ep_int_dci / 2
            );
        }
    } else if let Some(x) = pci_scan.iter().find(|d| d.is_xhci_controller()) {
        // The controller is on the bus but did not come up. The usual cause on
        // `-M virt -kernel` (no firmware) is an unprogrammed BAR0.
        let bar0_unset = x.bars[0].memory_address().unwrap_or(0) == 0;
        if bar0_unset {
            println!(
                "  pcie-xhci    xHCI {:04x}:{:04x} presente pero con BAR0 sin asignar · omitido (arranque sin firmware)",
                x.vendor_id, x.device_id
            );
        } else {
            println!(
                "  pcie-xhci    xHCI {:04x}:{:04x} presente · la inicialización del controlador falló",
                x.vendor_id, x.device_id
            );
        }
    }

    println!();
    println!("controlador de interrupciones y temporizador");
    println!("  {}", arch::aarch64::dtb::firmware_summary());

    // Descubrimiento del GIC (versión + bases) antes de gic::init().
    // Prioridad: device tree → ACPI MADT (ruta Limine/UEFI) → GICv2 por defecto.
    use arch::aarch64::gic::GicVersion;
    let mut gic_configured = false;

    if let Some(info) = arch::aarch64::dtb::find_gic() {
        let v = if info.is_v3 {
            GicVersion::V3
        } else {
            GicVersion::V2
        };
        arch::aarch64::gic::set_version(
            v,
            Some(info.gicd_base as usize),
            Some(info.second_base as usize),
        );
        println!(
            "  gic          DTB · {} d={:#x} 2={:#x}",
            arch::aarch64::gic::version_name(),
            info.gicd_base,
            info.second_base
        );
        gic_configured = true;
    }

    #[cfg(feature = "limine")]
    if booted_via_limine {
        let rsdp_resp = crate::limine::RSDP_REQUEST.response;
        if !rsdp_resp.is_null() {
            let rsdp_ptr = unsafe { (*rsdp_resp).address };
            if let Some(m) = unsafe { acpi::find_table(rsdp_ptr, b"MCFG") }.map(acpi::parse_mcfg) {
                if let Some(a) = m.first() {
                    println!(
                        "  acpi         MCFG: ECAM {:#x} buses {}..{}",
                        a.base_address, a.start_bus, a.end_bus
                    );
                }
            }
            if let Some(g) =
                unsafe { acpi::find_table(rsdp_ptr, b"APIC") }.map(acpi::parse_madt_gic)
            {
                println!(
                    "  acpi         MADT: GIC{} d={:#x} r={:#x}",
                    if g.is_v3() { "v3" } else { "v2" },
                    g.gicd_base.unwrap_or(0),
                    g.gicr_base.unwrap_or(0)
                );
                // Sin DTB, la MADT es la única fuente fiable de la topología del
                // GIC (VirtualBox ARM64: GICv3 en 0xfcd3_0000 / 0xfcd4_0000).
                if !gic_configured && g.gicd_base.is_some() {
                    let v = if g.is_v3() {
                        GicVersion::V3
                    } else {
                        GicVersion::V2
                    };
                    arch::aarch64::gic::set_version(
                        v,
                        g.gicd_base.map(|b| b as usize),
                        g.gicr_base.map(|b| b as usize),
                    );
                    gic_configured = true;
                }
            }
        } else {
            println!("  acpi         Limine no proporcionó RSDP");
        }
    }

    if !gic_configured {
        println!("  gic          sin DTB/ACPI · asumiendo GICv2 en bases por defecto");
    }

    arch::aarch64::gic::init();
    {
        let (_, d, s) = arch::aarch64::gic::bases();
        println!(
            "  {:<12} distribuidor y cpu interface activos · d={:#x} 2={:#x}",
            arch::aarch64::gic::version_name(),
            d,
            s
        );
    }

    arch::aarch64::timer::init();
    arch::aarch64::gic::enable_peripheral_irqs();
    arch::aarch64::exceptions::enable_irq();
    println!("  daif         irq habilitadas · verificando fuente de temporizador...");

    let src = arch::aarch64::timer::verify_and_fallback();
    let current_ticks = arch::aarch64::timer::ticks();
    if current_ticks > 0 {
        println!(
            "  timer        {} · {} pulsos verificados ({} Hz)",
            arch::aarch64::timer::source_name(),
            current_ticks,
            arch::aarch64::timer::TICK_HZ
        );
    } else {
        let _ = src;
        println!(
            "  timer        sin pulsos (contador del sistema congelado) · uptime derivado de CNTPCT"
        );
    }

    println!();
    println!("cargador de ejecutables ELF64 e initramfs (T26.4)");
    let sample_elf = [
        0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0xb7, 0, 1, 0, 0, 0, 0,
        0, 0x40, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0,
        56, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x40,
        0, 0, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 120, 0, 0, 0, 0, 0, 0, 0, 120, 0, 0, 0, 0, 0, 0,
        0, 0, 16, 0, 0, 0, 0, 0, 0,
    ];
    match elf::parse_elf(&sample_elf) {
        Ok(info) => {
            println!(
                "  cargador     ELF64 verificado para AArch64 (entrada: {:#x}, segmentos: {})",
                info.entry, info.loadable_segments
            );
        }
        Err(e) => {
            println!("  error        fallo en cargador ELF64: {}", e);
        }
    }

    println!();
    println!("espacio de usuario (EL0) y llamadas al sistema (SVC)");
    let (user_entry, user_sp, arg0, arg1) =
        unsafe { arch::aarch64::syscall::setup_test_userspace() };
    println!("  transición   saltando a EL0 en {user_entry:#x} con sp {user_sp:#x}");
    let exit_code =
        unsafe { arch::aarch64::syscall::enter_user_mode(user_entry, user_sp, arg0, arg1) };
    println!(
        "  retorno      el programa EL0 finalizó limpiamente con código de salida {exit_code}"
    );

    // ── Sovereign interactive shell, libantos + antos-init (T26.5) ────────
    // The hand-crafted probe above only proves the EL0/SVC round trip works.
    // Everything below runs the *real* userspace binary — the same ELF the
    // x86_64 boot path spawns as PID 1 — this time built for AArch64 by
    // `build.rs` and loaded through the actual VFS instead of being poked
    // into memory by hand.
    println!();
    println!("initramfs y VFS (T26.5)");
    if let Ok(tfs) = fs::tarfs::TarFs::from_memory(EMBEDDED_INITRD) {
        println!(
            "  vfs          raíz montada · {} ficheros indexados",
            tfs.entry_count()
        );
        fs::vfs::mount_root(tfs);
    } else {
        println!("  vfs          no se pudo montar initramfs · shell no disponible");
    }

    println!();
    println!("shell interactivo soberano en espacio de usuario (T26.5)");
    if fs::vfs::is_mounted() {
        match fs::vfs::read_all("/bin/init") {
            Ok(shell_elf) => match unsafe { elf::load_aarch64(&shell_elf) } {
                Ok((shell_entry, shell_stack)) => {
                    println!("  cargador     shell en {shell_entry:#x} · pila {shell_stack:#x}");
                    println!("  consola      escribe en la serie: PID 1 atendiendo antos>");

                    if graphical_fb_active {
                        ui::compositor::set_desktop_active(true);
                        if let Some(c) = console::CONSOLE.lock().as_mut() {
                            ui::render_desktop(
                                c.framebuffer_mut(),
                                allocator::used(),
                                AARCH64_HEAP_SIZE,
                                arch::aarch64::timer::ticks(),
                            );
                        }
                    }

                    const DESKTOP_HANDOFF_CODE: u64 = 42;
                    let code = unsafe {
                        arch::aarch64::syscall::enter_user_mode(shell_entry, shell_stack, 4, 0)
                    };

                    if code == DESKTOP_HANDOFF_CODE {
                        println!("  shell        cedió el control al compositor gráfico");
                        if let Some(c) = console::CONSOLE.lock().as_mut() {
                            ui::render_desktop(
                                c.framebuffer_mut(),
                                allocator::used(),
                                AARCH64_HEAP_SIZE,
                                arch::aarch64::timer::ticks(),
                            );
                        }
                    } else {
                        println!("  shell        terminó con código {code} · volviendo al bucle del kernel");
                        println!();
                        println!("sistema operativo listo (AArch64 bare metal)");
                    }
                }
                Err(e) => println!("  cargador     fallo al cargar /bin/init: {e}"),
            },
            Err(_) => println!("  vfs          /bin/init no encontrado en el initramfs"),
        }
    }

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
    let regions = regions_from_bootloader_api(&boot_info.memory_regions);

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
                        drivers::storage::register_virtio_device(
                            "/dev/vda",
                            sectors,
                            512,
                            "VirtIO Block Device",
                        );
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
    drivers::storage::detect_and_init_storage(
        &pci_devices,
        &mut mapper,
        &mut frames,
        physical_offset,
    );

    if !virtio_blk_found {
        println!("  virtio-blk   no detectado en bus PCI · usando ramdisk en memoria");
    }

    if let Ok(ramdisk) = fs::ramdisk::Ramdisk::new(EMBEDDED_INITRD) {
        println!(
            "  initramfs    Live Ramdisk detectado ({} KiB, {} entradas, init: {})",
            ramdisk.size() / 1024,
            ramdisk.entry_count(),
            ramdisk.has_init()
        );
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
        println!(
            "  vfs          raíz montada · {} ficheros indexados:",
            tfs.entry_count()
        );
        for entry in tfs.all_entries() {
            let kind = if entry.is_dir { "DIR " } else { "FILE" };
            println!("    [{kind}] {:<18} ({:>6} B)", entry.path, entry.size);
        }
        fs::vfs::mount_root(tfs);
    } else {
        panic!("no se pudo montar el sistema de ficheros raíz (VFS)");
    }

    if let Ok(conf_str) = fs::vfs::read_to_string("/etc/antos.conf") {
        input::apply_config(&conf_str);
        println!(
            "  config       /etc/antos.conf cargado ({} B) · {}",
            conf_str.len(),
            input::settings_report()
        );
    }

    println!();
    println!("\x1b[1;35mespacio de usuario (cargador dinámico VFS)\x1b[0m");

    userspace::init();
    println!("  syscall      habilitado · el anillo 3 ya tiene por dónde entrar");

    // Desacoplamiento de include_bytes!: cargar dinámicamente desde /bin/init en VFS
    let init_bytes = fs::vfs::read_all("/bin/init").expect("no pude leer /bin/init desde el VFS");
    println!(
        "  cargador     ELF dinámico de {} KiB cargado desde VFS (/bin/init)",
        init_bytes.len() / 1024
    );

    // Verificar carga de segundo ejecutable ELF (/bin/worker) desde VFS
    let worker_bytes =
        fs::vfs::read_all("/bin/worker").expect("no pude leer /bin/worker desde el VFS");
    println!(
        "  segundo ELF  {} KiB verificado desde VFS (/bin/worker)",
        worker_bytes.len() / 1024
    );

    // SAFETY: el ELF lo hemos compilado nosotros y cargado desde el VFS
    let entry = unsafe { elf::load(&init_bytes, &mut mapper, &mut frames) }
        .expect("no pude cargar el programa de usuario desde VFS");
    // SAFETY: el rango de pila no lo usa nadie más.
    let user_stack = unsafe { userspace::map_user_stack(&mut mapper, &mut frames) }
        .expect("no pude mapear la pila de usuario");
    println!("  cargado      entrada {entry:#x} · pila {user_stack:#x}");

    // Separate user stack for process 2 (0x6000_0000)
    let user_stack_2 =
        unsafe { userspace::map_user_stack_at(0x6000_0000, &mut mapper, &mut frames) }
            .expect("no pude mapear pila para proceso 2");

    // Separate user stack for process 3: the interactive shell (T26.5)
    let user_stack_shell =
        unsafe { userspace::map_user_stack_at(0x5000_0000, &mut mapper, &mut frames) }
            .expect("no pude mapear pila para el shell");

    // Initialize global memory controller for dynamic syscalls (mmap, munmap, spawn)
    memory::init_memory_controller(mapper, frames);

    // ── Peripherals: PS/2 mouse + USB xHCI (T28.9) ──────────────────────
    println!();
    println!("\x1b[1;36mperiféricos (PS/2 ratón · USB xHCI)\x1b[0m");
    let mouse_proto = unsafe { drivers::ps2::init() };
    println!("  ps2-mouse    IRQ12 activo · protocolo {:?}", mouse_proto);

    drivers::usb::init();
    match drivers::usb::XHCI.lock().as_ref() {
        Some(x) => println!(
            "  usb-xhci     controlador activo en PCI {}:{}.{} · {} interfaz(es) HID",
            x.pci_device.bus,
            x.pci_device.slot,
            x.pci_device.func,
            x.devices.len()
        ),
        None => println!("  usb-xhci     sin controlador xHCI en el bus PCI (pila inactiva)"),
    }

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

    // Spawn Process 3: the sovereign interactive shell (T26.5), mode 4.
    // Unlike processes 1 and 2 it never exits — the scheduler keeps it in
    // its round-robin rotation for the lifetime of the machine, which is
    // exactly what makes it PID 1 in spirit: the process everything else
    // runs alongside, not one more demo that finishes and gets reaped.
    let (pid3, tid3) = task::scheduler::spawn_process(
        "antos-shell",
        cr3_root,
        entry,
        user_stack_shell,
        0, // auto-allocate kernel stack
        true,
        4, // mode 4: interactive shell
    );
    println!("  proceso 3    spawned PID {pid3} (TID {tid3}) · shell interactivo (PID 1 soberano)");

    let metrics = task::scheduler::metrics();
    println!(
        "  metricas     {} procesos, {} hilos en cola de listos",
        metrics.total_processes, metrics.ready_threads
    );

    println!();
    println!("\x1b[1;33mmultitarea cooperativa\x1b[0m");

    // `task::keyboard::keyboard_task` (its own kernel-side echo loop) is
    // intentionally not spawned here anymore: the shell process above now
    // owns real keyboard input via `SYS_READ` → `input::drain_ascii`, and
    // running both would consume the same keystrokes for two different,
    // confusing echoes. The task is kept in `task::keyboard` for reference.
    let mut executor = task::executor::Executor::new();
    executor.spawn(Task::new(example_task()));
    executor.spawn(Task::new(heartbeat_task()));
    println!("  2 tareas cooperativas encoladas · el ejecutor toma el control");
    println!("  shell        listo en la consola serie — PID {pid3} atendiendo antos>");
    println!();

    // No vuelve: a partir de aquí el kernel es su bucle de eventos.
    executor.run()
}

/// Generous enough for any memory map QEMU or real firmware hands back;
/// `bootloader_api`'s own regions and Limine's memmap entries both convert
/// into this fixed buffer since neither source's count is known at compile
/// time and the frame allocator needs it before the heap exists to allocate
/// anything bigger (T27.1).
#[cfg(target_arch = "x86_64")]
const MAX_MEMORY_REGIONS: usize = 64;

#[cfg(target_arch = "x86_64")]
static mut REGION_BUF: [memory::Region; MAX_MEMORY_REGIONS] = [memory::Region {
    start: 0,
    end: 0,
    usable: false,
}; MAX_MEMORY_REGIONS];

/// Converts the `bootloader` crate's own memory map into the
/// bootloader-agnostic `memory::Region` shape `FrameAllocator` now expects.
#[cfg(target_arch = "x86_64")]
fn regions_from_bootloader_api(regions: &MemoryRegions) -> &'static [memory::Region] {
    let count = regions.len().min(MAX_MEMORY_REGIONS);
    // SAFETY: called once, early in boot, before any other code touches
    // REGION_BUF.
    unsafe {
        let buf = &mut *core::ptr::addr_of_mut!(REGION_BUF);
        for (i, region) in regions.iter().take(count).enumerate() {
            buf[i] = memory::Region {
                start: region.start,
                end: region.end,
                usable: region.kind == MemoryRegionKind::Usable,
            };
        }
        &buf[..count]
    }
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
        #[cfg(target_arch = "aarch64")]
        {
            drivers::usb::poll();
            input::poll_rx_report();
            input::service_auto_repeat(arch::aarch64::timer::ticks());
            input::sync_keyboard_leds();
            let (screen_w, screen_h) = console::resolution();
            if (drivers::virtio_input::poll_virtio_inputs(screen_w, screen_h) > 0
                || input::has_events())
                && ui::dispatch_pending_inputs(screen_w, screen_h)
            {
                if let Some(c) = console::CONSOLE.lock().as_mut() {
                    ui::render_desktop(
                        c.framebuffer_mut(),
                        allocator::used(),
                        AARCH64_HEAP_SIZE,
                        arch::aarch64::timer::ticks(),
                    );
                }
            }
        }
        crate::arch::current::Interrupts::halt();
    }
}

/// Lo que se ejecuta tras un panic. Ya no se limita a parar la máquina:
/// ahora dice qué pasó y dónde, que es la mitad del trabajo de depurar.
#[cfg(not(test))]
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
            let _ = writeln!(
                console,
                "\n\x1b[1;31m╔══════════════════════════════\x1b[0m"
            );
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
