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

/// Computes `offset + len`, checked, and validates the resulting range fits
/// inside a buffer of `total_len` bytes (T31.3).
///
/// Every bounds check in this module goes through this helper instead of a
/// raw `+`: on a `release` profile (no `overflow-checks`) a bare addition
/// wraps silently, so an `offset`/`len` pair near `usize::MAX` — both are
/// widened from attacker-controlled `u16`/`u64` header fields — could pass a
/// hand-written `offset + len > total_len` check and let `load`/`load_aarch64`
/// copy from outside the image.
fn checked_bounds(
    offset: usize,
    len: usize,
    total_len: usize,
    overflow_msg: &'static str,
    out_of_bounds_msg: &'static str,
) -> Result<usize, &'static str> {
    let end = offset.checked_add(len).ok_or(overflow_msg)?;
    if end > total_len {
        return Err(out_of_bounds_msg);
    }
    Ok(end)
}

/// Computes the byte offset of program header `index` within the program
/// header table, checked (T31.3). Used both by `parse_elf` and by every
/// loader that re-walks the table on its own, so none of them depend on a
/// remote validation having already run.
fn checked_program_header_offset(
    ph_offset: usize,
    ph_size: usize,
    index: usize,
) -> Result<usize, &'static str> {
    let stride = index
        .checked_mul(ph_size)
        .ok_or("desbordamiento al calcular el desplazamiento de una cabecera de programa")?;
    ph_offset
        .checked_add(stride)
        .ok_or("desbordamiento al calcular el desplazamiento de una cabecera de programa")
}

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

    if ph_count > 0 {
        let table_len = ph_count
            .checked_mul(ph_size)
            .ok_or("desbordamiento en el tamaño de la tabla de cabeceras de programa")?;
        checked_bounds(
            ph_offset,
            table_len,
            image.len(),
            "desbordamiento en el tamaño de la tabla de cabeceras de programa",
            "tabla de cabeceras de programa truncada",
        )?;
    }

    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;
    let mut loadable_count = 0;

    for i in 0..ph_count {
        let entry_offset = checked_program_header_offset(ph_offset, ph_size, i)?;
        checked_bounds(
            entry_offset,
            core::mem::size_of::<ProgramHeader>(),
            image.len(),
            "desbordamiento al calcular el desplazamiento de una cabecera de programa",
            "cabecera de programa desbordada",
        )?;

        let ph: ProgramHeader = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(entry_offset) as *const ProgramHeader)
        };

        if ph.segment_type == PT_LOAD {
            loadable_count += 1;
            let file_off = ph.offset as usize;
            let file_sz = ph.file_size as usize;

            checked_bounds(
                file_off,
                file_sz,
                image.len(),
                "desbordamiento en el tamaño del segmento PT_LOAD",
                "segmento PT_LOAD desborda la imagen del archivo",
            )?;
            if ph.file_size > ph.memory_size {
                return Err(
                    "el tamaño en archivo del segmento PT_LOAD excede su tamaño en memoria",
                );
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
        // Re-validated locally (T31.3): `load` is `unsafe` and must not rely
        // solely on `parse_elf`'s earlier pass over these same raw bytes.
        let offset = checked_program_header_offset(ph_offset, ph_size, index)?;
        checked_bounds(
            offset,
            core::mem::size_of::<ProgramHeader>(),
            image.len(),
            "desbordamiento al calcular el desplazamiento de una cabecera de programa",
            "cabecera de programa desbordada",
        )?;
        let segment: ProgramHeader = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(offset) as *const ProgramHeader)
        };

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
    if segment.memory_size == 0 {
        return Err("segmento PT_LOAD con tamaño en memoria nulo");
    }
    let end = start
        .checked_add(segment.memory_size)
        .ok_or("desbordamiento al calcular el fin del segmento en memoria")?;

    let first_page = start / PAGE_SIZE * PAGE_SIZE;
    let last_page = (end - 1) / PAGE_SIZE * PAGE_SIZE;

    // 1. Mapear inicialmente como WRITABLE para copiar contenido y limpiar BSS
    let mut page = first_page;
    while page <= last_page {
        if mapper.translate(page).is_none() {
            let frame = allocator
                .allocate()
                .ok_or("sin marcos físicos para el programa")?;
            unsafe {
                mapper.map(page, frame, PRESENT | WRITABLE | USER, allocator)?;
                core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize);
            }
        }
        page += PAGE_SIZE;
    }

    // 2. Copiar los bytes del archivo al espacio virtual
    // Re-validated locally (T31.3): `load_segment` is `unsafe` and must not
    // depend on `parse_elf`'s earlier, separate pass over these bytes.
    let from = segment.offset as usize;
    let length = segment.file_size as usize;
    checked_bounds(
        from,
        length,
        image.len(),
        "desbordamiento al validar el segmento dentro de la imagen",
        "segmento fuera de la imagen",
    )?;
    if segment.file_size > segment.memory_size {
        // Otherwise this copy would write past the zeroed-and-mapped
        // `memory_size` region validated above, into whatever page happens
        // to follow it.
        return Err("el tamaño en archivo del segmento excede su tamaño en memoria");
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

/// Reads `CTR_EL0.DminLine` and returns the smallest data cache line size in
/// bytes for this core (T31.3).
///
/// Cache maintenance instructions like `dc cvau` operate on exactly one line
/// at a time — a single instruction only ever cleans the line containing the
/// address it was given, never a whole range. `load_aarch64` used to issue
/// just one `dc cvau` per segment regardless of the segment's size; on real
/// hardware (QEMU's TCG backend doesn't model the incoherence) any segment
/// larger than one cache line left stale data visible to the instruction
/// cache beyond that first line.
#[cfg(target_arch = "aarch64")]
fn dcache_line_size() -> usize {
    let ctr: u64;
    unsafe {
        core::arch::asm!("mrs {}, ctr_el0", out(reg) ctr, options(nomem, nostack));
    }
    let dminline = (ctr >> 16) & 0xF;
    4usize << dminline
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
    let user_area_len = (user_limit - virt_base) as usize;

    if info.min_vaddr < virt_base || info.max_vaddr > user_limit {
        return Err("el binario excede el rango de espacio de usuario AArch64");
    }

    let header: Header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Header) };
    let ph_offset = header.program_header_offset as usize;
    let ph_size = header.program_header_size as usize;
    let ph_count = header.program_header_count as usize;
    let line_size = dcache_line_size().max(1);

    for i in 0..ph_count {
        // Re-validated locally (T31.3): `load_aarch64` is `unsafe` and must
        // not depend on `parse_elf`'s earlier, separate pass over these
        // bytes — including the file/memory bounds it never re-checked here
        // before this fix.
        let entry_offset = checked_program_header_offset(ph_offset, ph_size, i)?;
        checked_bounds(
            entry_offset,
            core::mem::size_of::<ProgramHeader>(),
            image.len(),
            "desbordamiento al calcular el desplazamiento de una cabecera de programa",
            "cabecera de programa desbordada",
        )?;
        let ph: ProgramHeader = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(entry_offset) as *const ProgramHeader)
        };

        if ph.segment_type == PT_LOAD {
            let vaddr = ph.virtual_address;
            if vaddr < virt_base {
                return Err("segmento PT_LOAD por debajo del área de usuario AArch64");
            }
            let offset_in_user = (vaddr - virt_base) as usize;
            let mem_sz = ph.memory_size as usize;

            // The aggregate min/max check above bounds the union of all
            // segments; this bounds *this* segment's own extent, so a
            // future change to that aggregate logic can't silently widen
            // what an individual segment is allowed to touch.
            let seg_end_in_user = offset_in_user
                .checked_add(mem_sz)
                .ok_or("desbordamiento al calcular el final del segmento en memoria")?;
            if seg_end_in_user > user_area_len {
                return Err("segmento PT_LOAD excede el área de usuario AArch64");
            }

            let file_off = ph.offset as usize;
            let file_sz = ph.file_size as usize;
            checked_bounds(
                file_off,
                file_sz,
                image.len(),
                "desbordamiento en el tamaño del segmento PT_LOAD",
                "segmento PT_LOAD desborda la imagen del archivo",
            )?;
            if file_sz > mem_sz {
                // Otherwise the copy below would write past the
                // zeroed-and-bounds-checked `mem_sz` region validated above.
                return Err("el tamaño en archivo del segmento excede su tamaño en memoria");
            }

            let paddr = (phys_base as usize + offset_in_user) as *mut u8;

            // Limpiar con ceros toda la extensión en memoria
            if mem_sz > 0 {
                unsafe {
                    core::ptr::write_bytes(paddr, 0, mem_sz);
                }
            }

            // Copiar datos del archivo
            if file_sz > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(image.as_ptr().add(file_off), paddr, file_sz);
                }
            }

            // Limpiar la caché de datos línea a línea sobre TODA la
            // extensión del segmento — no solo la línea que contiene
            // `paddr` — y refrescar la caché de instrucciones una vez al
            // final (T31.3).
            if mem_sz > 0 {
                let seg_start = paddr as usize;
                let seg_end = seg_start.saturating_add(mem_sz);
                let mut addr = seg_start & !(line_size - 1);
                while addr < seg_end {
                    unsafe {
                        core::arch::asm!("dc cvau, {addr}", addr = in(reg) addr, options(nostack));
                    }
                    addr = addr.saturating_add(line_size);
                }
                unsafe {
                    core::arch::asm!("dsb ish", "ic iallu", "dsb ish", "isb", options(nostack));
                }
            }
        }
    }

    // Pila de usuario ubicada al final del área de usuario con página de guarda
    let stack_top = user_limit - 4096;

    Ok((header.entry, stack_top))
}

