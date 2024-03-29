use clap::{App, Arg};
use easy_fs::{BlockDevice, EasyFileSystem, Inode};
use std::fs::{read_dir, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

const BLOCK_SZ: usize = 512;

struct BlockFile(Mutex<File>);

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.read(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.write(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }

    fn handle_irq(&self) {
        unimplemented!();
    }
}

fn main() {
    easy_fs_pack().expect("Error when packing easy-fs!");
    //easy_fs_read_metadata().expect("Error read fs.img");
}

fn easy_fs_read_metadata() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new().read(true).write(true).open("fs.img")?;
        f
    })));
    let efs = EasyFileSystem::open(block_file.clone());
    //Read Super block
    let root_inode = EasyFileSystem::root_inode(&efs);
    println!("start read metadata");
    let super_block = EasyFileSystem::read_super_block(block_file.clone());
    println!("super block : {:#?}", super_block);

    //Read inode bitmap
    let first_inode_bitmap = EasyFileSystem::read_inode_bitmap(&efs.lock());
    println!("first inode bitmap : [");
    for i in 0..first_inode_bitmap.len() {
        //he 018 pads with zeros to a width of 18. That width includes 0b (length=2) plus a u16 (length=16) so 18 = 2 + 16. It must come between # and b.
        println!("{:#066b},", first_inode_bitmap[i]);
    }
    println!("]");

    //Read data bitmap
    let first_data_bitmap = EasyFileSystem::read_data_bitmap(&efs.lock());
    println!("first data bitmap : [");
    for i in 0..first_data_bitmap.len() {
        //he 018 pads with zeros to a width of 18. That width includes 0b (length=2) plus a u16 (length=16) so 18 = 2 + 16. It must come between # and b.
        println!("{:#066b},", first_data_bitmap[i]);
    }
    println!("]");

    //Show inode area
    let inode_areas = EasyFileSystem::read_available_inode_areas(&efs.lock());
    println!("inode area blocks : {:#?}", inode_areas);

    //Show directory data block
    // let start_data_block_id = super_block.total_blocks - super_block.data_area_blocks;
    for block_id in [1704, 1705, 1706, 1707] {
        let data_block = EasyFileSystem::read_data_area(&efs.lock(), block_id as usize);
        println!("block id [{:#}] data {:#?}", block_id, data_block);
    }
    // Specified [block id 1191] indirect2
    // for block_id in [1192,1321,1450,1579]{
    // let indirect_block = EasyFileSystem::read_indirect_block(&efs.lock(), block_id as usize);
    //     println!("block id [{:#}] data {:#?}", block_id, indirect_block);
    // }

    //Show filename
    // list apps
    for app in root_inode.ls() {
        let file = root_inode.find(app.as_str()).unwrap();
        println!(
            "file name :[{}] , size : [{} Bytes]",
            app,
            file.get_inode_size()
        );
    }

    return Ok(());
}

fn tree(inode: &Arc<Inode>, name: &str, depth: usize) {
    if depth > 0 {
        print!("|")
    }
    for _ in 0..depth {
        print!("-");
    }
    println!("{}", name);
    for name in inode.ls() {
        let child = inode.find(&name).unwrap();
        tree(&child, &name, depth + 1);
    }
}

#[allow(dead_code)]
fn easy_fs_pack() -> std::io::Result<()> {
    let matches = App::new("EasyFileSystem packer")
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .takes_value(true)
                .help("Executable source dir(with backslash)"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .help("Executable target dir(with backslash)"),
        )
        .get_matches();
    let src_path = matches.value_of("source").unwrap();
    let target_path = matches.value_of("target").unwrap();
    println!("src_path={}\ntarget_path={}", src_path, target_path);
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(format!("{}{}", target_path, "fs.img"))?;
        f.set_len(16 * 2048 * 512).unwrap();
        f
    })));
    // 16MiB, at most 4096 files
    let efs = EasyFileSystem::create(block_file, 16 * 2048, 1);
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    let apps: Vec<_> = read_dir(src_path)
        .unwrap()
        .into_iter()
        .map(|dir_entry| {
            let mut name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();
            name_with_ext.drain(name_with_ext.find('.').unwrap()..name_with_ext.len());
            return name_with_ext;
        })
        .collect();
    for app in apps {
        // load app data from host file system
        let mut host_file = File::open(format!("{}{}", target_path, app)).unwrap();
        let mut all_data: Vec<u8> = Vec::new();
        host_file.read_to_end(&mut all_data).unwrap();
        // create a file in easy-fs
        let inode = root_inode.create(app.as_str()).unwrap();
        // write data to easy-fs
        inode.write_at(0, all_data.as_slice());
    }
    // list apps
    for app in root_inode.ls() {
        println!("{}", app);
    }
    // add dir A
    let dir_a = root_inode.create_dir("dira").unwrap();
    let file_c = dir_a.create("filec").unwrap();
    let dir_b = dir_a.create_dir("dirb").unwrap();
    let file_d = dir_b.create("filed").unwrap();

    let file_c_content = "3333333";
    let file_d_content = "4444444444444444444";
    file_c.write_at(0, file_c_content.as_bytes());
    file_d.write_at(0, file_d_content.as_bytes());
    
    Ok(())
}

