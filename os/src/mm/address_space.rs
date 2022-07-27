use super::*;
use alloc::vec::Vec;
use spin::Mutex;
use frame_allocator::frame_alloc;

pub static KERNEL_BASE_PAGE_TABLE: Mutex<PPN> = Mutex::new(PPN(0));
pub static KERNEL_BASE_BRK: Mutex<VPN> = Mutex::new(VPN(0));

pub fn init(kernel_base_page_table: PPN, kernel_base_brk: VPN) {
    *KERNEL_BASE_PAGE_TABLE.lock() = kernel_base_page_table;
    *KERNEL_BASE_BRK.lock() = kernel_base_brk;
}

pub struct AddressSpace {
    asid: usize,
    brk: VPN,
    page_table: PPN,
    allocated_frames: Vec<PPN>,
}

impl AddressSpace {
    pub fn new(asid: usize) -> Self {
        let mut allocated_frames = Vec::new();
        let root_page_table = {
            let ppn = frame_alloc();
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

    pub fn alloc_page(&mut self) -> (VPN, PPN) {
        let vpn = self.brk;
        self.brk.0 += 1;

        let ppn = self.alloc_frame();
        let flags_at_level = [
            PteFlags::V | PteFlags::R | PteFlags::W | PteFlags::X | PteFlags::U,
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
        self.allocated_frames.push(ppn);
        ppn
    }

    pub fn build_mapping(&mut self, vpn: VPN, ppn: PPN, flags_at_level: [PteFlags; 3]) {
        let root_table = unsafe { &mut *self.page_table.as_page_table() };
        let root_pte = {
            let index = vpn.level(2);
            if root_table.0[index].is_valid() {
                root_table.0[index]
            } else {
                let frame = self.alloc_frame();
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
                let frame = self.alloc_frame();
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
            root_table.set_entry(vpn.level(2), root_pte);
        }
    }
}
