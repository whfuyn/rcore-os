use super::*;
use alloc::vec::Vec;
use frame_allocator::*;
use page_table::GLOBAL_PTES;
use crate::{
    config::*, trap::TrapContext, task::KernelStack,
};
// use crate::println;

#[derive(Debug)]
pub struct AddressSpace {
    asid: usize,
    brk: VPN,
    page_table: PPN,
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
            brk: KERNEL_BRK_VA.vpn(),
            page_table: root_page_table.ppn(),
            allocated_frames,
        }
    }

    // Return the address space and kernel stack pointer
    pub fn from_elf(elf_data: &[u8], asid: usize) -> Self {
        let elf = xmas_elf::ElfFile::new(elf_data).expect("invalid elf data");
        let magic = elf.header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf");
        let entry_point = elf.header.pt2.entry_point();

        let mut addr_space = Self::new(asid);

        for ph in elf.program_iter() {
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va = VirtAddr::new(ph.virtual_addr() as usize);
                let end_va = VirtAddr::new((ph.virtual_addr() + ph.mem_size()) as usize);
                let flags_at_level = {
                    let mut flags_at_level = [
                        PteFlags::user_leaf(),
                        PteFlags::user_inner(),
                        PteFlags::user_inner(),
                    ];
                    let ph_flags = ph.flags();
                    if ph_flags.is_read() {
                        flags_at_level[0] |= PteFlags::R;
                    }
                    if ph_flags.is_write() {
                        // W imply R
                        flags_at_level[0] |= PteFlags::R;
                        flags_at_level[0] |= PteFlags::W;
                    }
                    if ph_flags.is_execute() {
                        flags_at_level[0] |= PteFlags::X;
                    }
                    flags_at_level
                };
                let mut mapped_size = 0;
                let mut mapped_va = start_va;
                while mapped_va.0 < end_va.0 {
                    let frame = addr_space.alloc_frame();
                    if mapped_size < ph.file_size() {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                &elf_data[(ph.offset() + mapped_size) as usize] as *const u8,
                                frame.as_pa().0 as *mut u8,
                                core::cmp::min(ph.file_size().checked_sub(mapped_size).unwrap(), 4096) as usize
                            );
                        }
                    }
                    addr_space.build_mapping(mapped_va.vpn(), frame, flags_at_level);
                    mapped_va.0 += 4096;
                    mapped_size += 4096;
                }
            }
        }

        let mut mapped_size = 0;
        while mapped_size < USER_STACK_SIZE {
            addr_space.alloc_page_for(USER_STACK_VA.add(mapped_size).vpn());
            mapped_size += PAGE_SIZE;
        }
        let usp = USER_STACK_VA.add(USER_STACK_SIZE);

        let mut mapped_size = 0;
        let mut kstack_ppn = PPN(0);
        while mapped_size < KERNEL_STACK_SIZE {
            kstack_ppn = addr_space.alloc_kernel_page_for(KERNEL_STACK_VA.add(mapped_size).vpn());
            mapped_size += PAGE_SIZE;
        }

        // let kstack =  unsafe { &mut *(kstack_ppn.as_pa().0 as *mut KernelStack) };
        let trap_cx_ptr = 
            (kstack_ppn.as_pa().0 + PAGE_SIZE - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        let task_init_trap_cx = TrapContext::app_init_context(
            entry_point as usize, usp.0
        );
        unsafe {
            trap_cx_ptr.write(task_init_trap_cx);
        }
        // kstack.push_context(task_init_trap_cx);

        addr_space
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

        let ppn = self.alloc_kernel_page_for(vpn);
        (vpn, ppn)
    }

    pub fn alloc_kernel_page_for(&mut self, vpn: VPN) -> PPN {
        let ppn = self.alloc_frame();
        let flags_at_level = [
            PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X,
            PteFlags::V,
            PteFlags::V,
        ];
        self.build_mapping(vpn, ppn, flags_at_level);
        ppn
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
            if !root_pte.is_valid() || index == global_index[0] || index == global_index[1] {
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
                if !sub_pte.is_valid() {
                    continue;
                }
                let leaf_table = unsafe { sub_pte.as_page_table() };
                let new_leaf_table = {
                    let new_leaf_ppn = new.alloc_page_table();
                    unsafe {
                        new_sub_table.set_entry(index, sub_pte.with_ppn(new_leaf_ppn));
                        new_leaf_ppn.as_page_table_mut()
                    }
                };
                for (index, leaf_pte) in leaf_table.0.iter().enumerate() {
                    if !leaf_pte.is_valid() {
                        continue;
                    }
                    let leaf_page = leaf_pte.ppn();
                    let new_leaf_page = new.alloc_frame();
                    unsafe {
                        // Use the identity mapping of physical memory
                        crate::println!("leaf_page 0x{:x}\nnew_leaf_page 0x{:x}", leaf_page.as_pa().0, new_leaf_page.as_pa().0);
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
        crate::println!("dup ok");

        new
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        self.allocated_frames.drain(..)
            .for_each(|frame| frame_free(frame));
    }
}