#[test]
fn efs_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("target/fs.img")?;
        f.set_len(8192 * 512).unwrap();
        f
    })));
    EasyFileSystem::create(block_file.clone(), 4096, 1);
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = EasyFileSystem::root_inode(&efs);
    root_inode.create("filea");
    root_inode.create("fileb");
    root_inode.create("filec");
    // for i in 0..100 {
    //     root_inode.create(format!("file-{:#}", i).as_str());
    // }

    for name in root_inode.ls() {
        println!("{}", name);
    }
    let filea = root_inode.find("filea").unwrap();
    let fileb = root_inode.find("fileb").unwrap();
    let filec = root_inode.find("filec").unwrap();
    let greet_str = "Hello, world!";
    let greet_str1 = "primary NG";
    let greet_str2 = "sublime text";
    filea.write_at(0, greet_str.as_bytes());
    fileb.write_at(0, greet_str1.as_bytes());
    filec.write_at(0, greet_str2.as_bytes());
    let mut buffer = [0u8; 233];
    let len = filea.read_at(0, &mut buffer);
    assert_eq!(greet_str, core::str::from_utf8(&buffer[..len]).unwrap());

    let mut random_str_test = |len: usize| {
        filea.clear();
        assert_eq!(filea.read_at(0, &mut buffer), 0);
        let mut str = String::new();
        use rand;
        for _ in 0..len {
            str.push(char::from('0' as u8 + rand::random::<u8>() % 10));
        }
        filea.write_at(0, str.as_bytes());
        let mut read_buffer = [0u8; 127];
        let mut offset = 0usize;
        let mut read_str = String::new();
        loop {
            let len = filea.read_at(offset, &mut read_buffer);
            if len == 0 {
                break;
            }
            offset += len;
            read_str.push_str(core::str::from_utf8(&read_buffer[..len]).unwrap());
        }
        assert_eq!(str, read_str);
    };

    // random_str_test(4 * BLOCK_SZ);
    // random_str_test(8 * BLOCK_SZ + BLOCK_SZ / 2);
    // random_str_test(100 * BLOCK_SZ);
    // random_str_test(70 * BLOCK_SZ + BLOCK_SZ / 7);
    // random_str_test((12 + 128) * BLOCK_SZ);
    // random_str_test(400 * BLOCK_SZ);
    // random_str_test(1000 * BLOCK_SZ);
    // random_str_test(2000 * BLOCK_SZ);

    Ok(())
}

#[test]
fn efs_dir_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("target/fs.img")?;
        f.set_len(8192 * 512).unwrap();
        f
    })));
    EasyFileSystem::create(block_file.clone(), 4096, 1);
    let efs = EasyFileSystem::open(block_file.clone());
    let root = Arc::new(EasyFileSystem::root_inode(&efs));
    root.create("filea");
    root.create("fileb");

    let dir_a = root.create_dir("dira").unwrap();
    let file_c = dir_a.create("filec").unwrap();
    let dir_b = dir_a.create_dir("dirb").unwrap();
    let file_d = dir_b.create("filed").unwrap();
    tree(&root, "/", 0);

    let file_c_content = "3333333";
    let file_d_content = "4444444444444444444";
    file_c.write_at(0, file_c_content.as_bytes());
    file_d.write_at(0, file_d_content.as_bytes());

    Ok(())
}
