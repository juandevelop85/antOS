//! QEMU `fw_cfg` driver (MMIO + DMA) and `ramfb` display setup.
//!
//! `ramfb` is the simplest way to get a framebuffer under QEMU/UTM without UEFI:
//! the guest writes a `RAMFBCfg` structure (via the `fw_cfg` DMA interface, to
//! the `etc/ramfb` entry) describing a linear framebuffer it has reserved in
//! RAM, and QEMU scans out from those bytes directly.

use core::ptr::{addr_of, addr_of_mut};

/// `fw_cfg` MMIO base on QEMU/UTM `-M virt`.
pub const FW_CFG_MMIO_DEFAULT: u64 = 0x0902_0000;

// MMIO register offsets (`-M virt`: data/selector big-endian, DMA 64-bit BE).
const FW_CFG_REG_SELECTOR: u64 = 0x08;
const FW_CFG_REG_DMA_HI: u64 = 0x10;
const FW_CFG_REG_DMA_LO: u64 = 0x14;

// Well-known selector keys.
const FW_CFG_SIGNATURE: u16 = 0x0000;
const FW_CFG_FILE_DIR: u16 = 0x0019;

// DMA control bits.
const FW_CFG_DMA_CTL_ERROR: u32 = 0x01;
const FW_CFG_DMA_CTL_READ: u32 = 0x02;
const FW_CFG_DMA_CTL_SKIP: u32 = 0x04;
const FW_CFG_DMA_CTL_SELECT: u32 = 0x08;
const FW_CFG_DMA_CTL_WRITE: u32 = 0x10;

/// `DRM_FORMAT_XRGB8888` (`"XR24"`), the format `ramfb` expects for a plain
/// 32-bit little-endian BGRX framebuffer.
pub const RAMFB_FORMAT_XRGB8888: u32 = 0x3432_5258;

/// Encodes the 28-byte `struct RAMFBCfg` written to the `etc/ramfb` file. Every
/// field is big-endian on the wire, independent of guest endianness.
pub fn encode_ramfb_cfg(
    addr: u64,
    fourcc: u32,
    flags: u32,
    width: u32,
    height: u32,
    stride: u32,
) -> [u8; 28] {
    let mut b = [0u8; 28];
    b[0..8].copy_from_slice(&addr.to_be_bytes());
    b[8..12].copy_from_slice(&fourcc.to_be_bytes());
    b[12..16].copy_from_slice(&flags.to_be_bytes());
    b[16..20].copy_from_slice(&width.to_be_bytes());
    b[20..24].copy_from_slice(&height.to_be_bytes());
    b[24..28].copy_from_slice(&stride.to_be_bytes());
    b
}

/// A parsed `fw_cfg` file directory entry (`struct FWCfgFile`, 64 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwCfgFile {
    pub size: u32,
    pub select: u16,
    pub name_len: usize,
    pub name: [u8; 56],
}

impl FwCfgFile {
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

/// Parses one 64-byte `FWCfgFile` entry (fields big-endian).
pub fn parse_fw_cfg_file(entry: &[u8]) -> Option<FwCfgFile> {
    if entry.len() < 64 {
        return None;
    }
    let size = u32::from_be_bytes([entry[0], entry[1], entry[2], entry[3]]);
    let select = u16::from_be_bytes([entry[4], entry[5]]);
    let mut name = [0u8; 56];
    name.copy_from_slice(&entry[8..64]);
    let name_len = name.iter().position(|&c| c == 0).unwrap_or(56);
    Some(FwCfgFile {
        size,
        select,
        name_len,
        name,
    })
}

/// Scans a raw `FW_CFG_FILE_DIR` blob (`u32 count` big-endian, then that many
/// 64-byte entries) for `name`, returning its selector key.
pub fn find_file_selector(dir_blob: &[u8], name: &str) -> Option<u16> {
    if dir_blob.len() < 4 {
        return None;
    }
    let count = u32::from_be_bytes([dir_blob[0], dir_blob[1], dir_blob[2], dir_blob[3]]) as usize;
    for i in 0..count {
        let start = 4 + i * 64;
        let end = start + 64;
        if end > dir_blob.len() {
            break;
        }
        if let Some(file) = parse_fw_cfg_file(&dir_blob[start..end]) {
            if file.name_str() == name {
                return Some(file.select);
            }
        }
    }
    None
}

// ── Live driver ───────────────────────────────────────────────────────────

#[repr(C, align(64))]
struct FwCfgDmaAccess {
    control: u32,
    length: u32,
    address: u64,
}

static mut DMA_ACCESS: FwCfgDmaAccess = FwCfgDmaAccess {
    control: 0,
    length: 0,
    address: 0,
};

/// Scratch buffer for `fw_cfg` reads (file directory, signature).
const FW_CFG_SCRATCH_LEN: usize = 8192;
static mut FW_CFG_SCRATCH: [u8; FW_CFG_SCRATCH_LEN] = [0u8; FW_CFG_SCRATCH_LEN];

/// `ramfb` framebuffer backing store: 1920x1080 @ 32bpp ceiling (~8 MiB), in
/// `.bss` so it lands in the RAM window the MMU already covers.
#[repr(align(4096))]
struct RamfbBacking([u8; 1920 * 1080 * 4]);
static mut RAMFB_MEM: RamfbBacking = RamfbBacking([0u8; 1920 * 1080 * 4]);

/// The framebuffer `ramfb` handed us.
#[derive(Debug, Clone, Copy)]
pub struct RamfbFramebuffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub width: usize,
    pub height: usize,
    pub stride_bytes: usize,
}

