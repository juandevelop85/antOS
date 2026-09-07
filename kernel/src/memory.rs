//! Paginación de 4 niveles y asignación de marcos físicos.
//!
//! ## El problema del huevo y la gallina
//!
//! Las tablas de páginas guardan direcciones **físicas**, pero la CPU, con la
//! paginación ya activa, solo sabe hablar de direcciones **virtuales**. Para
//! leer una tabla de páginas hace falta una dirección virtual que apunte a
//! ella... lo cual requiere consultar las tablas de páginas.
//!
//! La salida es pedirle al bootloader que mapee TODA la memoria física en un
//! rango virtual contiguo antes de saltar aquí. Con eso, la física `p` se lee
//! en la virtual `p + offset`, y el círculo se rompe. Ese offset es el
//! `physical_memory_offset` que las fases 1 y 2 llevaban dos versiones
//! reportando como «sin mapear».
//!
//! ## Cómo se traduce una dirección
//!
//! Los 48 bits útiles de una dirección virtual se trocean en cuatro índices
//! de 9 bits y un desplazamiento de 12:
//!
//! ```text
//!   47        39 38        30 29        21 20        12 11         0
//!  ┌────────────┬────────────┬────────────┬────────────┬────────────┐
//!  │  nivel 4   │  nivel 3   │  nivel 2   │  nivel 1   │ desplaz.   │
//!  └────────────┴────────────┴────────────┴────────────┴────────────┘
//! ```
//!
//! Cada índice selecciona una de las 512 entradas de una tabla, y cada
//! entrada apunta a la tabla del nivel siguiente. Cuatro saltos para traducir
//! una dirección — de ahí que exista la TLB, la caché que evita repetirlos.

use crate::println;
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};

pub const PAGE_SIZE: u64 = 4096;

// Banderas de una entrada de tabla de páginas.
pub const PRESENT: u64 = 1 << 0;
pub const WRITABLE: u64 = 1 << 1;
/// Sin este bit, el anillo 3 no puede ni mirar la página. Es lo único que
/// separa la memoria del kernel de la del usuario — y lo comprueba la MMU en
/// cada acceso, no nosotros.
pub const USER: u64 = 1 << 2;
/// En los niveles 2 y 3 significa «aquí acaba el recorrido»: la entrada apunta
/// directamente a una página de 2 MiB o 1 GiB en vez de a otra tabla.
const HUGE: u64 = 1 << 7;
/// Bit de no ejecución (NX / XD) en x86_64. Si está activo, saltar a esta
/// página genera un fallo de protección de página.
pub const NO_EXECUTE: u64 = 1 << 63;

/// Los bits 12-51 de una entrada son la dirección física. El resto son
/// banderas, y hay que enmascararlos para quedarse con la dirección.
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

pub struct Mapper {
    pub physical_offset: u64,
}

impl Mapper {
    /// # Safety
    /// `physical_offset` debe ser el offset donde el bootloader mapeó toda la
    /// memoria física. Si es falso, todo lo demás lee memoria arbitraria.
    pub unsafe fn new(physical_offset: u64) -> Self {
        Mapper { physical_offset }
    }

    /// Dirección virtual desde la que se lee un marco físico.
    fn virtual_of(&self, physical: u64) -> u64 {
        physical + self.physical_offset
    }

    /// Puntero a una entrada concreta de una tabla que vive en `table`.
    ///
    /// # Safety
    /// `table` debe ser la dirección física de una tabla de páginas viva.
    unsafe fn entry(&self, table: u64, index: usize) -> *mut u64 {
        unsafe { (self.virtual_of(table) as *mut u64).add(index) }
    }

    /// La tabla de nivel raíz en uso, obtenida a través de la capa de abstracción HAL.
    fn level4_table(&self) -> u64 {
        use crate::arch::traits::ArchMmu;
        crate::arch::current::mmu::CurrentMmu::read_root_table()
    }

    /// Recorre las tablas y devuelve la dirección física a la que apunta una
    /// virtual, o None si no está mapeada.
    pub fn translate(&self, virtual_address: u64) -> Option<u64> {
        let mut table = self.level4_table();

        for level in (1..=4u32).rev() {
            let index = table_index(virtual_address, level);
            // SAFETY: `table` viene de CR3 o de una entrada presente.
            let entry = unsafe { *self.entry(table, index) };

            if entry & PRESENT == 0 {
                return None;
            }
            table = entry & ADDRESS_MASK;

            // Una página enorme corta el recorrido antes de tiempo: lo que
            // queda de la dirección es todo desplazamiento.
            if level > 1 && entry & HUGE != 0 {
                let offset_bits = 12 + 9 * (level - 1);
                let offset_mask = (1u64 << offset_bits) - 1;
                return Some(table + (virtual_address & offset_mask));
            }
        }

        Some(table + (virtual_address & 0xFFF))
    }

