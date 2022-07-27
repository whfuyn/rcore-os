use crate::utils::BitField;
use super::*;
use frame_allocator::frame_alloc;
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

    pub fn ppn(&self) -> PPN {
        PPN(self as *const _ as usize)
    }

    /// This should only be called when self is a root page table.
    pub fn translate(&self, va: VirtAddr) -> Option<PhysAddr> {
        let vpn = va.vpn();
        let mut page_table = &self.0;
        for i in (0..=2).rev() {
            let index = vpn.level(i);
            let pte = page_table[index];
            if pte.is_leaf() {
                return Some(PhysAddr::new(pte.ppn().as_usize() << 12 | va.offset()));
            } else {
                unsafe {
                    page_table = &(*pte.ppn().as_page_table()).0;
                }
            }
        }
        None
    }

    pub fn clear(&mut self) {
        *self = Self::empty();
    }

    pub fn build_mapping(&mut self, vpn: VPN, ppn: PPN, flags_at_level: [PteFlags; 3]) {
        let root_pte = {
            let index = vpn.level(2);
            if self.0[index].is_valid() {
                self.0[index]
            } else {
                let frame = frame_alloc();
                unsafe {
                    (*frame.as_page_table()).clear();
                }
                PageTableEntry::inner(frame, flags_at_level[2])
            }
        };

        let sub_table = unsafe { &mut *root_pte.as_page_table() };
        let sub_pte = {
            let index = vpn.level(1);
            if sub_table.0[index].is_valid() {
                sub_table.0[index]
            } else {
                let frame = frame_alloc();
                unsafe {
                    (*frame.as_page_table()).clear();
                }
                PageTableEntry::inner(frame, flags_at_level[1])
            }
        };

        let leaf_table = unsafe { &mut *sub_pte.as_page_table() };
        let leaf_pte = PageTableEntry::leaf(ppn, flags_at_level[0]);
        unsafe {
            // Set entries in reverse order to avoid accessing uninit entries.
            leaf_table.set_entry(vpn.level(0), leaf_pte);
            sub_table.set_entry(vpn.level(1), sub_pte);
            self.set_entry(vpn.level(2), root_pte);
        }
    }

    // fn build_kernel_mapping(&mut self, vpn: VPN, ppn: PPN) {
    //     let flags_at_level = [
    //         PteFlags::kernel_inner(),
    //         PteFlags::kernel_inner(),
    //         PteFlags::kernel_leaf(),
    //     ];
    //     self.build_mapping(vpn, ppn, flags_at_level)
    // }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(pub usize);


impl PageTableEntry {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub fn new(ppn: PPN, flags: PteFlags) -> Self {
        let mut pte = Self::zero();
        pte.set_ppn(ppn);
        pte.set_flags(flags);

        pte
    }

    pub fn leaf(ppn: PPN, flags: PteFlags) -> Self {
        assert!(flags.contains(PteFlags::V));
        assert!(
            // It's a leaf.
            !(
                (flags & (PteFlags::R | PteFlags::X)).is_empty()
            )
        );
        if flags.contains(PteFlags::W) {
            // Required by the spec.
            assert!(flags.contains(PteFlags::R));
        }

        Self::new(ppn, flags)
    }

    pub fn inner(child_ppn: PPN, flags: PteFlags) -> Self {
        assert!(flags.contains(PteFlags::V));
        assert!(!flags.contains(PteFlags::R | PteFlags::W | PteFlags::X));
        Self::new(child_ppn, flags)
    }

    pub fn is_valid(self) -> bool {
        self.0.get_bits(0) == 1
    }

    pub fn is_leaf(self) -> bool {
        self.is_valid() & (self.0.get_bits(1) == 1 || self.0.get_bits(3) == 1)
    }

    pub fn set_ppn(&mut self, ppn: PPN) {
        self.0.set_bits(10..=53, ppn.as_usize());
    }

    pub fn set_flags(&mut self, flags: PteFlags) {
        self.0.set_bits(0..=7, flags.bits());
    }

    pub fn ppn(self) -> PPN {
        PPN(self.0.get_bits(10..=53))
    }

    pub fn as_page_table(self) -> *mut PageTable {
        self.ppn().as_page_table()
    }
}

bitflags! {
    pub struct PteFlags: usize {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;

        // const USER_PAGE = 0b11111111;
        // // All except user.
        // const KERNEL_PAGE = 0b11101111;
    }
}

impl PteFlags {
    pub fn kernel_leaf() -> Self {
        PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X | PteFlags::G | PteFlags::D | PteFlags::A
    }

    pub fn kernel_inner() -> Self {
        PteFlags::V | PteFlags::G
    }

    pub fn user_inner() -> Self {
        PteFlags::V
    }

    pub fn user_leaf() -> Self {
        PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X
    }
}

