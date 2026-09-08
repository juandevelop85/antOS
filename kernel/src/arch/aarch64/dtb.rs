//! Device Tree Blob (DTB / FDT) parser for AArch64 bare-metal kernel.
//!
//! Provides introspection of hardware device nodes passed by QEMU, UTM, or firmware
//! at boot time (FDT magic 0xd00dfeed). Specifically discovers `simple-framebuffer`
//! nodes, `virtio,mmio` GPU devices and the `pci-host-ecam-generic` node.

use bootloader_api::info::PixelFormat;
use core::sync::atomic::{AtomicU64, Ordering};

pub const FDT_MAGIC: u32 = 0xd00dfeed;

/// Physical base of the flattened device tree, stashed by `kmain_arm64` so
/// discovery helpers that run later (e.g. `pci::probe_ecam_base`) can reach it
/// without threading the pointer through every call site. `0` = unknown, fall
/// back to the QEMU `virt` convention of "DTB at the base of RAM".
static DTB_BASE: AtomicU64 = AtomicU64::new(0);

/// Records the firmware-provided device-tree pointer for later discovery.
pub fn set_dtb_base(ptr: u64) {
    DTB_BASE.store(ptr, Ordering::Relaxed);
}

/// Returns the stashed device-tree pointer, or the QEMU `virt` fallback
/// (`0x4000_0000`, base of RAM) when none was recorded.
pub fn dtb_base() -> u64 {
    let p = DTB_BASE.load(Ordering::Relaxed);
    if p != 0 {
        p
    } else {
        0x4000_0000
    }
}

const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_NOP: u32 = 0x0000_0004;
const FDT_END: u32 = 0x0000_0009;

