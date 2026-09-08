//! Generic flattened device tree (FDT / DTB) parser.
//!
//! Where [`super::dtb`] carries a handful of hand-rolled scanners, this module
//! offers a reusable walk: locate nodes by `compatible`, read `reg` with the
//! parent's `#address-cells` / `#size-cells`, and decode `interrupts` in the GIC
//! three-cell form. It backs the firmware-directed bring-up in
//! [`super::firmware`] and is unit-tested against real `qemu -M virt` blobs.

use alloc::vec::Vec;

const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_BEGIN_NODE: u32 = 0x0000_0001;
const FDT_END_NODE: u32 = 0x0000_0002;
const FDT_PROP: u32 = 0x0000_0003;
const FDT_NOP: u32 = 0x0000_0004;
const FDT_END: u32 = 0x0000_0009;

/// A parsed FDT ready to be queried. Borrows the blob in place.
pub struct Fdt<'a> {
    struct_block: &'a [u8],
    strings_block: &'a [u8],
}

/// One device node located by [`Fdt::find_compatible`].
#[derive(Clone)]
pub struct FdtNode<'a> {
    struct_block: &'a [u8],
    strings_block: &'a [u8],
    /// Node name (`pl011@9000000`), sans trailing NUL.
    pub name: &'a str,
    /// Offset in the structure block of this node's first child token.
    body: usize,
    /// `#address-cells` / `#size-cells` in effect for this node's `reg`
    /// (i.e. inherited from the parent).
    addr_cells: u32,
    size_cells: u32,
}

fn be32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn cstr(b: &[u8], off: usize) -> &str {
    let s = b.get(off..).unwrap_or(&[]);
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    core::str::from_utf8(&s[..end]).unwrap_or("")
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// Validates the FDT header at `ptr` and returns a parser borrowing it for
/// `'static` (the blob lives for the life of the kernel).
///
/// # Safety
/// `ptr` must point to a readable region at least as large as the blob's
/// declared `totalsize` (bounded here to 2 MiB).
pub unsafe fn from_ptr(ptr: *const u8) -> Option<Fdt<'static>> {
    if ptr.is_null() {
        return None;
    }
    let head = core::slice::from_raw_parts(ptr, 40);
    if be32(head, 0)? != FDT_MAGIC {
        return None;
    }
    let totalsize = be32(head, 4)? as usize;
    if !(40..=2 * 1024 * 1024).contains(&totalsize) {
        return None;
    }
    let blob: &'static [u8] = core::slice::from_raw_parts(ptr, totalsize);
    Fdt::from_slice(blob)
}

impl<'a> Fdt<'a> {
    /// Parses an FDT already resident in a byte slice.
    pub fn from_slice(blob: &'a [u8]) -> Option<Fdt<'a>> {
        if blob.len() < 40 || be32(blob, 0)? != FDT_MAGIC {
            return None;
        }
        let off_struct = be32(blob, 8)? as usize;
        let off_strings = be32(blob, 12)? as usize;
        let size_strings = be32(blob, 32)? as usize;
        let size_struct = be32(blob, 36)? as usize;
        let struct_block = blob.get(off_struct..off_struct.checked_add(size_struct)?)?;
        let strings_block = blob.get(off_strings..off_strings.checked_add(size_strings)?)?;
        Some(Fdt {
            struct_block,
            strings_block,
        })
    }

    /// Every node whose `compatible` list contains `needle` as a substring.
    pub fn find_compatible(&self, needle: &str) -> Vec<FdtNode<'a>> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if let Some(compat) = node.prop("compatible") {
                if slice_contains(compat, needle.as_bytes()) {
                    out.push(node.clone());
                }
            }
        });
        out
    }

    /// First node with the exact `name` (before any `@unit-address`, or the
    /// whole name if there is none).
    pub fn find_by_name(&self, name: &str) -> Option<FdtNode<'a>> {
        let mut found = None;
        self.walk(&mut |node| {
            if found.is_none() {
                let base = node.name.split('@').next().unwrap_or(node.name);
                if base == name || node.name == name {
                    found = Some(node.clone());
                }
            }
        });
        found
    }

    /// Walks the whole tree, invoking `f` for every node with the address/size
    /// cells its `reg` should be decoded with.
    fn walk(&self, f: &mut dyn FnMut(&FdtNode<'a>)) {
        // Stack of (addr_cells, size_cells) for the current path; root defaults
        // to 2/2 per the spec.
        let mut stack: Vec<(u32, u32)> = Vec::new();
        stack.push((2, 2));

        let sb = self.struct_block;
        let mut cur = 0usize;
        while cur + 4 <= sb.len() {
            let tag = match be32(sb, cur) {
                Some(t) => t,
                None => break,
            };
            cur += 4;
            match tag {
                FDT_BEGIN_NODE => {
                    let name_off = cur;
                    let name_end = sb[name_off..]
                        .iter()
                        .position(|&c| c == 0)
                        .map(|p| name_off + p)
                        .unwrap_or(sb.len());
                    let name = core::str::from_utf8(&sb[name_off..name_end]).unwrap_or("");
                    cur = align4(name_end + 1);

                    let (parent_ac, parent_sc) = *stack.last().unwrap_or(&(2, 2));
                    let node = FdtNode {
                        struct_block: sb,
                        strings_block: self.strings_block,
                        name,
                        body: cur,
                        addr_cells: parent_ac,
                        size_cells: parent_sc,
                    };
                    f(&node);

                    // This node's own cells govern its children.
                    let own_ac = node.prop_u32("#address-cells").unwrap_or(parent_ac);
                    let own_sc = node.prop_u32("#size-cells").unwrap_or(parent_sc);
                    stack.push((own_ac, own_sc));

                    // Advance `cur` past this node's own properties.
                    cur = skip_props(sb, cur);
                }
                FDT_END_NODE => {
                    stack.pop();
                }
                FDT_PROP => {
                    // Orphan prop at this level (rare); skip it.
                    let len = be32(sb, cur).unwrap_or(0) as usize;
                    cur = align4(cur + 8 + len);
                }
                FDT_NOP => {}
                FDT_END => break,
                _ => break,
            }
        }
    }
}

