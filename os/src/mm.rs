pub mod page_table;
pub mod frame_allocator;
pub mod heap_allocator;
pub mod address_space;

use crate::utils::BitField;
pub use page_table::*;

pub fn init(frame_start: PPN, frame_end: PPN) {
    heap_allocator::init();
    frame_allocator::init(frame_start, frame_end);
    page_table::init();
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PhysAddr(pub usize);

impl PhysAddr {
    pub const fn new(pa: usize) -> Self {
        Self(pa)
    }

    pub fn ppn(self) -> PPN {
        PPN(self.0.get_bits(12..=55))
    }

    pub fn offset(self) -> usize {
        self.0.get_bits(..12)
    }

}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    #[track_caller]
    pub fn new(va: usize) -> Self {
        // bit 39..=63 must equal to bit 38.
        // See privileged spec Sv39.
        match va.get_bits(38) {
            0 => assert_eq!(va.get_bits(39..=63), 0),
            1 => assert_eq!(va.get_bits(39..=63), (1 << 25) - 1),
            _ => unreachable!()
        }
        Self(va)
    }

    pub const unsafe fn new_unchecked(va: usize) -> Self {
        Self(va)
    }

    pub fn vpn(self) -> VPN {
        VPN(self.0.get_bits(12..=38))
    }

    pub fn offset(self) -> usize {
        self.0.get_bits(..12)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PPN(pub usize);

impl PPN {
    pub fn as_usize(self) -> usize {
        self.0
    }

    pub fn level(self, level: usize) -> usize {
        match level {
            0 => self.0.get_bits(0..=8),
            1 => self.0.get_bits(9..=17),
            2 => self.0.get_bits(18..=43),
            _ => panic!("invalid ppn level"),
        }
    }

    pub fn set_level(&mut self, level: usize, val: usize) {
        match level {
            0 => self.0.set_bits(0..=8, val),
            1 => self.0.set_bits(9..=17, val),
            2 => self.0.set_bits(18..=43, val),
            _ => panic!("invalid ppn level"),
        }
    }

    pub unsafe fn as_page_table<'a>(self) -> &'a PageTable {
        &*((self.0 << 12) as *const PageTable)
    }

    pub unsafe fn as_page_table_mut<'a>(self) -> &'a mut PageTable {
        &mut *((self.0 << 12) as *mut PageTable)
    }

    pub fn as_pa(self) -> PhysAddr {
        PhysAddr(self.0 << 12)
    }

}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct VPN(pub usize);

impl VPN {
    pub fn as_usize(self) -> usize {
        self.0
    }

    pub fn level(self, level: usize) -> usize {
        match level {
            0 => self.0.get_bits(0..=8),
            1 => self.0.get_bits(9..=17),
            2 => self.0.get_bits(18..=26),
            _ => panic!("invalid vpn level"),
        }
    }

    pub fn set_level(&mut self, level: usize, val: usize) {
        match level {
            0 => self.0.set_bits(0..=8, val),
            1 => self.0.set_bits(9..=17, val),
            2 => self.0.set_bits(18..=26, val),
            _ => panic!("invalid vpn level"),
        }
    }

    pub fn as_va(self) -> VirtAddr {
        let mut va = self.0 << 12;
        if va.get_bits(38) == 1 {
            va.set_bits(39..=63, (1 << 25) - 1);
        }
        VirtAddr::new(va)
    }
}
