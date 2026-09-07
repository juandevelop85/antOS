//! Cargador y parser de ejecutables ELF64 para antOS.
//!
//! Soporta binarios de 64 bits para arquitecturas x86_64 y AArch64,
//! validación de cabeceras, asignación y protección de memoria virtual
//! por segmentos (`PT_LOAD` con permisos RX / RW / NX), inicialización de
//! la sección BSS, y preparación de la pila de usuario conforme a System V ABI.

#[cfg(target_arch = "x86_64")]
use crate::memory::{FrameAllocator, Mapper, NO_EXECUTE, PAGE_SIZE, PRESENT, USER, WRITABLE};

pub const MAGIC: &[u8; 4] = b"\x7fELF";
pub const CLASS_64: u8 = 2;
pub const DATA_2LSB: u8 = 1; // Little endian

pub const MACHINE_X86_64: u16 = 0x3E;
pub const MACHINE_AARCH64: u16 = 0xB7;

pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;

pub const PT_LOAD: u32 = 1;

pub const PF_X: u32 = 1; // Execute
pub const PF_W: u32 = 2; // Write
pub const PF_R: u32 = 4; // Read

/// Cabecera ELF64 tal cual la define el estándar System V.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub identification: [u8; 16],
    pub file_type: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub program_header_offset: u64,
    pub section_header_offset: u64,
    pub flags: u32,
    pub header_size: u16,
    pub program_header_size: u16,
    pub program_header_count: u16,
    pub section_header_size: u16,
    pub section_header_count: u16,
    pub section_name_index: u16,
}

/// Cabecera de programa ELF64 (Elf64_Phdr).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgramHeader {
    pub segment_type: u32,
    pub flags: u32,
    pub offset: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub alignment: u64,
}

/// Metadatos resumidos de un binario ELF64 parseado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfInfo {
    pub entry: u64,
    pub machine: u16,
    pub is_pie: bool,
    pub min_vaddr: u64,
    pub max_vaddr: u64,
    pub total_memsz: u64,
    pub loadable_segments: usize,
}

/// Valida y parsea la imagen de un ejecutable ELF64 sin requerir asignaciones en heap.
pub fn parse_elf(image: &[u8]) -> Result<ElfInfo, &'static str> {
    if image.len() < core::mem::size_of::<Header>() {
        return Err("imagen demasiado pequeña para ser un ELF");
    }

    let header: Header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Header) };

    if &header.identification[0..4] != MAGIC {
        return Err("número mágico ELF inválido");
    }
    if header.identification[4] != CLASS_64 {
        return Err("no es un binario ELF de 64 bits");
    }
    if header.identification[5] != DATA_2LSB {
        return Err("no es Little Endian");
    }
    if header.machine != MACHINE_X86_64 && header.machine != MACHINE_AARCH64 {
        return Err("arquitectura de máquina no soportada");
    }

    let ph_offset = header.program_header_offset as usize;
    let ph_size = header.program_header_size as usize;
    let ph_count = header.program_header_count as usize;

    if ph_count > 0 && ph_offset + ph_count * ph_size > image.len() {
        return Err("tabla de cabeceras de programa truncada");
    }

    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;
    let mut loadable_count = 0;

    for i in 0..ph_count {
        let entry_offset = ph_offset + i * ph_size;
        if entry_offset + core::mem::size_of::<ProgramHeader>() > image.len() {
            return Err("cabecera de programa desbordada");
        }

        let ph: ProgramHeader = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(entry_offset) as *const ProgramHeader)
        };

        if ph.segment_type == PT_LOAD {
            loadable_count += 1;
            let file_off = ph.offset as usize;
            let file_sz = ph.file_size as usize;

            if file_off + file_sz > image.len() {
                return Err("segmento PT_LOAD desborda la imagen del archivo");
            }

            let start = ph.virtual_address;
            let end = start.saturating_add(ph.memory_size);

            if start < min_vaddr {
                min_vaddr = start;
            }
            if end > max_vaddr {
                max_vaddr = end;
            }
        }
    }

    if loadable_count == 0 {
        return Err("el binario no contiene segmentos cargables (PT_LOAD)");
    }

    Ok(ElfInfo {
        entry: header.entry,
        machine: header.machine,
        is_pie: header.file_type == ET_DYN,
        min_vaddr,
        max_vaddr,
        total_memsz: max_vaddr.saturating_sub(min_vaddr),
        loadable_segments: loadable_count,
    })
}

/// Extrae el punto de entrada de una imagen ELF válida si la arquitectura coincide.
pub fn parse_entry(image: &[u8]) -> Option<u64> {
    parse_elf(image).ok().map(|info| info.entry)
}