/// Advances past a run of `FDT_PROP` / `FDT_NOP` tokens, stopping at the next
/// `FDT_BEGIN_NODE` / `FDT_END_NODE` / `FDT_END`.
fn skip_props(sb: &[u8], mut cur: usize) -> usize {
    while cur + 4 <= sb.len() {
        match be32(sb, cur) {
            Some(FDT_PROP) => {
                let len = be32(sb, cur + 4).unwrap_or(0) as usize;
                cur = align4(cur + 12 + len);
            }
            Some(FDT_NOP) => cur += 4,
            _ => break,
        }
    }
    cur
}

impl<'a> FdtNode<'a> {
    /// Raw bytes of property `name`, or `None` if the node lacks it.
    pub fn prop(&self, name: &str) -> Option<&'a [u8]> {
        let sb = self.struct_block;
        let mut cur = self.body;
        while cur + 4 <= sb.len() {
            match be32(sb, cur)? {
                FDT_PROP => {
                    let len = be32(sb, cur + 4)? as usize;
                    let nameoff = be32(sb, cur + 8)? as usize;
                    let val_start = cur + 12;
                    let val_end = val_start.checked_add(len)?;
                    if val_end > sb.len() {
                        return None;
                    }
                    if cstr(self.strings_block, nameoff) == name {
                        return Some(&sb[val_start..val_end]);
                    }
                    cur = align4(val_end);
                }
                FDT_NOP => cur += 4,
                _ => break,
            }
        }
        None
    }

    fn prop_u32(&self, name: &str) -> Option<u32> {
        self.prop(name).and_then(|b| be32(b, 0))
    }

    /// `(address, size)` of the `index`-th entry of `reg`, decoded with this
    /// node's inherited `#address-cells` / `#size-cells`.
    pub fn reg_at(&self, index: usize) -> Option<(u64, u64)> {
        let reg = self.prop("reg")?;
        let ac = self.addr_cells.max(1) as usize;
        let sc = self.size_cells as usize;
        let stride = (ac + sc) * 4;
        let start = index.checked_mul(stride)?;
        if start + ac * 4 > reg.len() {
            return None;
        }
        let mut addr = 0u64;
        for i in 0..ac {
            addr = (addr << 32) | be32(reg, start + i * 4)? as u64;
        }
        let mut size = 0u64;
        for i in 0..sc {
            if start + (ac + i) * 4 + 4 > reg.len() {
                break;
            }
            size = (size << 32) | be32(reg, start + (ac + i) * 4)? as u64;
        }
        Some((addr, size))
    }

    /// `(address, size)` of the first `reg` entry.
    pub fn reg(&self) -> Option<(u64, u64)> {
        self.reg_at(0)
    }

    /// GIC-style interrupt specifiers `(kind, number, flags)` — `kind` 0 = SPI,
    /// 1 = PPI.
    pub fn interrupts(&self) -> Vec<(u32, u32, u32)> {
        let mut out = Vec::new();
        if let Some(ints) = self.prop("interrupts") {
            let mut i = 0;
            while i + 12 <= ints.len() {
                if let (Some(k), Some(n), Some(fl)) =
                    (be32(ints, i), be32(ints, i + 4), be32(ints, i + 8))
                {
                    out.push((k, n, fl));
                }
                i += 12;
            }
        }
        out
    }

    /// Resolved GIC INTIDs for this node's `interrupts` (SPI + 32, PPI + 16).
    pub fn interrupt_intids(&self) -> Vec<u32> {
        self.interrupts()
            .into_iter()
            .map(|(kind, num, _)| gic_intid(kind, num))
            .collect()
    }
}