/// Discovered Framebuffer configuration from Device Tree or VirtIO.
#[derive(Clone, Copy, Debug)]
pub struct FramebufferConfig {
    pub phys_addr: u64,
    pub size: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bytes_per_pixel: usize,
    pub format: PixelFormat,
    pub is_virtio: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

impl FdtHeader {
    unsafe fn from_ptr(ptr: *const u8) -> Option<Self> {
        if ptr.is_null() {
            return None;
        }

        let magic = u32::from_be(*(ptr as *const u32));
        if magic != FDT_MAGIC {
            return None;
        }

        let slice = core::slice::from_raw_parts(ptr as *const u32, 10);
        Some(FdtHeader {
            magic,
            totalsize: u32::from_be(slice[1]),
            off_dt_struct: u32::from_be(slice[2]),
            off_dt_strings: u32::from_be(slice[3]),
            off_mem_rsvmap: u32::from_be(slice[4]),
            version: u32::from_be(slice[5]),
            last_comp_version: u32::from_be(slice[6]),
            boot_cpuid_phys: u32::from_be(slice[7]),
            size_dt_strings: u32::from_be(slice[8]),
            size_dt_struct: u32::from_be(slice[9]),
        })
    }
}

/// Discovers a graphical framebuffer by scanning the DTB at `dtb_ptr`
/// (or base of RAM at 0x4000_0000 if `dtb_ptr` is 0).
pub fn find_framebuffer(dtb_ptr: u64) -> Option<FramebufferConfig> {
    // Try provided pointer first if non-zero
    if dtb_ptr != 0 {
        if let Some(cfg) = scan_dtb_at(dtb_ptr) {
            return Some(cfg);
        }
    }

    // Fallback: QEMU virt machine places DTB at start of RAM (0x4000_0000) when x0 is 0
    if dtb_ptr != 0x4000_0000 {
        if let Some(cfg) = scan_dtb_at(0x4000_0000) {
            return Some(cfg);
        }
    }

    None
}

/// Searches the DTB for VirtIO MMIO devices that have Device ID 16 (GPU).
pub fn find_virtio_gpu(dtb_ptr: u64) -> Option<u64> {
    if dtb_ptr != 0 {
        if let Some(base) = scan_dtb_for_virtio_gpu(dtb_ptr) {
            return Some(base);
        }
    }

    if dtb_ptr != 0x4000_0000 {
        if let Some(base) = scan_dtb_for_virtio_gpu(0x4000_0000) {
            return Some(base);
        }
    }

    None
}

fn scan_dtb_at(addr: u64) -> Option<FramebufferConfig> {
    if addr == 0 {
        return None;
    }

    let ptr = addr as *const u8;
    let header = unsafe { FdtHeader::from_ptr(ptr)? };

    if header.totalsize < 40 || header.totalsize > 16 * 1024 * 1024 {
        return None;
    }

    let dtb_bytes = unsafe { core::slice::from_raw_parts(ptr, header.totalsize as usize) };
    let struct_start = header.off_dt_struct as usize;
    let struct_end = struct_start.checked_add(header.size_dt_struct as usize)?;
    let strings_start = header.off_dt_strings as usize;
    let strings_end = strings_start.checked_add(header.size_dt_strings as usize)?;

    if struct_end > dtb_bytes.len() || strings_end > dtb_bytes.len() {
        return None;
    }

    let struct_block = &dtb_bytes[struct_start..struct_end];
    let strings_block = &dtb_bytes[strings_start..strings_end];

    let mut cursor = 0;
    while cursor + 4 <= struct_block.len() {
        let tag = read_u32_be(struct_block, cursor)?;
        cursor += 4;

        match tag {
            FDT_BEGIN_NODE => {
                // Find node name
                let name_start = cursor;
                while cursor < struct_block.len() && struct_block[cursor] != 0 {
                    cursor += 1;
                }
                cursor += 1; // skip null byte
                cursor = (cursor + 3) & !3; // align to 4 bytes

                let node_name = core::str::from_utf8(&struct_block[name_start..cursor.saturating_sub(1)]).unwrap_or("");

                // Parse properties of this node
                let mut is_simple_fb = false;
                let mut reg_addr = 0u64;
                let mut reg_size = 0usize;
                let mut width = 0usize;
                let mut height = 0usize;
                let mut stride = 0usize;
                let mut pixel_fmt = PixelFormat::Bgr;

                while cursor + 4 <= struct_block.len() {
                    let next_tag = read_u32_be(struct_block, cursor)?;
                    if next_tag == FDT_PROP {
                        cursor += 4;
                        let prop_len = read_u32_be(struct_block, cursor)? as usize;
                        cursor += 4;
                        let nameoff = read_u32_be(struct_block, cursor)? as usize;
                        cursor += 4;

                        let prop_val_end = cursor + prop_len;
                        if prop_val_end > struct_block.len() {
                            break;
                        }
                        let prop_val = &struct_block[cursor..prop_val_end];
                        cursor = (prop_val_end + 3) & !3;

                        let prop_name = get_string(strings_block, nameoff);

                        if prop_name == "compatible" {
                            if contains_str(prop_val, "simple-framebuffer") {
                                is_simple_fb = true;
                            }
                        } else if prop_name == "reg" {
                            if prop_len >= 16 {
                                let hi_addr = read_u32_be(prop_val, 0)? as u64;
                                let lo_addr = read_u32_be(prop_val, 4)? as u64;
                                let hi_size = read_u32_be(prop_val, 8)? as u64;
                                let lo_size = read_u32_be(prop_val, 12)? as u64;
                                reg_addr = (hi_addr << 32) | lo_addr;
                                reg_size = ((hi_size << 32) | lo_size) as usize;
                            } else if prop_len >= 8 {
                                reg_addr = read_u32_be(prop_val, 0)? as u64;
                                reg_size = read_u32_be(prop_val, 4)? as usize;
                            }
                        } else if prop_name == "width" && prop_len >= 4 {
                            width = read_u32_be(prop_val, 0)? as usize;
                        } else if prop_name == "height" && prop_len >= 4 {
                            height = read_u32_be(prop_val, 0)? as usize;
                        } else if prop_name == "stride" && prop_len >= 4 {
                            stride = read_u32_be(prop_val, 0)? as usize;
                        } else if prop_name == "format" {
                            if contains_str(prop_val, "r8g8b8") || contains_str(prop_val, "r8g8b8a8") {
                                pixel_fmt = PixelFormat::Rgb;
                            } else {
                                pixel_fmt = PixelFormat::Bgr;
                            }
                        }
                    } else if next_tag == FDT_NOP {
                        cursor += 4;
                    } else {
                        break;
                    }
                }

                if is_simple_fb || node_name.starts_with("framebuffer") {
                    if reg_addr != 0 && width > 0 && height > 0 {
                        let bytes_per_pixel = 4;
                        let stride_pixels = if stride > 0 {
                            stride / bytes_per_pixel
                        } else {
                            width
                        };
                        let calc_size = if reg_size > 0 {
                            reg_size
                        } else {
                            stride_pixels * height * bytes_per_pixel
                        };

                        return Some(FramebufferConfig {
                            phys_addr: reg_addr,
                            size: calc_size,
                            width,
                            height,
                            stride: stride_pixels,
                            bytes_per_pixel,
                            format: pixel_fmt,
                            is_virtio: false,
                        });
                    }
                }
            }
            FDT_END_NODE | FDT_NOP => {}
            FDT_PROP => {
                // Orphan prop, skip
                if cursor + 8 <= struct_block.len() {
                    let prop_len = read_u32_be(struct_block, cursor).unwrap_or(0) as usize;
                    cursor += 8;
                    cursor = (cursor + prop_len + 3) & !3;
                }
            }
            FDT_END => break,
            _ => break,
        }
    }

    None
}

/// Locates the PCIe ECAM configuration window (`pci-host-ecam-generic`) in the
/// device tree, returning `(ecam_base, bus_start, bus_end)`. Used by
/// `pci::probe_ecam_base` so the ECAM address comes from firmware instead of a
/// per-hypervisor constant (T28.2; full DTB/ACPI bring-up is T28.8).
pub fn find_pcie_ecam() -> Option<(u64, u8, u8)> {
    let base = dtb_base();
    if let Some(found) = scan_dtb_for_pcie_ecam(base) {
        return Some(found);
    }
    if base != 0x4000_0000 {
        return scan_dtb_for_pcie_ecam(0x4000_0000);
    }
    None
}

fn scan_dtb_for_pcie_ecam(addr: u64) -> Option<(u64, u8, u8)> {
    if addr == 0 {
        return None;
    }

    let ptr = addr as *const u8;
    let header = unsafe { FdtHeader::from_ptr(ptr)? };
    if header.totalsize < 40 || header.totalsize > 16 * 1024 * 1024 {
        return None;
    }
    let dtb_bytes = unsafe { core::slice::from_raw_parts(ptr, header.totalsize as usize) };

    let struct_start = header.off_dt_struct as usize;
    let struct_end = struct_start.checked_add(header.size_dt_struct as usize)?;
    let strings_start = header.off_dt_strings as usize;
    let strings_end = strings_start.checked_add(header.size_dt_strings as usize)?;
    if struct_end > dtb_bytes.len() || strings_end > dtb_bytes.len() {
        return None;
    }
    let struct_block = &dtb_bytes[struct_start..struct_end];
    let strings_block = &dtb_bytes[strings_start..strings_end];

    let mut cursor = 0;
    while cursor + 4 <= struct_block.len() {
        let tag = read_u32_be(struct_block, cursor)?;
        cursor += 4;

        if tag == FDT_BEGIN_NODE {
            while cursor < struct_block.len() && struct_block[cursor] != 0 {
                cursor += 1;
            }
            cursor += 1;
            cursor = (cursor + 3) & !3;

            let mut is_ecam = false;
            let mut ecam_base = 0u64;
            let mut bus_lo = 0u8;
            let mut bus_hi = 0u8;

            while cursor + 4 <= struct_block.len() {
                let next_tag = read_u32_be(struct_block, cursor)?;
                if next_tag == FDT_PROP {
                    cursor += 4;
                    let prop_len = read_u32_be(struct_block, cursor)? as usize;
                    cursor += 4;
                    let nameoff = read_u32_be(struct_block, cursor)? as usize;
                    cursor += 4;

                    let prop_val_end = cursor + prop_len;
                    if prop_val_end > struct_block.len() {
                        break;
                    }
                    let prop_val = &struct_block[cursor..prop_val_end];
                    cursor = (prop_val_end + 3) & !3;

                    let prop_name = get_string(strings_block, nameoff);
                    if prop_name == "compatible"
                        && (contains_str(prop_val, "pci-host-ecam-generic")
                            || contains_str(prop_val, "pci-host-cam-generic"))
                    {
                        is_ecam = true;
                    } else if prop_name == "reg" && prop_len >= 16 {
                        // Root cells are #address-cells=2 / #size-cells=2 on the
                        // `virt` machine: the first 8 bytes are the ECAM base.
                        let hi = read_u32_be(prop_val, 0)? as u64;
                        let lo = read_u32_be(prop_val, 4)? as u64;
                        ecam_base = (hi << 32) | lo;
                    } else if prop_name == "bus-range" && prop_len >= 8 {
                        bus_lo = (read_u32_be(prop_val, 0)? & 0xFF) as u8;
                        bus_hi = (read_u32_be(prop_val, 4)? & 0xFF) as u8;
                    }
                } else if next_tag == FDT_NOP {
                    cursor += 4;
                } else {
                    break;
                }
            }

            if is_ecam && ecam_base != 0 {
                let bus_hi = if bus_hi >= bus_lo { bus_hi } else { 0xFF };
                return Some((ecam_base, bus_lo, bus_hi));
            }
        } else if tag == FDT_END {
            break;
        }
    }

    None
}

fn scan_dtb_for_virtio_gpu(addr: u64) -> Option<u64> {
    if addr == 0 {
        return None;
    }

    let ptr = addr as *const u8;
    let header = unsafe { FdtHeader::from_ptr(ptr)? };
    let dtb_bytes = unsafe { core::slice::from_raw_parts(ptr, header.totalsize as usize) };

    let struct_start = header.off_dt_struct as usize;
    let struct_end = struct_start.checked_add(header.size_dt_struct as usize)?;
    let strings_start = header.off_dt_strings as usize;
    let strings_end = strings_start.checked_add(header.size_dt_strings as usize)?;

    if struct_end > dtb_bytes.len() || strings_end > dtb_bytes.len() {
        return None;
    }

    let struct_block = &dtb_bytes[struct_start..struct_end];
    let strings_block = &dtb_bytes[strings_start..strings_end];

    let mut cursor = 0;
    while cursor + 4 <= struct_block.len() {
        let tag = read_u32_be(struct_block, cursor)?;
        cursor += 4;

        if tag == FDT_BEGIN_NODE {
            while cursor < struct_block.len() && struct_block[cursor] != 0 {
                cursor += 1;
            }
            cursor += 1;
            cursor = (cursor + 3) & !3;

            let mut is_virtio_mmio = false;
            let mut mmio_base = 0u64;

            while cursor + 4 <= struct_block.len() {
                let next_tag = read_u32_be(struct_block, cursor)?;
                if next_tag == FDT_PROP {
                    cursor += 4;
                    let prop_len = read_u32_be(struct_block, cursor)? as usize;
                    cursor += 4;
                    let nameoff = read_u32_be(struct_block, cursor)? as usize;
                    cursor += 4;

                    let prop_val_end = cursor + prop_len;
                    if prop_val_end > struct_block.len() {
                        break;
                    }
                    let prop_val = &struct_block[cursor..prop_val_end];
                    cursor = (prop_val_end + 3) & !3;

                    let prop_name = get_string(strings_block, nameoff);
                    if prop_name == "compatible" && contains_str(prop_val, "virtio,mmio") {
                        is_virtio_mmio = true;
                    } else if prop_name == "reg" {
                        if prop_len >= 16 {
                            let hi = read_u32_be(prop_val, 0)? as u64;
                            let lo = read_u32_be(prop_val, 4)? as u64;
                            mmio_base = (hi << 32) | lo;
                        } else if prop_len >= 8 {
                            mmio_base = read_u32_be(prop_val, 0)? as u64;
                        }
                    }
                } else if next_tag == FDT_NOP {
                    cursor += 4;
                } else {
                    break;
                }
            }

            if is_virtio_mmio && mmio_base >= 0x0a00_0000 && mmio_base < 0x0a20_0000 {
                // Check if this virtio MMIO peripheral is a GPU (DeviceID == 16)
                // Offset 0x00: Magic (0x74726976)
                // Offset 0x08: DeviceID
                unsafe {
                    let magic = core::ptr::read_volatile(mmio_base as *const u32);
                    let device_id = core::ptr::read_volatile((mmio_base + 0x08) as *const u32);
                    if magic == 0x74726976 && device_id == 16 {
                        return Some(mmio_base);
                    }
                }
            }
        } else if tag == FDT_END {
            break;
        }
    }

    None
}

fn read_u32_be(slice: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 <= slice.len() {
        let bytes: [u8; 4] = [
            slice[offset],
            slice[offset + 1],
            slice[offset + 2],
            slice[offset + 3],
        ];
        Some(u32::from_be_bytes(bytes))
    } else {
        None
    }
}

fn get_string(strings_block: &[u8], offset: usize) -> &str {
    if offset >= strings_block.len() {
        return "";
    }
    let slice = &strings_block[offset..];
    let len = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    core::str::from_utf8(&slice[..len]).unwrap_or("")
}

fn contains_str(bytes: &[u8], pattern: &str) -> bool {
    let pat_bytes = pattern.as_bytes();
    if pat_bytes.len() > bytes.len() {
        return false;
    }
    for window in bytes.windows(pat_bytes.len()) {
        if window == pat_bytes {
            return true;
        }
    }
    false
}
