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
/// En los niveles 2 y 3 significa «aquí acaba el recorrido»: la entrada apunta
/// directamente a una página de 2 MiB o 1 GiB en vez de a otra tabla.
const HUGE: u64 = 1 << 7;

/// Los bits 12-51 de una entrada son la dirección física. El resto son
/// banderas, y hay que enmascararlos para quedarse con la dirección.
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

pub struct Mapper {
    physical_offset: u64,
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

    /// La tabla de nivel 4 en uso, según CR3.
    fn level4_table(&self) -> u64 {
        let cr3: u64;
        // SAFETY: leer CR3 no tiene efectos secundarios.
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack)) };
        cr3 & ADDRESS_MASK
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
                    *entry = new_table | PRESENT | WRITABLE;
                }
                table = new_table;
            } else if value & HUGE != 0 {
                return Err("la ruta atraviesa una página enorme");
            } else {
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
        // entrada deja a la CPU usando la traducción vieja — un bug que
        // aparece "a veces", que son los peores.
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) virtual_address, options(nostack, preserves_flags))
        };
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

    pub fn frames_handed_out(&self) -> usize {
        self.next
    }
}

// ------------------------------------------------------------------- heap

/// Una dirección virtual cualquiera, lejos de donde el bootloader mapeó nada.
/// No hace falta que se corresponda con memoria física contigua: para eso
/// existe la paginación.
pub const HEAP_START: u64 = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 512 * 1024;

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
