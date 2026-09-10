//! Minimal ACPI parsing for the Limine boot path.
//!
//! Firmware that boots via UEFI hands the kernel an RSDP instead of a device
//! tree. This module walks RSDP → RSDT/XSDT → tables and decodes the two the
//! bring-up needs: **MCFG** (PCIe ECAM windows) and **MADT** (GIC layout and
//! version). The byte-level decoders are pure and unit-tested against
//! hand-built sample tables; the pointer walk is a thin unsafe wrapper.

use alloc::vec::Vec;

/// The 36-byte System Description Table header shared by every ACPI table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpiHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
}

impl AcpiHeader {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 36 {
            return None;
        }
        Some(Self {
            signature: [bytes[0], bytes[1], bytes[2], bytes[3]],
            length: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            revision: bytes[8],
        })
    }

    pub fn signature_is(&self, sig: &[u8; 4]) -> bool {
        &self.signature == sig
    }
}

/// One PCIe ECAM allocation from an `MCFG` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McfgAllocation {
    pub base_address: u64,
    pub pci_segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

/// Decodes every allocation entry of an `MCFG` table (header + 8 reserved bytes,
/// then 16-byte entries).
pub fn parse_mcfg(table: &[u8]) -> Vec<McfgAllocation> {
    let mut out = Vec::new();
    let Some(hdr) = AcpiHeader::parse(table) else {
        return out;
    };
    if !hdr.signature_is(b"MCFG") {
        return out;
    }
    let len = (hdr.length as usize).min(table.len());
    let mut off = 44; // 36-byte header + 8 reserved
    while off + 16 <= len {
        let e = &table[off..off + 16];
        out.push(McfgAllocation {
            base_address: u64::from_le_bytes([e[0], e[1], e[2], e[3], e[4], e[5], e[6], e[7]]),
            pci_segment: u16::from_le_bytes([e[8], e[9]]),
            start_bus: e[10],
            end_bus: e[11],
        });
        off += 16;
    }
    out
}

/// GIC facts pulled out of an `MADT` table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MadtGic {
    /// GIC Distributor physical base (record type 0x0C).
    pub gicd_base: Option<u64>,
    /// GIC version byte from the Distributor record (0 = unspecified).
    pub gic_version: u8,
    /// GIC Redistributor discovery-range base (record type 0x0E) — GICv3+.
    pub gicr_base: Option<u64>,
    /// Per-CPU GIC CPU Interface bases (record type 0x0B).
    pub gicc_bases: Vec<u64>,
}

impl MadtGic {
    pub fn is_v3(&self) -> bool {
        self.gic_version >= 3 || self.gicr_base.is_some()
    }
}

/// Walks the interrupt-controller records of an `MADT` and extracts GIC data.
///
/// Record layout (ACPI 6.x §5.2.12): after the 36-byte header, a `u32` local
/// interrupt-controller address and a `u32` flags word, then a stream of
/// `{ type: u8, length: u8, .. }` records.
pub fn parse_madt_gic(table: &[u8]) -> MadtGic {
    let mut gic = MadtGic::default();
    let Some(hdr) = AcpiHeader::parse(table) else {
        return gic;
    };
    if !hdr.signature_is(b"APIC") {
        return gic;
    }
    let len = (hdr.length as usize).min(table.len());
    let mut off = 44; // header (36) + local IC address (4) + flags (4)
    while off + 2 <= len {
        let rec_type = table[off];
        let rec_len = table[off + 1] as usize;
        if rec_len < 2 || off + rec_len > len {
            break;
        }
        let rec = &table[off..off + rec_len];
        match rec_type {
            0x0B if rec_len >= 48 => {
                // GICC: Physical Base Address at offset 40 (u64).
                gic.gicc_bases.push(u64::from_le_bytes([
                    rec[40], rec[41], rec[42], rec[43], rec[44], rec[45], rec[46], rec[47],
                ]));
            }
            0x0C if rec_len >= 24 => {
                // GICD: Physical Base Address at offset 8, version at offset 20.
                gic.gicd_base = Some(u64::from_le_bytes([
                    rec[8], rec[9], rec[10], rec[11], rec[12], rec[13], rec[14], rec[15],
                ]));
                gic.gic_version = rec[20];
            }
            0x0E if rec_len >= 16 => {
                // GICR: Discovery Range Base Address at offset 4 (u64).
                gic.gicr_base = Some(u64::from_le_bytes([
                    rec[4], rec[5], rec[6], rec[7], rec[8], rec[9], rec[10], rec[11],
                ]));
            }
            _ => {}
        }
        off += rec_len;
    }
    gic
}

// ── Live RSDP / XSDT walk ─────────────────────────────────────────────────