/// Maps a device-tree `(kind, number)` GIC interrupt to its INTID.
pub fn gic_intid(kind: u32, number: u32) -> u32 {
    match kind {
        0 => number + 32, // SPI
        1 => number + 16, // PPI
        _ => number,      // extended / unknown: pass through
    }
}

fn slice_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return needle.is_empty();
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIRT_GICV2: &[u8] = include_bytes!("testdata/qemu-virt-gicv2.dtb");
    const VIRT_GICV3: &[u8] = include_bytes!("testdata/qemu-virt-gicv3.dtb");

    #[test]
    fn parses_pl011_reg_and_irq_from_real_blob() {
        let fdt = Fdt::from_slice(VIRT_GICV2).expect("valid fdt");
        let pl011 = fdt
            .find_compatible("arm,pl011")
            .into_iter()
            .next()
            .expect("pl011 node");
        assert_eq!(pl011.reg(), Some((0x0900_0000, 0x1000)));
        // interrupts = <0x00 0x01 0x04> -> SPI 1 -> INTID 33, level (0x4).
        assert_eq!(pl011.interrupts(), alloc::vec![(0, 1, 4)]);
        assert_eq!(pl011.interrupt_intids(), alloc::vec![33]);
    }

    #[test]
    fn parses_highmem_ecam_base_and_bus_range() {
        let fdt = Fdt::from_slice(VIRT_GICV2).unwrap();
        let pcie = fdt
            .find_compatible("pci-host-ecam-generic")
            .into_iter()
            .next()
            .expect("pcie node");
        // reg = <0x40 0x10000000 0x00 0x10000000> with parent #address-cells=2.
        assert_eq!(pcie.reg(), Some((0x40_1000_0000, 0x1000_0000)));
        let bus_range = pcie.prop("bus-range").unwrap();
        assert_eq!(be32(bus_range, 0), Some(0));
        assert_eq!(be32(bus_range, 4), Some(0xFF));
    }

    #[test]
    fn distinguishes_gicv2_and_gicv3_layouts() {
        let v2 = Fdt::from_slice(VIRT_GICV2).unwrap();
        let g2 = v2
            .find_compatible("arm,cortex-a15-gic")
            .into_iter()
            .next()
            .unwrap();
        // reg: GICD @ 0x8000000 (0x10000), GICC @ 0x8010000 (0x10000).
        assert_eq!(g2.reg_at(0), Some((0x0800_0000, 0x1_0000)));
        assert_eq!(g2.reg_at(1), Some((0x0801_0000, 0x1_0000)));
        assert!(v2.find_compatible("arm,gic-v3").is_empty());

        let v3 = Fdt::from_slice(VIRT_GICV3).unwrap();
        let g3 = v3
            .find_compatible("arm,gic-v3")
            .into_iter()
            .next()
            .unwrap();
        // reg: GICD @ 0x8000000, GICR @ 0x80a0000 (0xf60000).
        assert_eq!(g3.reg_at(0), Some((0x0800_0000, 0x1_0000)));
        assert_eq!(g3.reg_at(1), Some((0x080a_0000, 0x00f6_0000)));
    }

    #[test]
    fn timer_ppis_resolve_to_intid_27_and_30() {
        let fdt = Fdt::from_slice(VIRT_GICV2).unwrap();
        let timer = fdt.find_by_name("timer").expect("timer node");
        // interrupts = <1 0x0d ..> <1 0x0e ..> <1 0x0b ..> <1 0x0a ..>
        //   -> PPI 13 (secure), 14 (phys=INTID 30), 11 (virt=INTID 27), 10 (hyp).
        let intids = timer.interrupt_intids();
        assert!(intids.contains(&30), "physical timer INTID present: {:?}", intids);
        assert!(intids.contains(&27), "virtual timer INTID present: {:?}", intids);
    }

    #[test]
    fn finds_fw_cfg_and_first_virtio_mmio() {
        let fdt = Fdt::from_slice(VIRT_GICV2).unwrap();
        let fw = fdt
            .find_compatible("qemu,fw-cfg-mmio")
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(fw.reg(), Some((0x0902_0000, 0x18)));

        let virtio = fdt.find_compatible("virtio,mmio");
        assert!(virtio.len() >= 8, "several virtio-mmio transports");
        assert_eq!(virtio[0].reg(), Some((0x0a00_0000, 0x200)));
        // interrupts = <0x00 0x10 0x01> -> SPI 16 -> INTID 48.
        assert_eq!(virtio[0].interrupt_intids(), alloc::vec![48]);
    }

    #[test]
    fn rejects_non_fdt_input() {
        assert!(Fdt::from_slice(&[0u8; 8]).is_none());
        assert!(Fdt::from_slice(b"not a device tree at all").is_none());
    }
}
