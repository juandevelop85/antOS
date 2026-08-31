//! Cargador de ELF64.
//!
//! Es exactamente lo que el bootloader nos hizo a nosotros en la Fase 0. Si
//! miras el log de arranque de entonces, ahí está:
//!
//! ```text
//! INFO : Handling Segment: Ph64(ProgramHeader64 { type_: Ok(Load), ... })
//! INFO : Mapping bss section
//! INFO : Jumping to kernel entry point at VirtAddr(0x10000004170)
//! ```
//!
//! Ahora nos toca hacérselo a otro. Un ELF trae dos tablas: la de secciones,
//! que le interesa al enlazador, y la de **segmentos de programa**, que es la
//! que dice cómo cargarlo en memoria. Solo importa la segunda.
//!
//! Cada segmento `PT_LOAD` dice: coge `filesz` bytes desde `offset` del
//! fichero, ponlos en la dirección virtual `vaddr`, y rellena con ceros hasta
//! `memsz`. Esa diferencia entre `filesz` y `memsz` es el `.bss`: variables
//! que empiezan a cero y por eso no hace falta guardarlas en el fichero.

use crate::memory::{FrameAllocator, Mapper, PAGE_SIZE, PRESENT, USER, WRITABLE};

const MAGIC: &[u8; 4] = b"\x7fELF";
const CLASS_64: u8 = 2;
const MACHINE_X86_64: u16 = 0x3E;
const PT_LOAD: u32 = 1;

/// La cabecera, tal cual la define el estándar. Solo se usan unos pocos
/// campos; el resto están para que los desplazamientos cuadren.
#[repr(C)]
struct Header {
    identification: [u8; 16],
    file_type: u16,
    machine: u16,
    version: u32,
    entry: u64,
    program_header_offset: u64,
    section_header_offset: u64,
    flags: u32,
    header_size: u16,
    program_header_size: u16,
    program_header_count: u16,
    section_header_size: u16,
    section_header_count: u16,
    section_name_index: u16,
}

#[repr(C)]
struct ProgramHeader {
    segment_type: u32,
    flags: u32,
    offset: u64,
    virtual_address: u64,
    physical_address: u64,
    file_size: u64,
    memory_size: u64,
    alignment: u64,
}

/// Carga los segmentos del programa en el espacio de direcciones actual y
/// devuelve su punto de entrada.
///
/// # Safety
/// Mapea páginas nuevas y escribe en ellas. El ELF debe ser de confianza: no
/// se comprueba que sus direcciones no pisen al kernel.
pub unsafe fn load(
    image: &[u8],
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    if image.len() < core::mem::size_of::<Header>() {
        return Err("la imagen es demasiado pequeña para ser un ELF");
    }

    // Se lee SIN alinear, y no es paranoia: `include_bytes!` produce un array
    // de bytes con alineación 1, mientras que `Header` contiene campos de 64
    // bits que exigen alineación 8. Castear el puntero y desreferenciarlo es
    // comportamiento indefinido — y Rust en modo depuración lo caza.
    //
    // SAFETY: se acaba de comprobar que hay bytes suficientes, y Header es
    // #[repr(C)] con la disposición exacta del estándar.
    let header: Header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Header) };

    if &header.identification[0..4] != MAGIC {
        return Err("no es un ELF");
    }
    if header.identification[4] != CLASS_64 {
        return Err("no es un ELF de 64 bits");
    }
    if header.machine != MACHINE_X86_64 {
        return Err("no es para x86_64");
    }

    for index in 0..header.program_header_count as usize {
        let offset =
            header.program_header_offset as usize + index * header.program_header_size as usize;
        if offset + core::mem::size_of::<ProgramHeader>() > image.len() {
            return Err("tabla de segmentos truncada");
        }
        // SAFETY: el desplazamiento está dentro de la imagen. Sin alinear,
        // por lo mismo que la cabecera.
        let segment: ProgramHeader =
            unsafe { core::ptr::read_unaligned(image.as_ptr().add(offset) as *const ProgramHeader) };

        if segment.segment_type != PT_LOAD {
            continue;
        }
        unsafe { load_segment(image, &segment, mapper, allocator)? };
    }

    Ok(header.entry)
}

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

    let mut page = first_page;
    while page <= last_page {
        // Dos segmentos pueden compartir página: el final de uno y el
        // principio del siguiente caen en los mismos 4 KiB. Mapear la segunda
        // vez sería un error, así que se comprueba antes.
        if mapper.translate(page).is_none() {
            let frame = allocator.allocate().ok_or("sin marcos para el programa")?;
            // SIMPLIFICACIÓN: todo se mapea escribible, ignorando los permisos
            // que declara el segmento. Un cargador serio dejaría el código de
            // solo lectura y los datos sin ejecutar. Aquí haría falta además
            // poder escribirlos primero, así que se pospone.
            unsafe {
                mapper.map(page, frame, PRESENT | WRITABLE | USER, allocator)?;
                core::ptr::write_bytes(page as *mut u8, 0, PAGE_SIZE as usize);
            }
        }
        page += PAGE_SIZE;
    }

    // Copiar la parte que sí viene en el fichero. Lo que queda hasta memsz ya
    // está a cero de haber limpiado las páginas: eso es el .bss.
    let from = segment.offset as usize;
    let length = segment.file_size as usize;
    if from + length > image.len() {
        return Err("segmento fuera de la imagen");
    }
    // SAFETY: las páginas de destino acaban de mapearse y son escribibles.
    unsafe {
        core::ptr::copy_nonoverlapping(image.as_ptr().add(from), start as *mut u8, length);
    }

    Ok(())
}
