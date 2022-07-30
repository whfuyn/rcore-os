use super::*;
use alloc::vec::Vec;
use spin::Mutex;
use frame_allocator::*;
use crate::config::*;
use riscv::register::satp;
// use crate::println;

pub struct AddressSpace {
    asid: usize,
    brk: VPN,
    // TODO: no pub
    pub page_table: PPN,
    allocated_frames: Vec<PPN>,
}

impl AddressSpace {
    pub fn new(asid: usize) -> Self {
        let mut allocated_frames = Vec::new();
        let root_page_table = unsafe {
            let ppn = frame_alloc();
            allocated_frames.push(ppn);
            ppn.as_page_table_mut()
        };
        root_page_table.clear();
        root_page_table.add_globals();

        Self {
            asid,
            // Be careful not to overlap with global mapping (esp. 1 GB pte).
            brk: VPN(0x70000),
            page_table: root_page_table.ppn(),
            allocated_frames,
        }
    }

    pub fn from_elf(elf: &[u8], asid: usize) -> Self {

        todo!()
    }

    pub fn fork(&self) -> Self {
        todo!()
    }

    pub fn satp(&self) -> usize {
        let mut satp = 0usize;
        satp.set_bits(60..=63, 8);
        satp.set_bits(44..=59, self.asid);
        satp.set_bits(0..=43, self.page_table.0);

        satp
    }

    pub fn alloc_page(&mut self) -> (VPN, PPN) {
        let vpn = self.brk;
        self.brk.0 += 1;

        (vpn, self.alloc_page_for(vpn))
    }

    pub fn alloc_page_for(&mut self, vpn: VPN) -> PPN {
        let ppn = self.alloc_frame();
        let flags_at_level = [
            PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X | PteFlags::U,
            PteFlags::V,
            PteFlags::V,
        ];
        self.build_mapping(vpn, ppn, flags_at_level);
        ppn
    }

    pub fn alloc_kernel_page(&mut self) -> (VPN, PPN) {
        let vpn = self.brk;
        self.brk.0 += 1;

        let ppn = self.alloc_frame();
        let flags_at_level = [
            PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X,
            PteFlags::V,
            PteFlags::V,
        ];
        self.build_mapping(vpn, ppn, flags_at_level);
        (vpn, ppn)
    }

    pub fn alloc_frame(&mut self) -> PPN {
        let ppn = frame_alloc();
        self.allocated_frames.push(ppn);
        ppn
    }

    pub fn build_mapping(&mut self, vpn: VPN, ppn: PPN, flags_at_level: [PteFlags; 3]) {
        let root_table = unsafe { self.page_table.as_page_table_mut() };
        let root_pte = {
            let index = vpn.level(2);
            if root_table.0[index].is_valid() {
                root_table.0[index]
            } else {
                let frame = self.alloc_frame();
                unsafe {
                    frame.as_page_table_mut().clear();
                }
                PageTableEntry::inner(frame, flags_at_level[2])
            }
        };

        let sub_table = unsafe { root_pte.as_page_table_mut() };
        let sub_pte = {
            let index = vpn.level(1);
            if sub_table.0[index].is_valid() {
                sub_table.0[index]
            } else {
                let frame = self.alloc_frame();
                unsafe {
                    frame.as_page_table_mut().clear();
                }
                PageTableEntry::inner(frame, flags_at_level[1])
            }
        };

        let leaf_table = unsafe { sub_pte.as_page_table_mut() };
        let leaf_pte = PageTableEntry::leaf(ppn, flags_at_level[0]);
        unsafe {
            // Set entries in reverse order to avoid accessing uninit entries.
            leaf_table.set_entry(vpn.level(0), leaf_pte);
            sub_table.set_entry(vpn.level(1), sub_pte);
            root_table.set_entry(vpn.level(2), root_pte);
        }
        // println!("map 0x{:x} to 0x{:x}", vpn.as_va().0, ppn.as_pa().0);
    }

    pub fn translate(&self, va: VirtAddr) -> Option<PhysAddr> {
        unsafe {
            (*self.page_table.as_page_table()).translate(va)
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        self.allocated_frames.drain(..)
            .for_each(|frame| frame_free(frame));
    }
}
