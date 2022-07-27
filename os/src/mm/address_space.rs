use super::*;
use alloc::vec::Vec;
use spin::Mutex;
use frame_allocator::frame_alloc;
use crate::println;

pub static KERNEL_BASE_PAGE_TABLE: Mutex<PPN> = Mutex::new(PPN(0));
pub static KERNEL_BASE_BRK: Mutex<VPN> = Mutex::new(VPN(0));

pub fn init(kernel_base_page_table: PPN, kernel_base_brk: VPN) {
    *KERNEL_BASE_PAGE_TABLE.lock() = kernel_base_page_table;
    *KERNEL_BASE_BRK.lock() = kernel_base_brk;
    println!("kernel base brk: 0x{:x}", kernel_base_brk.0);
}

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
        let root_page_table = {
            let ppn = frame_alloc();
            println!("frame alloced, ppn: 0x{:x}", ppn.as_usize());
            // TODO: remove it
            // unsafe {
            //     let addr = (ppn.as_pa().0 as *mut u8).add(0x1ff * 8);
            //     println!("try access roote page table 0x{:x}", addr as usize);
            //     addr.write(42);
            //     println!("try access roote page table ok {}", addr.read());
            // }
            allocated_frames.push(ppn);
            ppn.as_page_table()
        };
        unsafe {
            (*root_page_table).clear();
        }

        unsafe {
            let base_ppn = KERNEL_BASE_PAGE_TABLE.lock();
            root_page_table.write(base_ppn.as_page_table().read());
        }

        Self {
            asid,
            brk: *KERNEL_BASE_BRK.lock(),
            page_table: unsafe { (*root_page_table).ppn() },
            allocated_frames,
        }
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

        self.alloc_page_at(vpn)
    }

    pub fn alloc_page_at(&mut self, vpn: VPN) -> (VPN, PPN) {
        let ppn = self.alloc_frame();
        let flags_at_level = [
            PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X | PteFlags::U,
            // PteFlags::V | PteFlags::R,
            PteFlags::V,
            PteFlags::V,
        ];
        self.build_mapping(vpn, ppn, flags_at_level);
        (vpn, ppn)
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
        println!("frame alloced, ppn: 0x{:x}", ppn.as_usize());
        self.allocated_frames.push(ppn);
        ppn
    }

    pub fn build_mapping(&mut self, vpn: VPN, ppn: PPN, flags_at_level: [PteFlags; 3]) {
        // println!("build mapping");
        let root_table = unsafe { &mut *self.page_table.as_page_table() };
        // println!("get root table done");
        let root_pte = {
            let index = vpn.level(2);
            // println!("root table: 0x{:x}", root_table as *mut _ as usize);
            // println!("access root table index 0x{:x}", index);
            let addr = unsafe { VirtAddr::new(&root_table.0[index] as *const _ as usize) };
            // println!("entry at 0x{:x} 0b{:b}", addr.0, addr.0);
            let base_table = unsafe { &*(*KERNEL_BASE_PAGE_TABLE.lock()).as_page_table() };
            // println!("translated addr 0x{:x}", base_table.translate(addr).unwrap().0);

            // println!("entry 0x{:x}", root_table.0[index].0);
            if root_table.0[index].is_valid() {
                // println!("access root table index ok");
                root_table.0[index]
            } else {
                // println!("alloc frame");
                let frame = self.alloc_frame();
                // println!("alloc frame done");
                unsafe {
                    (*frame.as_page_table()).clear();
                }
                // println!("frame cleared");
                PageTableEntry::inner(frame, flags_at_level[2])
            }
        };
        // println!("root done");

        let sub_table = unsafe { &mut *root_pte.as_page_table() };
        let sub_pte = {
            let index = vpn.level(1);
            if sub_table.0[index].is_valid() {
                sub_table.0[index]
            } else {
                let frame = self.alloc_frame();
                unsafe {
                    (*frame.as_page_table()).clear();
                }
                PageTableEntry::inner(frame, flags_at_level[1])
            }
        };
        // println!("sub done");

        let leaf_table = unsafe { &mut *sub_pte.as_page_table() };
        let leaf_pte = PageTableEntry::leaf(ppn, flags_at_level[0]);
        // println!("leaf done");
        unsafe {
            // Set entries in reverse order to avoid accessing uninit entries.
            leaf_table.set_entry(vpn.level(0), leaf_pte);
            sub_table.set_entry(vpn.level(1), sub_pte);
            root_table.set_entry(vpn.level(2), root_pte);
        }
        // println!("set entry done");
    }
}
