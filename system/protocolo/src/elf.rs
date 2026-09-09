//! antOS ELF64 Binary Format Parser and Loader Protocol.
//!
//! Provides typed definitions, validation, and layout calculations for
//! 64-bit ELF executables on AArch64 and x86_64 architectures, including
//! segment permission inspection, memory range computation, and System V ABI stack setup.

use serde::{Deserialize, Serialize};

pub const ELF_MAGIC: &[u8; 4] = b"\x7fELF";

pub const ELFCLASS64: u8 = 2;
pub const ELFDATA2LSB: u8 = 1; // 2's complement, little endian
pub const EV_CURRENT: u32 = 1;

pub const ET_EXEC: u16 = 2; // Executable file
pub const ET_DYN: u16 = 3; // Position-independent executable (PIE) / shared library

pub const EM_X86_64: u16 = 0x3E; // AMD x86-64
pub const EM_AARCH64: u16 = 0xB7; // ARM 64-bit (AArch64)

pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
pub const PT_GNU_EH_FRAME: u32 = 0x6474e550;
pub const PT_GNU_STACK: u32 = 0x6474e551;
pub const PT_GNU_RELRO: u32 = 0x6474e552;

pub const PF_X: u32 = 1; // Execute
pub const PF_W: u32 = 2; // Write
pub const PF_R: u32 = 4; // Read

/// Raw 64-bit ELF File Header (Elf64_Ehdr).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elf64Header {
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

/// Raw 64-bit ELF Program Header (Elf64_Phdr).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elf64ProgramHeader {
    pub segment_type: u32,
    pub flags: u32,
    pub offset: u64,
    pub virtual_address: u64,
    pub physical_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub alignment: u64,
}

/// A parsed, loadable memory segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElfProgramSegment {
    pub segment_type: u32,
    pub flags: u32,
    pub file_offset: usize,
    pub file_size: usize,
    pub virtual_address: u64,
    pub memory_size: usize,
    pub alignment: u64,
}

impl ElfProgramSegment {
    #[inline]
    pub fn is_readable(&self) -> bool {
        (self.flags & PF_R) != 0
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        (self.flags & PF_W) != 0
    }

    #[inline]
    pub fn is_executable(&self) -> bool {
        (self.flags & PF_X) != 0
    }

    #[inline]
    pub fn is_loadable(&self) -> bool {
        self.segment_type == PT_LOAD
    }

    #[inline]
    pub fn bss_size(&self) -> usize {
        self.memory_size.saturating_sub(self.file_size)
    }
}

/// Successfully parsed and validated 64-bit ELF image metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedElf64 {
    pub entry_point: u64,
    pub machine: u16,
    pub is_pie: bool,
    pub segments: Vec<ElfProgramSegment>,
    pub min_vaddr: u64,
    pub max_vaddr: u64,
    pub total_memory_size: u64,
}

/// Errors occurring during ELF validation or parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElfError {
    ImageTooSmall,
    InvalidMagic,
    Not64Bit,
    NotLittleEndian,
    InvalidVersion,
    UnsupportedArchitecture(u16),
    TruncatedProgramHeaders,
    SegmentOutOfBounds,
    NoLoadableSegments,
}

