use super::*;
use alloc::vec::Vec;
use frame_allocator::*;
use page_table::GLOBAL_PTES;
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
        let addr_space = Self::new(asid);

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

    fn alloc_page_table(&mut self) -> PPN {
        let ppn = frame_alloc();
        unsafe {
            ppn.as_page_table_mut().clear();
        }
        self.allocated_frames.push(ppn);
        ppn
    }

    pub fn translate(&self, va: VirtAddr) -> Option<PhysAddr> {
        unsafe {
            (*self.page_table.as_page_table()).translate(va)
        }
    }

    pub fn build_mapping(&mut self, vpn: VPN, ppn: PPN, flags_at_level: [PteFlags; 3]) {
        let root_table = unsafe { self.page_table.as_page_table_mut() };
        let root_pte = {
            let index = vpn.level(2);
            if root_table.0[index].is_valid() {
                root_table.0[index]
            } else {
                let frame = self.alloc_page_table();
                PageTableEntry::inner(frame, flags_at_level[2])
            }
        };

        let sub_table = unsafe { root_pte.as_page_table_mut() };
        let sub_pte = {
            let index = vpn.level(1);
            if sub_table.0[index].is_valid() {
                sub_table.0[index]
            } else {
                let frame = self.alloc_page_table();
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

    pub fn dup(&self, asid: usize) -> Self {
        let mut new = Self::new(asid) ;
        new.brk = self.brk;

        let global_index = {
            let global_ptes = GLOBAL_PTES.lock();
            [global_ptes.kernel_pte_index, global_ptes.memory_pte_index]
        };
        let root_table = unsafe { self.page_table.as_page_table() };
        let new_root_table = unsafe { new.page_table.as_page_table_mut() };
        for (index, root_pte) in root_table.0.iter().enumerate() {
            if index == global_index[0] || index == global_index[1] {
                continue;
            }
            let sub_table = unsafe { root_pte.as_page_table() };
            let new_sub_table = {
                let new_sub_ppn = new.alloc_page_table();
                unsafe {
                    new_root_table.set_entry(index, root_pte.with_ppn(new_sub_ppn));
                    new_sub_ppn.as_page_table_mut()
                }
            };

            for (index, sub_pte) in sub_table.0.iter().enumerate() {
                let leaf_table = unsafe { sub_pte.as_page_table() };
                let new_leaf_table = {
                    let new_leaf_ppn = new.alloc_page_table();
                    unsafe {
                        new_sub_table.set_entry(index, sub_pte.with_ppn(new_leaf_ppn));
                        new_leaf_ppn.as_page_table_mut()
                    }
                };
                for (index, leaf_pte) in leaf_table.0.iter().enumerate() {
                    let leaf_page = leaf_pte.ppn();
                    let new_leaf_page = new.alloc_frame();
                    unsafe {
                        // Use the identity mapping of physical memory
                        core::ptr::copy_nonoverlapping(
                            leaf_page.as_pa().0 as *const u8, 
                            new_leaf_page.as_pa().0 as *mut u8,
                            4096
                        );
                        new_leaf_table.set_entry(index, leaf_pte.with_ppn(new_leaf_page))
                    }
                }
            }
        }

        new
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        self.allocated_frames.drain(..)
            .for_each(|frame| frame_free(frame));
    }
}