#[inline]
unsafe fn write32_be(addr: u64, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val.to_be());
}

#[inline]
unsafe fn write16_be(addr: u64, val: u16) {
    core::ptr::write_volatile(addr as *mut u16, val.to_be());
}

/// Runs one `fw_cfg` DMA transfer. `control` already carries the direction and
/// (for a SELECT) the selector in its high half. Returns `false` on timeout or
/// device-reported error.
unsafe fn dma(base: u64, control: u32, buf_paddr: u64, len: u32) -> bool {
    let dma_ptr = addr_of_mut!(DMA_ACCESS);
    (*dma_ptr).control = control.to_be();
    (*dma_ptr).length = len.to_be();
    (*dma_ptr).address = buf_paddr.to_be();

    core::arch::asm!("dmb sy", options(nomem, nostack));

    let ctl_paddr = dma_ptr as u64;
    write32_be(base + FW_CFG_REG_DMA_HI, (ctl_paddr >> 32) as u32);
    write32_be(base + FW_CFG_REG_DMA_LO, ctl_paddr as u32); // low write kicks it off

    let mut spin = 4_000_000u32;
    loop {
        core::arch::asm!("dmb sy", options(nomem, nostack));
        let c = u32::from_be(core::ptr::read_volatile(addr_of!((*dma_ptr).control)));
        if c & FW_CFG_DMA_CTL_ERROR != 0 {
            return false;
        }
        if c == 0 {
            return true;
        }
        spin -= 1;
        if spin == 0 {
            return false;
        }
    }
}

/// Reads `len` bytes of the entry `selector` into `FW_CFG_SCRATCH`.
unsafe fn read_entry(base: u64, selector: u16, len: u32) -> Option<&'static [u8]> {
    let len = len.min(FW_CFG_SCRATCH_LEN as u32);
    // Rewind the entry's read cursor with a bare selector write, then DMA-read.
    write16_be(base + FW_CFG_REG_SELECTOR, selector);
    let buf = addr_of_mut!(FW_CFG_SCRATCH) as u64;
    let control = ((selector as u32) << 16) | FW_CFG_DMA_CTL_SELECT | FW_CFG_DMA_CTL_READ;
    if dma(base, control, buf, len) {
        Some(&FW_CFG_SCRATCH[..len as usize])
    } else {
        None
    }
}

/// Verifies the `"QEMU"` signature at selector 0.
unsafe fn signature_ok(base: u64) -> bool {
    matches!(read_entry(base, FW_CFG_SIGNATURE, 4), Some(sig) if sig == b"QEMU")
}

