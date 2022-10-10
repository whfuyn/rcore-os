#![feature(let_else)]

use easy_fs::EasyFileSystem;
use easy_fs::BlockDevice;
use easy_fs::BlockCacheManager;
use easy_fs::Block;
use easy_fs::BLOCK_SIZE;
use easy_fs::Directory;
use easy_fs::FileOrDirectory;
use clap::Parser;
use clap::Subcommand;
use std::fs::File;
use std::sync::Mutex;
use std::path::Path;
use std::fs;
use std::io::{
    Read, Write, Seek, SeekFrom,
};
use static_assertions::const_assert_eq;
use anyhow::Result;

const INODE_BITMAP_BLOCKS: u32 = 1;
const INODE_AREA_BLOCKS: u32 = 4094;
const DATA_BITMAP_BLOCKS: u32 = 1;
const DATA_AREA_BLOCKS: u32 = 4095;
const TOTAL_BLOCKS: u32 = 1 + INODE_BITMAP_BLOCKS + INODE_AREA_BLOCKS + DATA_BITMAP_BLOCKS + DATA_AREA_BLOCKS;
const_assert_eq!(TOTAL_BLOCKS, 8192);

const BLOCK_FILE_SIZE: usize = TOTAL_BLOCKS as usize * BLOCK_SIZE;


struct BlockFile(Mutex<File>);

impl BlockFile {
    fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
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

    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::options()
            .read(true)
            .write(true)
            .open(path)?;
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
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    commands: Commands
}

#[derive(Debug, Subcommand)]
enum Commands {
    Pack {
        #[arg(short, long)]
        src_dir: String,
        #[arg(short, long, default_value_t = String::from("easy-fs.img"))]
        out_img: String,
    },
    Unpack {
        #[arg(short, long, default_value_t = String::from("easy-fs.img"))]
        src_img: String,
        #[arg(short, long)]
        out_dir: String,
    },
}

fn pack_dir<P: AsRef<Path>>(src_dir: P, out_img: &Directory) -> Result<()> {
    let dir = fs::read_dir(&src_dir)?;
    for ent in dir {
        let ent = ent?;
        let name = ent.file_name().into_string().unwrap();
        if ent.file_type()?.is_dir() {
            let dir_in_img = out_img.create_dir(&name).unwrap();
            pack_dir(ent.path(), &dir_in_img)?;
        } else {
            let file_in_img = out_img.create_file(&name).unwrap();
            let mut file = File::open(ent.path())?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            file_in_img.write_at(0, &buf);
        }
    }
    Ok(())
}

fn unpack_dir<P: AsRef<Path>>(src_img: &Directory, out_dir: P) -> Result<()> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(&out_dir)?;

    let ents = src_img.list();
    for ent in ents {
        let out_path = out_dir.join(&ent);
        match src_img.open(&ent).unwrap() {
            FileOrDirectory::Directory(dir_in_img) => {
                fs::create_dir_all(&out_path)?;
                unpack_dir(&dir_in_img, out_path)?;
            }
            FileOrDirectory::File(file_in_img) => {
                let mut out_file = File::create(out_path)?;
                let mut buf = [0u8; BLOCK_SIZE];
                let total = file_in_img.size();
                let mut offset = 0;
                while offset < total {
                    let n = file_in_img.read_at(offset, &mut buf);
                    assert_ne!(n, 0);
                    out_file.write_all(&buf[..n])?;
                    offset += n;
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.commands {
        Commands::Pack {
            src_dir,
            out_img
        } => {
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
            pack_dir(src_dir, &root_dir).unwrap();
            println!("{:#?}", root_dir.list());
        }
        Commands::Unpack {
            out_dir,
            src_img
        } => {
            let block_file = BlockFile::open(src_img).unwrap();
            let cache_mgr = BlockCacheManager::new(block_file);
            let Ok(efs) = EasyFileSystem::open(cache_mgr) else { panic!("fail to open efs") };
            let root_dir = efs.open_root_dir().unwrap();
            unpack_dir(&root_dir, out_dir).unwrap();
        }
    }
    Ok(())
}