/// Parses and validates a raw 64-bit ELF binary image into structured metadata.
pub fn parse_elf64(image: &[u8]) -> Result<ParsedElf64, ElfError> {
    if image.len() < core::mem::size_of::<Elf64Header>() {
        return Err(ElfError::ImageTooSmall);
    }

    // Read header without alignment assumptions
    let header = unsafe { core::ptr::read_unaligned(image.as_ptr() as *const Elf64Header) };

    if &header.identification[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }
    if header.identification[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }
    if header.identification[5] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }
    if header.version != EV_CURRENT && header.identification[6] != 1 {
        return Err(ElfError::InvalidVersion);
    }
    if header.machine != EM_X86_64 && header.machine != EM_AARCH64 {
        return Err(ElfError::UnsupportedArchitecture(header.machine));
    }

    let is_pie = header.file_type == ET_DYN;
    let ph_offset = header.program_header_offset as usize;
    let ph_size = header.program_header_size as usize;
    let ph_count = header.program_header_count as usize;

    if ph_count > 0 && ph_offset + ph_count * ph_size > image.len() {
        return Err(ElfError::TruncatedProgramHeaders);
    }

    let mut segments = Vec::with_capacity(ph_count);
    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0u64;
    let mut has_loadable = false;

    for i in 0..ph_count {
        let entry_offset = ph_offset + i * ph_size;
        if entry_offset + core::mem::size_of::<Elf64ProgramHeader>() > image.len() {
            return Err(ElfError::TruncatedProgramHeaders);
        }

        let ph = unsafe {
            core::ptr::read_unaligned(image.as_ptr().add(entry_offset) as *const Elf64ProgramHeader)
        };

        if ph.segment_type == PT_LOAD {
            has_loadable = true;
            let file_off = ph.offset as usize;
            let file_sz = ph.file_size as usize;

            if file_off + file_sz > image.len() {
                return Err(ElfError::SegmentOutOfBounds);
            }

            let start = ph.virtual_address;
            let end = start.saturating_add(ph.memory_size);

            if start < min_vaddr {
                min_vaddr = start;
            }
            if end > max_vaddr {
                max_vaddr = end;
            }

            segments.push(ElfProgramSegment {
                segment_type: ph.segment_type,
                flags: ph.flags,
                file_offset: file_off,
                file_size: file_sz,
                virtual_address: ph.virtual_address,
                memory_size: ph.memory_size as usize,
                alignment: ph.alignment,
            });
        }
    }

    if !has_loadable {
        return Err(ElfError::NoLoadableSegments);
    }

    let total_memory_size = max_vaddr.saturating_sub(min_vaddr);

    Ok(ParsedElf64 {
        entry_point: header.entry,
        machine: header.machine,
        is_pie,
        segments,
        min_vaddr,
        max_vaddr,
        total_memory_size,
    })
}

/// Verifies that a parsed ELF binary matches the required target machine architecture.
pub fn validate_for_arch(elf: &ParsedElf64, expected_arch: u16) -> Result<(), ElfError> {
    if elf.machine != expected_arch {
        Err(ElfError::UnsupportedArchitecture(elf.machine))
    } else {
        Ok(())
    }
}

