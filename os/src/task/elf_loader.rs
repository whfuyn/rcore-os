use core::arch::global_asm;
use core::ffi::CStr;
use core::ffi::c_char;
use alloc::collections::BTreeMap;
use lazy_static::lazy_static;

global_asm!(include_str!("../link_app.S"));
extern "C" {
    static _num_app: usize;
    static _app_names: usize;
}

lazy_static! {
    static ref ELF_LOADER: ElfLoader = unsafe { ElfLoader::new() };
}

pub fn get_app_data(name: &str) -> Option<&'static [u8]> {
    ELF_LOADER.get_elf(name)
}

struct ElfLoader {
    elfs: BTreeMap<&'static str, &'static [u8]>
}

impl ElfLoader {
    unsafe fn new() -> Self {
        let elfs = BTreeMap::new();

        let ptr = _num_app as *const usize;
        let num_app = *ptr;
        let app_starts = {
            let table = ptr.add(1);
            // The last one is a marker for the end.
            core::slice::from_raw_parts(table, num_app + 1)
        };

        let mut ptr = _app_names as *const c_char;
        for i in 0..num_app {
            let name: &'static str = CStr::from_ptr(ptr).to_str().expect("invalid app name");
            ptr = ptr.add(name.len() + 1);
            let elf: &'static [u8] = {
                let start = app_starts[i];
                let end = app_starts[i + 1];
                let len = end - start;
                core::slice::from_raw_parts(start as *const u8, len)
            };
            elfs.insert(name, elf);
        }
        Self { elfs }
    }

    pub fn get_elf(&self, name: &str) -> Option<&'static [u8]> {
        self.elfs.get(name).map(|r| *r)
    }
}