    /// Mapea una página virtual a un marco físico, creando por el camino las
    /// tablas intermedias que falten.
    ///
    /// # Safety
    /// Mapear una página cambia lo que significa una dirección. Si `frame` ya
    /// está en uso para otra cosa, se crea aliasing invisible para Rust.
    pub unsafe fn map(
        &mut self,
        virtual_address: u64,
        frame: u64,
        flags: u64,
        allocator: &mut FrameAllocator,
    ) -> Result<(), &'static str> {
        let mut table = self.level4_table();

        // Niveles 4, 3 y 2: bajar, creando tablas si hace falta.
        for level in (2..=4u32).rev() {
            let index = table_index(virtual_address, level);
            let entry = unsafe { self.entry(table, index) };
            let value = unsafe { *entry };

            if value & PRESENT == 0 {
                let new_table = allocator.allocate().ok_or("no quedan marcos libres")?;
                // Una tabla nueva con basura dentro son 512 mapeos aleatorios.
                unsafe {
                    core::ptr::write_bytes(
                        self.virtual_of(new_table) as *mut u8,
                        0,
                        PAGE_SIZE as usize,
                    );
                    // Los permisos se acumulan a la baja: la CPU exige el
                    // bit de usuario en TODOS los niveles del camino, así que
                    // hay que propagarlo hacia arriba.
                    *entry = new_table | PRESENT | WRITABLE | (flags & USER);
                }
                table = new_table;
            } else if value & HUGE != 0 {
                return Err("la ruta atraviesa una página enorme");
            } else {
                // Una tabla intermedia que ya existía puede haberse creado sin
                // el bit de usuario. Abrirlo aquí no da acceso a nada por sí
                // solo: la entrada final sigue mandando.
                if flags & USER != 0 && value & USER == 0 {
                    unsafe { *entry = value | USER };
                }
                table = value & ADDRESS_MASK;
            }
        }

        // Nivel 1: la entrada que de verdad apunta al marco.
        let index = table_index(virtual_address, 1);
        let entry = unsafe { self.entry(table, index) };
        if unsafe { *entry } & PRESENT != 0 {
            return Err("esa página ya estaba mapeada");
        }
        unsafe { *entry = frame | flags | PRESENT };

        // La TLB cachea traducciones. Cambiar una tabla sin invalidar su
        // entrada deja a la CPU usando la traducción vieja.
        use crate::arch::traits::ArchMmu;
        crate::arch::current::mmu::CurrentMmu::flush_tlb(virtual_address);
        Ok(())
    }

    /// Desmapea una página virtual de nivel 1 e invalida la entrada TLB.
    pub unsafe fn unmap(&mut self, virtual_address: u64) -> Result<u64, &'static str> {
        let mut table = self.level4_table();
        for level in (2..=4u32).rev() {
            let index = table_index(virtual_address, level);
            let entry = unsafe { *self.entry(table, index) };
            if entry & PRESENT == 0 {
                return Err("page not mapped");
            }
            if level > 1 && entry & HUGE != 0 {
                return Err("huge page unmap unsupported");
            }
            table = entry & ADDRESS_MASK;
        }

        let index = table_index(virtual_address, 1);
        let entry_ptr = unsafe { self.entry(table, index) };
        let entry_val = unsafe { *entry_ptr };
        if entry_val & PRESENT == 0 {
            return Err("page not mapped");
        }
        unsafe { *entry_ptr = 0 };

        use crate::arch::traits::ArchMmu;
        crate::arch::current::mmu::CurrentMmu::flush_tlb(virtual_address);
        Ok(entry_val & ADDRESS_MASK)
    }

    /// Actualiza los bits de protección (flags) de una página virtual previamente mapeada.
    pub unsafe fn update_flags(&mut self, virtual_address: u64, flags: u64) -> Result<(), &'static str> {
        let mut table = self.level4_table();
        for level in (2..=4u32).rev() {
            let index = table_index(virtual_address, level);
            let entry = unsafe { *self.entry(table, index) };
            if entry & PRESENT == 0 {
                return Err("page not mapped");
            }
            table = entry & ADDRESS_MASK;
        }

        let index = table_index(virtual_address, 1);
        let entry_ptr = unsafe { self.entry(table, index) };
        let entry_val = unsafe { *entry_ptr };
        if entry_val & PRESENT == 0 {
            return Err("page not mapped");
        }
        let frame = entry_val & ADDRESS_MASK;
        unsafe { *entry_ptr = frame | flags | PRESENT };

        use crate::arch::traits::ArchMmu;
        crate::arch::current::mmu::CurrentMmu::flush_tlb(virtual_address);
        Ok(())
    }
}

