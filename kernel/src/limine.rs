//! Limine Boot Protocol requests (T27.1).
//!
//! antOS ships two independent boot conventions today: the `bootloader` crate
//! on x86_64 (BIOS, `run.sh`) and a hand-rolled direct-kernel-boot sequence on
//! AArch64 (`qemu-system-aarch64 -kernel`, no bootloader). Neither one is what
//! `builder` actually writes into the UEFI/GPT images it assembles — those
//! embed the real Limine bootloader (`BOOTX64.EFI` / `BOOTAA64.EFI`, T24.1)
//! and hand it the kernel ELF as `KERNEL.ELF`. Limine refuses to load an ELF
//! whose `PT_LOAD` segments sit in the lower half of the address space
//! ("Lower half PHDRs are not allowed") unless it is built as a relocatable
//! executable — and a plain static kernel binary, linked at a low fixed
//! address the way ours is, is exactly what it rejects.
//!
//! This module defines the Limine request/response ABI in Rust — a faithful,
//! field-for-field port of the C structs in the protocol specification
//! (<https://github.com/limine-bootloader/limine-protocol>, checked here
//! against the exact bootloader version this repository embeds, v12.7.0).
//! Nothing here talks to hardware; it only describes the memory layout
//! Limine and the kernel agree on. The actual entry points that *use* these
//! requests live in `arch::x86_64::limine_boot` and
//! `arch::aarch64::limine_boot`, each behind the `limine` Cargo feature —
//! Limine calls a genuinely different entry point, in a genuinely different
//! machine state (MMU and paging already enabled, higher-half kernel already
//! mapped via its own page tables), than either of antOS's existing boot
//! paths, so unlike those, this one cannot share `_start` with them.

#![allow(dead_code)]

/// Common magic shared by every Limine request ID (`LIMINE_COMMON_MAGIC`).
const COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

/// Declares the base revision antOS speaks. Revisions above 0 drop the
/// deprecated `.limine_reqs` section and (2+) make the start/end markers
/// below mandatory rather than advisory; we do not need the stricter
/// physical-only pointers introduced at revision 3 nor the tighter machine
/// state guarantees of 5/6, so on x86_64 revision 2 is the most modern
/// revision that still matches what the rest of this module assumes.
/// AArch64 does not get a choice here: this Limine build (v12.7.0) rejects
/// anything below revision 6 outright ("Base revision 2 is no longer
/// supported for aarch64, minimum: 6") — found empirically, booting in
/// QEMU, since neither the protocol spec nor its header say so. Revision
/// 6's one AArch64-specific consequence, entry at EL2 with VHE instead of
/// EL1, only triggers when the hardware actually supports VHE (ARMv8.1+);
/// `cortex-a72` (ARMv8.0-A, this repo's target CPU for AArch64 QEMU boots)
/// does not, so `arch::aarch64::limine_boot` can still assume EL1 in
/// practice — `kmain_arm64` reads `CurrentEL` itself right after entry
/// regardless, rather than trusting that assumption blindly, and reports
/// whichever level Limine actually handed off at.
/// Limine overwrites the third element of this array with `0` once it has
/// loaded us at the requested revision — a non-zero value after boot means
/// it silently fell back to an older, unsupported revision, and the kernel
/// should treat that as a boot-time fault.
#[cfg(target_arch = "aarch64")]
const BASE_REVISION_NUMBER: u64 = 6;
#[cfg(not(target_arch = "aarch64"))]
const BASE_REVISION_NUMBER: u64 = 2;

#[used]
#[unsafe(link_section = ".requests")]
pub static BASE_REVISION: [u64; 3] =
    [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, BASE_REVISION_NUMBER];

/// Every Limine executable using base revision 2+ must bracket its requests
/// with these markers so the bootloader can find the whole block reliably
/// instead of scanning the entire image for magic numbers.
#[used]
#[unsafe(link_section = ".requests_start_marker")]
pub static REQUESTS_START_MARKER: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];

#[used]
#[unsafe(link_section = ".requests_end_marker")]
pub static REQUESTS_END_MARKER: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

/// Higher Half Direct Map: `offset` added to any physical address gives a
/// virtual address the kernel can dereference. This is the Limine
/// equivalent of `bootloader_api::BootInfo::physical_memory_offset` — it
/// plugs directly into `memory::Mapper::new`, which only ever wanted a
/// plain `u64` offset and never actually cared which bootloader produced it.
#[repr(C)]
pub struct HhdmRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const HhdmResponse,
}

// SAFETY: Limine writes `response` exactly once, before jumping to the
// kernel's entry point; the kernel only ever reads it afterward, so there is
// no concurrent mutation for `Sync` to actually guard against here.
unsafe impl Sync for HhdmRequest {}

#[repr(C)]
pub struct HhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[used]
#[unsafe(link_section = ".requests")]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0x48dcf1cb8ad2b852, 0x63984e959a98244b],
    revision: 0,
    response: core::ptr::null(),
};