/// Generates an initial user stack frame conforming to the System V ABI.
///
/// Places strings for program arguments (`argv`), followed by alignment padding,
/// `NULL` sentinel, argument pointers array (`argv[0]..argv[argc-1]`), and `argc`.
/// Returns the updated stack pointer (guaranteed 16-byte aligned) and the byte payload.
pub fn build_system_v_abi_stack(initial_sp: u64, args: &[&str]) -> (u64, Vec<u8>) {
    let mut payload = Vec::new();
    let mut string_offsets = Vec::with_capacity(args.len());

    // 1. Push argument strings with null terminators
    for arg in args {
        let offset = payload.len();
        payload.extend_from_slice(arg.as_bytes());
        payload.push(0); // null terminator
        string_offsets.push(offset);
    }

    // 2. Pad to 8-byte alignment before pointer table
    while payload.len() % 8 != 0 {
        payload.push(0);
    }

    // Calculate virtual addresses of argument strings once placed on user stack
    let string_area_size = payload.len();
    let string_area_base = initial_sp - string_area_size as u64;

    let mut string_addrs = Vec::with_capacity(args.len());
    for &off in &string_offsets {
        string_addrs.push(string_area_base + off as u64);
    }

    // 3. Pointer table: argv[0..argc], null terminator, and argc
    let mut ptr_table = Vec::new();

    // argc (u64)
    ptr_table.extend_from_slice(&(args.len() as u64).to_le_bytes());

    // argv pointers
    for addr in string_addrs {
        ptr_table.extend_from_slice(&addr.to_le_bytes());
    }

    // NULL pointer terminator for argv
    ptr_table.extend_from_slice(&0u64.to_le_bytes());

    // NULL envp terminator
    ptr_table.extend_from_slice(&0u64.to_le_bytes());

    let total_bytes = payload.len() + ptr_table.len();
    let mut final_sp = initial_sp - total_bytes as u64;

    // Ensure 16-byte alignment required by System V ABI
    let align_pad = (final_sp % 16) as usize;
    final_sp -= align_pad as u64;

    let mut final_payload = Vec::with_capacity(total_bytes + align_pad);
    if align_pad > 0 {
        final_payload.resize(align_pad, 0);
    }
    final_payload.extend_from_slice(&ptr_table);
    final_payload.extend_from_slice(&payload);

    (final_sp, final_payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_synthetic_elf(machine: u16, class: u8, endian: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 128];

        // e_ident
        buf[0..4].copy_from_slice(ELF_MAGIC);
        buf[4] = class;
        buf[5] = endian;
        buf[6] = 1; // EV_CURRENT

        // Header fields
        let header = Elf64Header {
            identification: [
                0x7F, b'E', b'L', b'F', class, endian, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            file_type: ET_EXEC,
            machine,
            version: EV_CURRENT,
            entry: 0x0040_0000,
            program_header_offset: 64,
            section_header_offset: 0,
            flags: 0,
            header_size: 64,
            program_header_size: 56,
            program_header_count: 1,
            section_header_size: 0,
            section_header_count: 0,
            section_name_index: 0,
        };

        unsafe {
            core::ptr::copy_nonoverlapping(
                &header as *const Elf64Header as *const u8,
                buf.as_mut_ptr(),
                core::mem::size_of::<Elf64Header>(),
            );
        }

        // Program header 0 (PT_LOAD)
        let ph = Elf64ProgramHeader {
            segment_type: PT_LOAD,
            flags: PF_R | PF_X,
            offset: 0,
            virtual_address: 0x0040_0000,
            physical_address: 0x0040_0000,
            file_size: 128,
            memory_size: 256, // 128 bytes of code + 128 bytes of BSS
            alignment: 4096,
        };

        unsafe {
            core::ptr::copy_nonoverlapping(
                &ph as *const Elf64ProgramHeader as *const u8,
                buf.as_mut_ptr().add(64),
                core::mem::size_of::<Elf64ProgramHeader>(),
            );
        }

        buf
    }

    #[test]
    fn test_valid_x86_64_elf_parsing() {
        let image = create_synthetic_elf(EM_X86_64, ELFCLASS64, ELFDATA2LSB);
        let parsed = parse_elf64(&image).expect("Debe parsear ELF x86_64 valido");

        assert_eq!(parsed.entry_point, 0x0040_0000);
        assert_eq!(parsed.machine, EM_X86_64);
        assert!(!parsed.is_pie);
        assert_eq!(parsed.segments.len(), 1);

        let seg = &parsed.segments[0];
        assert!(seg.is_loadable());
        assert!(seg.is_readable());
        assert!(!seg.is_writable());
        assert!(seg.is_executable());
        assert_eq!(seg.bss_size(), 128);
        assert_eq!(parsed.total_memory_size, 256);
    }

    #[test]
    fn test_valid_aarch64_elf_parsing() {
        let image = create_synthetic_elf(EM_AARCH64, ELFCLASS64, ELFDATA2LSB);
        let parsed = parse_elf64(&image).expect("Debe parsear ELF AArch64 valido");

        assert_eq!(parsed.entry_point, 0x0040_0000);
        assert_eq!(parsed.machine, EM_AARCH64);
        assert_eq!(parsed.segments.len(), 1);
        assert!(validate_for_arch(&parsed, EM_AARCH64).is_ok());
        assert!(validate_for_arch(&parsed, EM_X86_64).is_err());
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut image = create_synthetic_elf(EM_X86_64, ELFCLASS64, ELFDATA2LSB);
        image[0] = 0x00;
        assert_eq!(parse_elf64(&image), Err(ElfError::InvalidMagic));
    }

    #[test]
    fn test_32bit_rejected() {
        let image = create_synthetic_elf(EM_X86_64, 1 /* ELFCLASS32 */, ELFDATA2LSB);
        assert_eq!(parse_elf64(&image), Err(ElfError::Not64Bit));
    }

    #[test]
    fn test_big_endian_rejected() {
        let image = create_synthetic_elf(EM_X86_64, ELFCLASS64, 2 /* MSB */);
        assert_eq!(parse_elf64(&image), Err(ElfError::NotLittleEndian));
    }

    #[test]
    fn test_unsupported_arch_rejected() {
        let image = create_synthetic_elf(0x03 /* EM_386 */, ELFCLASS64, ELFDATA2LSB);
        assert_eq!(
            parse_elf64(&image),
            Err(ElfError::UnsupportedArchitecture(3))
        );
    }

    #[test]
    fn test_truncated_headers_rejected() {
        let image = create_synthetic_elf(EM_X86_64, ELFCLASS64, ELFDATA2LSB);
        // Truncate to just header size without room for program header
        let truncated = &image[..64];
        assert_eq!(
            parse_elf64(truncated),
            Err(ElfError::TruncatedProgramHeaders)
        );
    }

    #[test]
    fn test_system_v_abi_stack_building() {
        let initial_sp = 0x0000_7FFF_FFFF_0000u64;
        let args = ["/bin/init", "--verbose", "mode=interactive"];
        let (new_sp, payload) = build_system_v_abi_stack(initial_sp, &args);

        // System V ABI mandates 16-byte stack alignment
        assert_eq!(new_sp % 16, 0);
        assert!(new_sp < initial_sp);
        assert!(!payload.is_empty());
    }
}