/// Extrae el índice de 9 bits que corresponde a un nivel.
fn table_index(virtual_address: u64, level: u32) -> usize {
    ((virtual_address >> (12 + 9 * (level - 1))) & 0x1FF) as usize
}

/// Reparte marcos físicos de 4 KiB a partir del mapa de memoria que dejó el
/// bootloader.
///
/// No sabe liberar. Es deliberado: solo se usa durante el arranque, para las
/// tablas de páginas y las páginas del heap, y nada de eso se devuelve jamás.
/// Un asignador de marcos completo —con mapa de bits o listas de bloques—
/// hace falta cuando existan procesos que nazcan y mueran.
pub struct FrameAllocator {
    regions: &'static MemoryRegions,
    next: usize,
}

impl FrameAllocator {
    /// # Safety
    /// Las regiones marcadas como utilizables deben serlo de verdad: entregar
    /// un marco que ya está en uso corrompe lo que hubiera allí.
    pub unsafe fn new(regions: &'static MemoryRegions) -> Self {
        FrameAllocator { regions, next: 0 }
    }

    fn usable_frames(&self) -> impl Iterator<Item = u64> + '_ {
        self.regions
            .iter()
            .filter(|region| region.kind == MemoryRegionKind::Usable)
            .flat_map(|region| {
                // Los extremos de una región no tienen por qué caer en un
                // límite de página; se recorta hacia dentro.
                let start = region.start.div_ceil(PAGE_SIZE) * PAGE_SIZE;
                let end = region.end / PAGE_SIZE * PAGE_SIZE;
                (start..end).step_by(PAGE_SIZE as usize)
            })
    }

    pub fn allocate(&mut self) -> Option<u64> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }

    /// Allocates `count` contiguous physical frames (4 KiB each).
    pub fn allocate_contiguous(&mut self, count: usize) -> Option<u64> {
        if count == 0 {
            return None;
        }
        let first = self.usable_frames().nth(self.next)?;
        for i in 1..count {
            let next_frame = self.usable_frames().nth(self.next + i)?;
            if next_frame != first + (i as u64) * PAGE_SIZE {
                self.next += 1;
                return self.allocate_contiguous(count);
            }
        }
        self.next += count;
        Some(first)
    }

    pub fn frames_handed_out(&self) -> usize {
        self.next
    }
}

// ------------------------------------------------------------------- heap

/// Una dirección virtual cualquiera, lejos de donde el bootloader mapeó nada.
/// No hace falta que se corresponda con memoria física contigua: para eso
/// existe la paginación.
pub const HEAP_START: u64 = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 4 * 1024 * 1024;

/// Reserva marcos y los mapea al rango virtual del heap.
///
/// # Safety
/// Solo puede llamarse una vez, antes de que exista ninguna asignación.
pub unsafe fn init_heap(mapper: &mut Mapper, allocator: &mut FrameAllocator) -> Result<(), &'static str> {
    let pages = HEAP_SIZE as u64 / PAGE_SIZE;

    for page in 0..pages {
        let virtual_address = HEAP_START + page * PAGE_SIZE;
        let frame = allocator.allocate().ok_or("no hay marcos para el heap")?;
        unsafe { mapper.map(virtual_address, frame, PRESENT | WRITABLE, allocator)? };
    }

    println!(
        "  heap         {} KiB mapeadas en {:#x} ({} páginas)",
        HEAP_SIZE / 1024,
        HEAP_START,
        pages
    );
    Ok(())
}

// ----------------------------------------------------------- global memory controller

/// Global controller managing page tables and physical frame allocations.
pub struct MemoryController {
    pub mapper: Mapper,
    pub allocator: FrameAllocator,
}

unsafe impl Send for MemoryController {}
unsafe impl Sync for MemoryController {}

