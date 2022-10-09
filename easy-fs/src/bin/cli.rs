use easy_fs::EasyFileSystem;
use easy_fs::BlockDevice;
use easy_fs::BlockCacheManager;
use easy_fs::Block;
use easy_fs::BLOCK_SIZE;
use easy_fs::Directory;
use clap::Parser;
use std::fs::File;
use std::sync::Mutex;
use std::path::Path;
use std::fs;
use std::io::{
    self, Read, Write, Seek, SeekFrom,
};
use static_assertions::const_assert_eq;

const INODE_BITMAP_BLOCKS: u32 = 1;
const INODE_AREA_BLOCKS: u32 = 4094;
const DATA_BITMAP_BLOCKS: u32 = 1;
const DATA_AREA_BLOCKS: u32 = 4095;
const TOTAL_BLOCKS: u32 = 1 + INODE_BITMAP_BLOCKS + INODE_AREA_BLOCKS + DATA_BITMAP_BLOCKS + DATA_AREA_BLOCKS;
const_assert_eq!(TOTAL_BLOCKS, 8192);

const BLOCK_FILE_SIZE: usize = TOTAL_BLOCKS as usize * BLOCK_SIZE;


struct BlockFile(Mutex<File>);

impl BlockFile {
    fn create<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let path = path.as_ref();
        let file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        file.set_len(BLOCK_FILE_SIZE as u64)?;
        Ok(BlockFile(Mutex::new(file)))
    }
}

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: usize, buf: &mut Block) {
        let mut f = self.0.lock().unwrap();
        let pos = SeekFrom::Start((block_id * BLOCK_SIZE).try_into().unwrap());
        f.seek(pos).unwrap();
        f.read_exact(buf).unwrap();
    }

    fn write_block(&self, block_id: usize, buf: &Block) {
        let mut f = self.0.lock().unwrap();
        let pos = SeekFrom::Start((block_id * BLOCK_SIZE).try_into().unwrap());
        f.seek(pos).unwrap();
        f.write_all(buf).unwrap();
    }
}

#[derive(Debug, Parser)]
struct CliArgs {
    #[arg(short, long)]
    target_dir: String,
    #[arg(short, long, default_value_t = String::from("easy-fs.img"))]
    out_img: String,
}


fn clone_dir<P: AsRef<Path>>(target_dir: P, img_dir: &Directory) -> Result<(), io::Error> {
    let dir = fs::read_dir(&target_dir)?;
    for ent in dir {
        let ent = ent?;
        let name = ent.file_name().into_string().unwrap();
        if ent.file_type()?.is_dir() {
            let sub_img_dir = img_dir.create_dir(&name).unwrap();
            clone_dir(ent.path(), &sub_img_dir)?;
        } else {
            let file_in_img = img_dir.create_file(&name).unwrap();
            let mut file = File::open(ent.path())?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            file_in_img.write_at(0, &buf);
        }
    }
    Ok(())
}

fn main() {
    let args = CliArgs::parse();
    let out_img = &args.out_img;
    let target_dir = &args.target_dir;

    let block_file = BlockFile::create(out_img).unwrap();
    let cache_mgr = BlockCacheManager::new(block_file);
    let efs = EasyFileSystem::create(
        cache_mgr,
        TOTAL_BLOCKS,
        INODE_BITMAP_BLOCKS,
        INODE_AREA_BLOCKS,
        DATA_BITMAP_BLOCKS,
        DATA_AREA_BLOCKS
    );

    let root_dir = efs.create_root_dir().unwrap();
    clone_dir(target_dir, &root_dir).unwrap();
    println!("{:#?}", root_dir.list());
}