/// Discovers `fw_cfg`, finds `etc/ramfb`, and points it at a RAM framebuffer of
/// `width`x`height`. Returns the backing store on success.
///
/// # Safety
/// Touches `fw_cfg` MMIO at `mmio_base` and programs a DMA descriptor; the
/// caller must ensure the region is mapped as device memory.
pub unsafe fn init_ramfb(mmio_base: u64, width: usize, height: usize) -> Option<RamfbFramebuffer> {
    if !signature_ok(mmio_base) {
        return None;
    }

    // File directory: u32 count then 64-byte entries.
    let dir = read_entry(mmio_base, FW_CFG_FILE_DIR, FW_CFG_SCRATCH_LEN as u32)?;
    let mut dir_copy = [0u8; 8192];
    dir_copy[..dir.len()].copy_from_slice(dir);
    let selector = find_file_selector(&dir_copy, "etc/ramfb")?;

    let stride_bytes = width * 4;
    let needed = stride_bytes * height;
    let backing = addr_of_mut!(RAMFB_MEM.0) as *mut u8;
    if needed > 1920 * 1080 * 4 {
        return None;
    }
    core::ptr::write_bytes(backing, 0, needed);

    let cfg = encode_ramfb_cfg(
        backing as u64,
        RAMFB_FORMAT_XRGB8888,
        0,
        width as u32,
        height as u32,
        stride_bytes as u32,
    );

    // Write the config struct into the etc/ramfb entry via a DMA write.
    let scratch = addr_of_mut!(FW_CFG_SCRATCH) as *mut u8;
    core::ptr::copy_nonoverlapping(cfg.as_ptr(), scratch, cfg.len());
    write16_be(mmio_base + FW_CFG_REG_SELECTOR, selector);
    let control = ((selector as u32) << 16) | FW_CFG_DMA_CTL_SELECT | FW_CFG_DMA_CTL_WRITE;
    let _ = FW_CFG_DMA_CTL_SKIP; // documented for completeness
    if !dma(mmio_base, control, scratch as u64, cfg.len() as u32) {
        return None;
    }

    Some(RamfbFramebuffer {
        ptr: backing,
        len: needed,
        width,
        height,
        stride_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn ramfb_cfg_is_big_endian_28_bytes() {
        let b = encode_ramfb_cfg(0x4200_1000, RAMFB_FORMAT_XRGB8888, 0, 1024, 768, 4096);
        assert_eq!(b.len(), 28);
        assert_eq!(&b[0..8], &0x4200_1000u64.to_be_bytes());
        // fourcc travels big-endian: QEMU reads be32() and matches DRM_FORMAT_XRGB8888.
        assert_eq!(
            u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
            RAMFB_FORMAT_XRGB8888
        );
        assert_eq!(&b[8..12], &[0x34, 0x32, 0x52, 0x58]);
        assert_eq!(&b[12..16], &[0, 0, 0, 0]);
        assert_eq!(u32::from_be_bytes([b[16], b[17], b[18], b[19]]), 1024);
        assert_eq!(u32::from_be_bytes([b[20], b[21], b[22], b[23]]), 768);
        assert_eq!(u32::from_be_bytes([b[24], b[25], b[26], b[27]]), 4096);
    }

    fn dir_entry(size: u32, select: u16, name: &str) -> [u8; 64] {
        let mut e = [0u8; 64];
        e[0..4].copy_from_slice(&size.to_be_bytes());
        e[4..6].copy_from_slice(&select.to_be_bytes());
        let n = name.as_bytes();
        e[8..8 + n.len()].copy_from_slice(n);
        e
    }

    #[test_case]
    fn parses_a_file_directory_entry() {
        let e = dir_entry(28, 0x0028, "etc/ramfb");
        let f = parse_fw_cfg_file(&e).expect("entry");
        assert_eq!(f.size, 28);
        assert_eq!(f.select, 0x0028);
        assert_eq!(f.name_str(), "etc/ramfb");
        assert!(parse_fw_cfg_file(&[0u8; 10]).is_none());
    }

    #[test_case]
    fn finds_ramfb_selector_in_a_directory_blob() {
        let mut blob = alloc::vec::Vec::new();
        blob.extend_from_slice(&3u32.to_be_bytes());
        blob.extend_from_slice(&dir_entry(16, 0x0025, "etc/boot-fail-wait"));
        blob.extend_from_slice(&dir_entry(28, 0x002A, "etc/ramfb"));
        blob.extend_from_slice(&dir_entry(8, 0x002B, "etc/table-loader"));
        assert_eq!(find_file_selector(&blob, "etc/ramfb"), Some(0x002A));
        assert_eq!(find_file_selector(&blob, "etc/nonexistent"), None);
        assert_eq!(find_file_selector(&[0, 0, 0], "etc/ramfb"), None);
    }
}