// -------------------------------------------------------------------- tests
//
// NOTE (T31.3 / T31.16): the kernel workspace's `cargo test` does not yet
// compile — there is no `#[test]` harness wired for a `no_std` binary target
// (`error[E0463]: can't find crate for `test``), tracked separately in
// T31.16. These tests are written against the exact fix in this file and
// were additionally verified by hand: the same `checked_bounds` /
// `checked_program_header_offset` / `parse_elf` logic, copied verbatim into
// a standalone host binary and compiled with `rustc -O` (so wrapping
// arithmetic would have shown up exactly as it would in the kernel's
// `release` profile), rejects every overflow scenario below and still
// accepts a well-formed image. They will run for real as soon as T31.16
// lands.
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn base_header(ph_offset: u64, ph_size: u16, ph_count: u16) -> Header {
        let mut ident = [0u8; 16];
        ident[0..4].copy_from_slice(MAGIC);
        ident[4] = CLASS_64;
        ident[5] = DATA_2LSB;
        Header {
            identification: ident,
            file_type: ET_EXEC,
            machine: MACHINE_X86_64,
            version: 1,
            entry: 0x1000,
            program_header_offset: ph_offset,
            section_header_offset: 0,
            flags: 0,
            header_size: core::mem::size_of::<Header>() as u16,
            program_header_size: ph_size,
            program_header_count: ph_count,
            section_header_size: 0,
            section_header_count: 0,
            section_name_index: 0,
        }
    }

    fn to_bytes<T>(v: &T) -> &[u8] {
        // SAFETY: test-only, reading a plain-old-data `#[repr(C)]` struct as
        // bytes to build a synthetic image buffer.
        unsafe {
            core::slice::from_raw_parts(v as *const T as *const u8, core::mem::size_of::<T>())
        }
    }

    fn image_with_header(header: &Header, extra_len: usize) -> Vec<u8> {
        let mut image = vec![0u8; core::mem::size_of::<Header>() + extra_len];
        image[0..core::mem::size_of::<Header>()].copy_from_slice(to_bytes(header));
        image
    }

    fn write_program_header(image: &mut [u8], at: usize, ph: &ProgramHeader) {
        image[at..at + core::mem::size_of::<ProgramHeader>()].copy_from_slice(to_bytes(ph));
    }

    #[test_case]
    fn test_parse_elf_rejects_program_header_offset_near_u64_max() {
        let ph_size = core::mem::size_of::<ProgramHeader>() as u16;
        let header = base_header(u64::MAX - 4, ph_size, 1);
        let image = image_with_header(&header, 64);

        assert!(
            parse_elf(&image).is_err(),
            "an implausible program_header_offset must be rejected, not overflow"
        );
    }

    #[test_case]
    fn test_parse_elf_rejects_program_header_count_times_size_overflow() {
        // program_header_count and program_header_size are both u16, so their
        // product can't overflow usize on any platform this kernel targets —
        // but the multiplication is still checked (T31.3), and a table that
        // doesn't fit the image must still be rejected.
        let ph_size = core::mem::size_of::<ProgramHeader>() as u16;
        let header = base_header(core::mem::size_of::<Header>() as u64, ph_size, u16::MAX);
        // Deliberately far too small an image for `u16::MAX` headers.
        let image = image_with_header(&header, 8);

        assert!(
            parse_elf(&image).is_err(),
            "a program header table that doesn't fit must be rejected"
        );
    }

    #[test_case]
    fn test_parse_elf_rejects_segment_offset_near_u64_max() {
        let ph_size = core::mem::size_of::<ProgramHeader>() as u16;
        let header = base_header(core::mem::size_of::<Header>() as u64, ph_size, 1);
        let mut image = image_with_header(&header, core::mem::size_of::<ProgramHeader>());

        let ph = ProgramHeader {
            segment_type: PT_LOAD,
            flags: 0,
            offset: u64::MAX - 10,
            virtual_address: 0x1000,
            physical_address: 0,
            file_size: 1000,
            memory_size: 1000,
            alignment: 0,
        };
        write_program_header(&mut image, core::mem::size_of::<Header>(), &ph);

        assert!(
            parse_elf(&image).is_err(),
            "a segment offset near u64::MAX must be rejected, not overflow"
        );
    }

    #[test_case]
    fn test_parse_elf_rejects_file_size_larger_than_memory_size() {
        let ph_size = core::mem::size_of::<ProgramHeader>() as u16;
        let header = base_header(core::mem::size_of::<Header>() as u64, ph_size, 1);
        let ph_start = core::mem::size_of::<Header>();
        let data_start = ph_start + core::mem::size_of::<ProgramHeader>();
        let mut image = image_with_header(&header, core::mem::size_of::<ProgramHeader>() + 4096);

        let ph = ProgramHeader {
            segment_type: PT_LOAD,
            flags: 0,
            offset: data_start as u64,
            virtual_address: 0x1000,
            physical_address: 0,
            file_size: 4096,
            memory_size: 16, // smaller than file_size — the loader would
            // otherwise copy past the zeroed-and-mapped memory_size region.
            alignment: 0,
        };
        write_program_header(&mut image, ph_start, &ph);

        assert!(
            parse_elf(&image).is_err(),
            "file_size > memory_size must be rejected"
        );
    }

    #[test_case]
    fn test_parse_elf_still_accepts_a_well_formed_image() {
        let ph_size = core::mem::size_of::<ProgramHeader>() as u16;
        let header = base_header(core::mem::size_of::<Header>() as u64, ph_size, 1);
        let ph_start = core::mem::size_of::<Header>();
        let data_start = ph_start + core::mem::size_of::<ProgramHeader>();
        let payload = b"hello, antOS init!";
        let mut image = image_with_header(
            &header,
            core::mem::size_of::<ProgramHeader>() + payload.len(),
        );

        let ph = ProgramHeader {
            segment_type: PT_LOAD,
            flags: 0,
            offset: data_start as u64,
            virtual_address: 0x1000,
            physical_address: 0,
            file_size: payload.len() as u64,
            memory_size: payload.len() as u64,
            alignment: 0x1000,
        };
        write_program_header(&mut image, ph_start, &ph);
        image[data_start..data_start + payload.len()].copy_from_slice(payload);

        let info = parse_elf(&image)
            .expect("a well-formed ELF must still parse after the checked-arithmetic rewrite");
        assert_eq!(info.loadable_segments, 1);
        assert_eq!(info.entry, 0x1000);
        assert_eq!(info.machine, MACHINE_X86_64);
    }

    #[cfg(target_arch = "aarch64")]
    #[test_case]
    fn test_dcache_line_size_is_a_plausible_power_of_two() {
        // Real hardware and QEMU's CTR_EL0 emulation both report a DminLine
        // field that yields a small power-of-two line size (16-256 bytes);
        // this guards against a misreading of the field.
        let line = dcache_line_size();
        assert!(
            line >= 4 && line <= 2048,
            "unexpected cache line size: {line}"
        );
        assert_eq!(
            line & (line - 1),
            0,
            "cache line size must be a power of two"
        );
    }
}
