use crate::utils::BitField;
use super::*;
use bitflags::bitflags;

const PAGE_TABLE_ENTRIES: usize = 1 << 9;
const PAGE_TABLE_SIZE: usize = PAGE_TABLE_ENTRIES * 8;

#[derive(Debug, Clone)]
#[repr(C, align(4096))]
pub struct PageTable(pub [PageTableEntry; PAGE_TABLE_ENTRIES]);

impl PageTable {
    pub const fn empty() -> Self {
        Self([PageTableEntry::zero(); PAGE_TABLE_ENTRIES])
    }

    pub unsafe fn set_entry(&mut self, index: usize, pte: PageTableEntry) {
        self.0[index] = pte;
    }

    pub fn pa(&mut self) -> PhysAddr {
        PhysAddr::new(self as *mut _ as usize)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(usize);


impl PageTableEntry {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub fn leaf(ppn: PPN, flags: PteFlags) -> Self {
        assert!(flags.contains(PteFlags::VALID));
        assert!(
            // It's a leaf.
            !(
                (flags & (PteFlags::READ | PteFlags::EXECUTE)).is_empty()
            )
        );
        if flags.contains(PteFlags::WRITE) {
            // Required by the spec.
            assert!(flags.contains(PteFlags::READ));
        }

        let mut pte = Self::zero();
        pte.set_ppn(ppn);
        pte.set_flags(flags);

        pte
    }

    pub fn parent(child_ppn: PPN, flags: PteFlags) -> Self {
        assert!(flags.contains(PteFlags::VALID));
        assert!(!flags.contains(PteFlags::READ | PteFlags::WRITE | PteFlags::EXECUTE));

        let mut pte = Self::zero();
        pte.set_ppn(child_ppn);
        pte.set_flags(flags);

        pte
    }

    pub fn is_valid(&self) -> bool {
        self.0.get_bits(0) == 1
    }

    pub fn is_leaf(&self) -> bool {
        self.0.get_bits(1) == 1 || self.0.get_bits(3) == 1
    }

    pub fn set_ppn(&mut self, ppn: PPN) {
        self.0.set_bits(10..=53, ppn.as_usize());
    }

    pub fn set_flags(&mut self, flags: PteFlags) {
        self.0.set_bits(0..=7, flags.bits());
    }

    pub fn ppn(&self) -> PPN {
        PPN(self.0.get_bits(10..=53))
    }
}

bitflags! {
    pub struct PteFlags: usize {
        const VALID = 1 << 0;
        const READ = 1 << 1;
        const WRITE = 1 << 2;
        const EXECUTE = 1 << 3;
        const USER = 1 << 4;
        const GLOBAL = 1 << 5;
        const ACCESS = 1 << 6;
        const DIRTY = 1 << 6;
    }
}

