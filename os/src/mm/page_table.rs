use crate::utils::BitField;
use super::*;
use bitflags::bitflags;
use spin::Mutex;
use crate::config::*;
use riscv::register::satp;

const PAGE_TABLE_ENTRIES: usize = 1 << 9;
// const PAGE_TABLE_SIZE: usize = PAGE_TABLE_ENTRIES * 8;

pub static GLOBAL_PTES: Mutex<GlobalPtes> = Mutex::new(GlobalPtes{
    kernel_pte_index: 0,
    kernel_pte: PageTableEntry::zero(),

    memory_pte_index: 0,
    memory_pte: PageTableEntry::zero(),
});

pub struct GlobalPtes {
    // kernel memory mapping
    pub kernel_pte_index: usize,
    pub kernel_pte: PageTableEntry,

    // physical memory identity mapping
    pub memory_pte_index: usize,
    pub memory_pte: PageTableEntry,
}

pub fn init() {
    let current_page_table = unsafe {
        PPN(satp::read().ppn()).as_page_table()
    };

    let mut global_ptes = GLOBAL_PTES.lock();
    global_ptes.kernel_pte_index = KERNEL_BASE_ADDRESS.vpn().level(2);
    global_ptes.kernel_pte = *current_page_table.pte_of(KERNEL_BASE_ADDRESS, 2);

    let memory_va = VirtAddr::new(QEMU_MEMORY_START);
    global_ptes.memory_pte_index = memory_va.vpn().level(2);
    global_ptes.memory_pte = *current_page_table.pte_of(memory_va, 2);
}

#[derive(Debug, Clone)]
#[repr(C, align(4096))]
pub struct PageTable(pub [PageTableEntry; PAGE_TABLE_ENTRIES]);

impl PageTable {
    pub const fn empty() -> Self {
        Self([PageTableEntry::zero(); PAGE_TABLE_ENTRIES])
    }

    pub fn add_globals(&mut self) {
        let global_ptes = GLOBAL_PTES.lock();
        unsafe {
            self.set_entry(global_ptes.kernel_pte_index, global_ptes.kernel_pte);
            self.set_entry(global_ptes.memory_pte_index, global_ptes.memory_pte);
        }
    }

    pub unsafe fn set_entry(&mut self, index: usize, pte: PageTableEntry) {
        self.0[index] = pte;
    }

    pub fn pa(&mut self) -> PhysAddr {
        PhysAddr::new(self as *mut _ as usize)
    }

    pub fn ppn(&self) -> PPN {
        PPN((self as *const _ as usize) >> 12)
    }

    /// This should only be called when self is a root page table.
    pub fn translate(&self, va: VirtAddr) -> Option<PhysAddr> {
        let vpn = va.vpn();
        let mut page_table = &self.0;
        for i in (0..=2).rev() {
            let index = vpn.level(i);
            let pte = page_table[index];
            // crate::println!("transalte ppn: 0x{:x}", pte.ppn().0);
            if pte.is_valid() {
                if pte.is_leaf() {
                    return Some(PhysAddr::new(pte.ppn().as_usize() << 12 | va.offset()));
                } else {
                    unsafe {
                        page_table = &(*pte.ppn().as_page_table()).0;
                    }
                }
            } else {
                return None;
            }
       }
        None
    }

    pub fn pte_of(&self, va: VirtAddr, level: usize) -> &PageTableEntry {
        let vpn = va.vpn();
        let index = vpn.level(level);
        &self.0[index]
    }

    pub unsafe fn pte_of_mut(&mut self, va: VirtAddr, level: usize) -> &mut PageTableEntry {
        let vpn = va.vpn();
        let index = vpn.level(level);
        &mut self.0[index]
    }

    pub fn clear(&mut self) {
        *self = Self::empty();
    }

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
        // Disable this check because we use it to create an invalid pte.

        // assert!(flags.contains(PteFlags::V));
        // assert!(
        //     // It's a leaf.
        //     !(
        //         (flags & (PteFlags::R | PteFlags::X)).is_empty()
        //     )
        // );
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

    pub fn with_ppn(self, ppn: PPN) -> Self {
        let mut ret = self;
        ret.set_ppn(ppn);
        ret
    }

    pub unsafe fn as_page_table<'a>(self) -> &'a PageTable {
        self.ppn().as_page_table()
    }

    pub unsafe fn as_page_table_mut<'a>(self) -> &'a mut PageTable {
        self.ppn().as_page_table_mut()
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
        // PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X | PteFlags::D | PteFlags::A
    }

    pub fn kernel_inner() -> Self {
        PteFlags::V
    }

    pub fn user_inner() -> Self {
        PteFlags::V
        // PteFlags::V
    }

    pub fn user_leaf() -> Self {
        PteFlags::V | PteFlags::U
        // PteFlags::V 
    }
}