pub static MEMORY_CONTROLLER: crate::sync::SpinLock<Option<MemoryController>> =
    crate::sync::SpinLock::new(None);

/// Initializes the global memory controller with the active mapper and frame allocator.
pub fn init_memory_controller(mapper: Mapper, allocator: FrameAllocator) {
    *MEMORY_CONTROLLER.lock() = Some(MemoryController { mapper, allocator });
}

/// Allocates physical frames and maps contiguous user pages starting at `start_vaddr`.
pub fn mmap_user_pages(start_vaddr: u64, page_count: usize) -> Result<u64, u64> {
    let mut guard = MEMORY_CONTROLLER.lock();
    let Some(controller) = guard.as_mut() else {
        return Err(crate::syscall::ENOMEM);
    };

    for i in 0..page_count {
        let vaddr = start_vaddr + (i as u64) * PAGE_SIZE;
        let Some(frame) = controller.allocator.allocate() else {
            return Err(crate::syscall::ENOMEM);
        };
        // Zero physical page content
        unsafe {
            let virt = (frame + controller.mapper.physical_offset) as *mut u8;
            core::ptr::write_bytes(virt, 0, PAGE_SIZE as usize);
        }
        if unsafe {
            controller
                .mapper
                .map(vaddr, frame, PRESENT | WRITABLE | USER, &mut controller.allocator)
        }
        .is_err()
        {
            return Err(crate::syscall::ENOMEM);
        }
    }
    Ok(start_vaddr)
}

/// Unmaps user pages starting at `start_vaddr`.
pub fn munmap_user_pages(start_vaddr: u64, page_count: usize) -> Result<(), u64> {
    let mut guard = MEMORY_CONTROLLER.lock();
    let Some(controller) = guard.as_mut() else {
        return Err(crate::syscall::EINVAL);
    };

    for i in 0..page_count {
        let vaddr = start_vaddr + (i as u64) * PAGE_SIZE;
        let _ = unsafe { controller.mapper.unmap(vaddr) };
    }
    Ok(())
}

/// Returns the physical memory mapping offset used by the kernel.
pub fn physical_memory_offset() -> u64 {
    MEMORY_CONTROLLER.lock().as_ref().map(|c| c.mapper.physical_offset).unwrap_or(0)
}

/// Translates a physical address to the higher-half virtual address.
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + physical_memory_offset()
}

/// Allocates a single 4 KiB frame for DMA and returns (physical_address, virtual_address).
/// The page is initialized with zeros.
pub fn allocate_dma_frame() -> Option<(u64, u64)> {
    let mut guard = MEMORY_CONTROLLER.lock();
    let controller = guard.as_mut()?;
    let frame = controller.allocator.allocate()?;
    let virt = frame + controller.mapper.physical_offset;
    unsafe {
        core::ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE as usize);
    }
    Some((frame, virt))
}

/// Allocates `count` contiguous 4 KiB frames for DMA and returns (physical_address, virtual_address).
/// The pages are initialized with zeros.
pub fn allocate_dma_frames(count: usize) -> Option<(u64, u64)> {
    if count == 0 {
        return None;
    }
    let mut guard = MEMORY_CONTROLLER.lock();
    let controller = guard.as_mut()?;
    let frame = controller.allocator.allocate_contiguous(count)?;
    let virt = frame + controller.mapper.physical_offset;
    unsafe {
        core::ptr::write_bytes(virt as *mut u8, 0, count * PAGE_SIZE as usize);
    }
    Some((frame, virt))
}

/// Ensures that a physical MMIO memory range is mapped in the virtual address space.
/// Returns the virtual address pointing to `phys_start`.
pub fn ensure_mmio_mapped(phys_start: u64, size: usize) -> Result<u64, &'static str> {
    let mut guard = MEMORY_CONTROLLER.lock();
    let controller = guard.as_mut().ok_or("Memory controller uninitialized")?;
    let phys_offset = controller.mapper.physical_offset;
    let virt_start = phys_start + phys_offset;
    let num_pages = (size as u64).div_ceil(PAGE_SIZE);

    for i in 0..num_pages {
        let p_addr = (phys_start & !0xFFF) + i * PAGE_SIZE;
        let v_addr = (virt_start & !0xFFF) + i * PAGE_SIZE;
        if controller.mapper.translate(v_addr).is_none() {
            unsafe {
                controller.mapper.map(v_addr, p_addr, PRESENT | WRITABLE, &mut controller.allocator)?;
            }
        }
    }
    Ok(virt_start)
}