/// Physical memory map: the Limine analogue of
/// `bootloader_api::info::MemoryRegions`. `memory::FrameAllocator` used to
/// require that exact type; T27.1 decoupled it to a plain `&'static [Region]`
/// slice (see `memory::Region`) so either bootloader's map can feed it.
#[repr(C)]
pub struct MemmapRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const MemmapResponse,
}

// SAFETY: see `unsafe impl Sync for HhdmRequest` above — same reasoning.
unsafe impl Sync for MemmapRequest {}

#[repr(C)]
pub struct MemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *const *const MemmapEntry,
}

#[repr(C)]
pub struct MemmapEntry {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}

pub const MEMMAP_USABLE: u64 = 0;

#[used]
#[unsafe(link_section = ".requests")]
pub static MEMMAP_REQUEST: MemmapRequest = MemmapRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0x67cf3d9d378a806f, 0xe304acdfc50c3c62],
    revision: 0,
    response: core::ptr::null(),
};

/// Graphical framebuffer, handed back HHDM-mapped and immediately usable —
/// no MMIO mapping of our own required, unlike the AArch64 direct-boot path
/// (T26.1), which has to probe the DTB / VirtIO-GPU by hand for the same
/// information.
#[repr(C)]
pub struct FramebufferRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const FramebufferResponse,
}

// SAFETY: see `unsafe impl Sync for HhdmRequest` above — same reasoning.
unsafe impl Sync for FramebufferRequest {}

#[repr(C)]
pub struct FramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *const *const Framebuffer,
}

#[repr(C)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *const u8,
    // Response revision 1 adds `mode_count`/`modes` here; we request and
    // read revision 0 only, so this struct stops where our own field access
    // does. Reading only a prefix of a larger struct Limine actually wrote
    // is safe — the fields above are still at their correct offsets.
}

#[used]
#[unsafe(link_section = ".requests")]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0x9d5827dcd881dd75, 0xa3148604f6fab11b],
    revision: 0,
    response: core::ptr::null(),
};

/// Where the kernel actually landed — needed to turn a virtual address of
/// one of *our own* statics into the physical address something like
/// `TTBR0_EL1` requires. Direct-QEMU-boot's `mmu::init` never needed this:
/// with the MMU off, virtual and physical addresses of its page tables were
/// numerically identical by coincidence of the boot environment. Under
/// Limine that coincidence is gone — the kernel runs at a *virtual*
/// higher-half address a slide away from wherever Limine physically loaded
/// it — so `mmu::init_ttbr0_under_limine` (T27.1) uses this response to
/// compute `physical_base - virtual_base` and apply it to `L0_TABLE` &co.
#[repr(C)]
pub struct ExecutableAddressRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const ExecutableAddressResponse,
}

#[repr(C)]
pub struct ExecutableAddressResponse {
    pub revision: u64,
    pub physical_base: u64,
    pub virtual_base: u64,
}

// SAFETY: see `unsafe impl Sync for HhdmRequest` above — same reasoning.
unsafe impl Sync for ExecutableAddressRequest {}

#[used]
#[unsafe(link_section = ".requests")]
pub static EXECUTABLE_ADDRESS_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0x71ba76863cc55f63, 0xb2644a48c516a487],
    revision: 0,
    response: core::ptr::null(),
};

/// Device Tree Blob pointer (AArch64) — the Limine analogue of the raw
/// physical pointer QEMU's direct-kernel-boot leaves in `x0`, except this
/// one comes back HHDM-mapped and immediately dereferenceable, in
/// bootloader-reclaimable memory.
#[repr(C)]
pub struct DtbRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const DtbResponse,
}

#[repr(C)]
pub struct DtbResponse {
    pub revision: u64,
    pub dtb_ptr: *const u8,
}

// SAFETY: see `unsafe impl Sync for HhdmRequest` above — same reasoning.
unsafe impl Sync for DtbRequest {}

#[used]
#[unsafe(link_section = ".requests")]
pub static DTB_REQUEST: DtbRequest = DtbRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0xb40ddb48fb54bac7, 0x545081493f81ffb7],
    revision: 0,
    response: core::ptr::null(),
};

/// Limine RSDP request — hands back the ACPI Root System Description Pointer on
/// firmware that boots via UEFI (T28.8, ACPI discovery path).
#[repr(C)]
pub struct RsdpRequest {
    id: [u64; 4],
    revision: u64,
    pub response: *const RsdpResponse,
}

#[repr(C)]
pub struct RsdpResponse {
    pub revision: u64,
    /// Pointer to the RSDP structure (HHDM-offset on recent Limine revisions).
    pub address: *const u8,
}

// SAFETY: see `unsafe impl Sync for HhdmRequest` above — same reasoning.
unsafe impl Sync for RsdpRequest {}

#[used]
#[unsafe(link_section = ".requests")]
pub static RSDP_REQUEST: RsdpRequest = RsdpRequest {
    id: [COMMON_MAGIC[0], COMMON_MAGIC[1], 0xc5e77b6b397e7b43, 0x27637845accdcf3c],
    revision: 0,
    response: core::ptr::null(),
};