/// Finds the ACPI table whose signature is `sig`, starting from an RSDP.
///
/// # Safety
/// `rsdp_ptr` must point to a valid ACPI RSDP structure.
pub unsafe fn find_table(rsdp_ptr: *const u8, sig: &[u8; 4]) -> Option<&'static [u8]> {
    if rsdp_ptr.is_null() {
        return None;
    }
    let rsdp = core::slice::from_raw_parts(rsdp_ptr, 36);
    if &rsdp[0..8] != b"RSD PTR " {
        return None;
    }
    let revision = rsdp[15];

    // Revision >= 2 has an XSDT (64-bit pointers) at offset 24.
    let (sdt_ptr, entry_size) = if revision >= 2 {
        let xsdt = u64::from_le_bytes([
            rsdp[24], rsdp[25], rsdp[26], rsdp[27], rsdp[28], rsdp[29], rsdp[30], rsdp[31],
        ]);
        (xsdt as usize, 8usize)
    } else {
        let rsdt = u32::from_le_bytes([rsdp[16], rsdp[17], rsdp[18], rsdp[19]]);
        (rsdt as usize, 4usize)
    };
    if sdt_ptr == 0 {
        return None;
    }

    let sdt_hdr = core::slice::from_raw_parts(sdt_ptr as *const u8, 36);
    let sdt_len = u32::from_le_bytes([sdt_hdr[4], sdt_hdr[5], sdt_hdr[6], sdt_hdr[7]]) as usize;
    if !(36..=64 * 1024).contains(&sdt_len) {
        return None;
    }
    let sdt = core::slice::from_raw_parts(sdt_ptr as *const u8, sdt_len);

    let mut off = 36;
    while off + entry_size <= sdt.len() {
        let table_ptr = if entry_size == 8 {
            u64::from_le_bytes([
                sdt[off],
                sdt[off + 1],
                sdt[off + 2],
                sdt[off + 3],
                sdt[off + 4],
                sdt[off + 5],
                sdt[off + 6],
                sdt[off + 7],
            ]) as usize
        } else {
            u32::from_le_bytes([sdt[off], sdt[off + 1], sdt[off + 2], sdt[off + 3]]) as usize
        };
        off += entry_size;
        if table_ptr == 0 {
            continue;
        }
        let hdr = core::slice::from_raw_parts(table_ptr as *const u8, 36);
        if &hdr[0..4] == sig {
            let tlen = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
            if (36..=1024 * 1024).contains(&tlen) {
                return Some(core::slice::from_raw_parts(table_ptr as *const u8, tlen));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acpi_header(sig: &[u8; 4], length: u32) -> [u8; 36] {
        let mut h = [0u8; 36];
        h[0..4].copy_from_slice(sig);
        h[4..8].copy_from_slice(&length.to_le_bytes());
        h[8] = 1; // revision
        h
    }

    #[test_case]
    fn parses_mcfg_ecam_allocations() {
        let mut t = acpi_header(b"MCFG", 44 + 32).to_vec();
        t.extend_from_slice(&[0u8; 8]); // reserved
                                        // Entry 0: base 0x4010000000, segment 0, bus 0..255.
        t.extend_from_slice(&0x40_1000_0000u64.to_le_bytes());
        t.extend_from_slice(&0u16.to_le_bytes());
        t.extend_from_slice(&[0x00, 0xFF, 0, 0, 0, 0]);
        // Entry 1: base 0x3f000000, segment 1, bus 0..15.
        t.extend_from_slice(&0x3f00_0000u64.to_le_bytes());
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&[0x00, 0x0F, 0, 0, 0, 0]);

        let allocs = parse_mcfg(&t);
        assert_eq!(allocs.len(), 2);
        assert_eq!(allocs[0].base_address, 0x40_1000_0000);
        assert_eq!(allocs[0].end_bus, 0xFF);
        assert_eq!(allocs[1].base_address, 0x3f00_0000);
        assert_eq!(allocs[1].pci_segment, 1);
        assert_eq!(allocs[1].end_bus, 0x0F);
    }

    #[test_case]
    fn rejects_mcfg_with_wrong_signature() {
        let t = acpi_header(b"FACP", 44).to_vec();
        assert!(parse_mcfg(&t).is_empty());
    }

    #[test_case]
    fn parses_madt_gicv3_layout() {
        let mut records = Vec::new();
        // GICC (type 0x0B), length 80, base at offset 40.
        let mut gicc = alloc::vec![0u8; 80];
        gicc[0] = 0x0B;
        gicc[1] = 80;
        gicc[40..48].copy_from_slice(&0u64.to_le_bytes()); // no GICC MMIO in v3
        records.extend_from_slice(&gicc);
        // GICD (type 0x0C), length 24, base at 8, version at 20.
        let mut gicd = alloc::vec![0u8; 24];
        gicd[0] = 0x0C;
        gicd[1] = 24;
        gicd[8..16].copy_from_slice(&0x0800_0000u64.to_le_bytes());
        gicd[20] = 3;
        records.extend_from_slice(&gicd);
        // GICR (type 0x0E), length 16, base at 4.
        let mut gicr = alloc::vec![0u8; 16];
        gicr[0] = 0x0E;
        gicr[1] = 16;
        gicr[4..12].copy_from_slice(&0x080a_0000u64.to_le_bytes());
        records.extend_from_slice(&gicr);

        let mut t = acpi_header(b"APIC", (44 + records.len()) as u32).to_vec();
        t.extend_from_slice(&[0u8; 8]); // local IC addr + flags
        t.extend_from_slice(&records);

        let gic = parse_madt_gic(&t);
        assert_eq!(gic.gicd_base, Some(0x0800_0000));
        assert_eq!(gic.gic_version, 3);
        assert_eq!(gic.gicr_base, Some(0x080a_0000));
        assert!(gic.is_v3());
    }

    #[test_case]
    fn parses_madt_gicv2_without_redistributor() {
        let mut gicd = alloc::vec![0u8; 24];
        gicd[0] = 0x0C;
        gicd[1] = 24;
        gicd[8..16].copy_from_slice(&0x0800_0000u64.to_le_bytes());
        gicd[20] = 2;

        let mut t = acpi_header(b"APIC", (44 + gicd.len()) as u32).to_vec();
        t.extend_from_slice(&[0u8; 8]);
        t.extend_from_slice(&gicd);

        let gic = parse_madt_gic(&t);
        assert_eq!(gic.gic_version, 2);
        assert_eq!(gic.gicr_base, None);
        assert!(!gic.is_v3());
    }
}