/// Carga los segmentos de un ELF64 en el espacio de usuario (x86_64) con permisos estrictos de página.
#[cfg(target_arch = "x86_64")]
pub unsafe fn load(
    image: &[u8],
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    let info = parse_elf(image)?;
    if info.machine != MACHINE_X86_64 {
        return Err("el binario ELF no es para la arquitectura x86_64");
    }

    let header: Header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Header) };
    let ph_offset = header.program_header_offset as usize;
    let ph_size = header.program_header_size as usize;
    let ph_count = header.program_header_count as usize;

    for index in 0..ph_count {
        let offset = ph_offset + index * ph_size;
        let segment: ProgramHeader =
            unsafe { core::ptr::read_unaligned(image.as_ptr().add(offset) as *const ProgramHeader) };

        if segment.segment_type != PT_LOAD {
            continue;
        }
        unsafe { load_segment(image, &segment, mapper, allocator)? };
    }

    Ok(header.entry)
}

#[cfg(target_arch = "x86_64")]
unsafe fn load_segment(
    image: &[u8],
    segment: &ProgramHeader,
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<(), &'static str> {
    let start = segment.virtual_address;
    let end = start + segment.memory_size;

    let first_page = start / PAGE_SIZE * PAGE_SIZE;
    let last_page = (end - 1) / PAGE_SIZE * PAGE_SIZE;

    // 1. Mapear inicialmente como WRITABLE para copiar contenido y limpiar BSS
    let mut page = first_page;
    while page <= last_page {
        if mapper.translate(page).is_none() {
            let frame = allocator.allocate().ok_or("sin marcos físicos para el programa")?;
            unsafe {
                mapper.map(page, frame, PRESENT | WRITABLE | USER, allocator)?;
                core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize);
            }
        }
        page += PAGE_SIZE;
    }

    // 2. Copiar los bytes del archivo al espacio virtual
    let from = segment.offset as usize;
    let length = segment.file_size as usize;
    if from + length > image.len() {
        return Err("segmento fuera de la imagen");
    }
    unsafe {
        core::ptr::copy_nonoverlapping(image.as_ptr().add(from), start as *mut u8, length);
    }

    // 3. Ajustar permisos de protección estrictos de página según flags del segmento
    // PF_R | PF_X -> Solo Lectura / Ejecutable (código .text)
    // PF_R | PF_W -> Lectura / Escritura / No Ejecutable (datos .data / .bss)
    let is_writable = (segment.flags & PF_W) != 0;
    let is_executable = (segment.flags & PF_X) != 0;

    let mut final_flags = PRESENT | USER;
    if is_writable {
        final_flags |= WRITABLE;
    }
    if !is_executable {
        final_flags |= NO_EXECUTE;
    }

    let mut page = first_page;
    while page <= last_page {
        unsafe {
            let _ = mapper.update_flags(page, final_flags);
        }
        page += PAGE_SIZE;
    }

    Ok(())
}

/// Carga un binario ELF64 en AArch64 dentro del área de usuario de 2 MiB.
#[cfg(target_arch = "aarch64")]
pub unsafe fn load_aarch64(image: &[u8]) -> Result<(u64, u64), &'static str> {
    let info = parse_elf(image)?;
    if info.machine != MACHINE_AARCH64 {
        return Err("el binario ELF no es para la arquitectura AArch64");
    }

    let virt_base = crate::arch::aarch64::mmu::USER_SPACE_VIRT;
    let phys_base = crate::arch::aarch64::mmu::USER_SPACE_PHYS;
    let user_limit = virt_base + 0x0020_0000; // 2 MiB

    if info.min_vaddr < virt_base || info.max_vaddr > user_limit {
        return Err("el binario excede el rango de espacio de usuario AArch64");
    }

    let header: Header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Header) };
    let ph_offset = header.program_header_offset as usize;
    let ph_size = header.program_header_size as usize;
    let ph_count = header.program_header_count as usize;

    for i in 0..ph_count {
        let entry_offset = ph_offset + i * ph_size;
        let ph: ProgramHeader = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(entry_offset) as *const ProgramHeader)
        };

        if ph.segment_type == PT_LOAD {
            let vaddr = ph.virtual_address;
            let offset_in_user = (vaddr - virt_base) as usize;
            let paddr = (phys_base as usize + offset_in_user) as *mut u8;

            let file_off = ph.offset as usize;
            let file_sz = ph.file_size as usize;
            let mem_sz = ph.memory_size as usize;

            // Limpiar con ceros toda la extensión en memoria
            core::ptr::write_bytes(paddr, 0, mem_sz);

            // Copiar datos del archivo
            if file_sz > 0 {
                core::ptr::copy_nonoverlapping(image.as_ptr().add(file_off), paddr, file_sz);
            }

            // Limpiar caché de datos y refrescar caché de instrucciones para código cargado
            core::arch::asm!(
                "dc cvau, {dst}",
                "dsb ish",
                "ic iallu",
                "dsb ish",
                "isb",
                dst = in(reg) paddr,
                options(nostack)
            );
        }
    }

    // Pila de usuario ubicada al final del área de usuario con página de guarda
    let stack_top = user_limit - 4096;

    Ok((header.entry, stack_top))
}
