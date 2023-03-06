use crate::{bitmap::BitmapBlock, vfs::Inode, BLOCK_SZ};

use super::{
    block_cache_sync_all, get_block_cache, Bitmap, BlockDevice, DiskInode, DiskInodeType,
    SuperBlock,
};
use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;

///An easy file system on block
pub struct EasyFileSystem {
    ///Real device
    pub block_device: Arc<dyn BlockDevice>,
    ///Inode bitmap
    pub inode_bitmap: Bitmap,
    ///Data bitmap
    pub data_bitmap: Bitmap,
    inode_area_start_block: u32,
    data_area_start_block: u32,
}
/// A indirect block
type IndirectBlock = [u32; BLOCK_SZ / 4];
/// A data block
type DataBlock = [u8; BLOCK_SZ];
/// An easy fs over a block device
impl EasyFileSystem {
    /// A data block of block size
    pub fn create(
        block_device: Arc<dyn BlockDevice>,
        total_blocks: u32,        //在一个磁盘中, 块的总数
        inode_bitmap_blocks: u32, //在一个磁盘中, 索引位图块的数量
    ) -> Arc<Mutex<Self>> {
        // calculate block size of areas & create bitmaps
        let inode_bitmap = Bitmap::new(1, inode_bitmap_blocks as usize);
        let inode_num = inode_bitmap.maximum(); // 一个索引位图块 能指向64*64=4096个索引节点块
        let inode_area_blocks =
            ((inode_num * core::mem::size_of::<DiskInode>() + BLOCK_SZ - 1) / BLOCK_SZ) as u32; //计算索引节点Inode 需要多少磁盘块, 先算出内存大小, 每个磁盘块存512bit
        let inode_total_blocks = inode_bitmap_blocks + inode_area_blocks; //索引区域总共需要多少磁盘块 = 索引节点所需磁盘块 加上 索引位图所需磁盘块
        let data_total_blocks = total_blocks - 1 - inode_total_blocks; //剩下都是数据总磁盘块 这里减一是减去了超级块
        let data_bitmap_blocks = (data_total_blocks + 4096) / 4097; //?? 计算数据块位图 需要多少磁盘块
        let data_area_blocks = data_total_blocks - data_bitmap_blocks; // 再算出数据区域 需要多少磁盘块
        let data_bitmap = Bitmap::new(
            (1 + inode_bitmap_blocks + inode_area_blocks) as usize, //这里的1+ 也是超级块的位置
            data_bitmap_blocks as usize,
        );
        let mut efs = Self {
            block_device: Arc::clone(&block_device),
            inode_bitmap,
            data_bitmap,
            inode_area_start_block: 1 + inode_bitmap_blocks, //计算索引节点区域的起始块编号  最前面的1是超级块
            data_area_start_block: 1 + inode_total_blocks + data_bitmap_blocks, //计算数据区域的起始块编号
        };
        // clear all blocks
        for i in 0..total_blocks {
            get_block_cache(i as usize, Arc::clone(&block_device))
                .lock()
                .modify(0, |data_block: &mut DataBlock| {
                    for byte in data_block.iter_mut() {
                        *byte = 0;
                    }
                });
        }
        // initialize SuperBlock
        get_block_cache(0, Arc::clone(&block_device)).lock().modify(
            0,
            |super_block: &mut SuperBlock| {
                super_block.initialize(
                    total_blocks,
                    inode_bitmap_blocks,
                    inode_area_blocks,
                    data_bitmap_blocks,
                    data_area_blocks,
                );
            },
        );
        //write back immediately
        // create a inode for root node "/"
        assert_eq!(efs.alloc_inode(), 0); //请求分配一个索引节点的磁盘块, 给根目录分配的
        let (root_inode_block_id, root_inode_offset) = efs.get_disk_inode_pos(0); //根目录的inode编号 固定为0
        get_block_cache(root_inode_block_id as usize, Arc::clone(&block_device))
            .lock()
            .modify(root_inode_offset, |disk_inode: &mut DiskInode| {
                disk_inode.initialize(DiskInodeType::Directory); //给根目录写上数据
            });
        block_cache_sync_all(); //写入磁盘
        return Arc::new(Mutex::new(efs));
    }
    /// Open a block device as a filesystem
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Mutex<Self>> {
        // read SuperBlock
        // 这里最核心的是参数block_id=0, 指定了超级块的编号也就是编号0.
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .read(0, |super_block: &SuperBlock| {
                assert!(super_block.is_valid(), "Error Loading EFS!"); //读出了超级块
                let inode_total_blocks =
                    super_block.inode_bitmap_blocks + super_block.inode_area_blocks;
                let efs = Self {
                    //构造了EasyFileSystem实例
                    block_device,
                    inode_bitmap: Bitmap::new(1, super_block.inode_bitmap_blocks as usize),
                    data_bitmap: Bitmap::new(
                        (1 + inode_total_blocks) as usize,
                        super_block.data_bitmap_blocks as usize,
                    ),
                    inode_area_start_block: 1 + super_block.inode_bitmap_blocks,
                    data_area_start_block: 1 + inode_total_blocks + super_block.data_bitmap_blocks,
                };
                return Arc::new(Mutex::new(efs));
            })
    }
    /// Get the root inode of the filesystem
    pub fn root_inode(efs: &Arc<Mutex<Self>>) -> Inode {
        let block_device = Arc::clone(&efs.lock().block_device);
        // acquire efs lock temporarily
        let (block_id, block_offset) = efs.lock().get_disk_inode_pos(0); //这里inode_id=0 意味着根目录
                                                                         // release efs lock
        return Inode::new(block_id, block_offset, Arc::clone(efs), block_device);
    }
    /// Get inode by id
    pub fn get_disk_inode_pos(&self, inode_id: u32) -> (u32, usize) {
        let inode_size = core::mem::size_of::<DiskInode>();
        let inodes_per_block = (BLOCK_SZ / inode_size) as u32;
        let block_id = self.inode_area_start_block + inode_id / inodes_per_block;
        (
            block_id,
            (inode_id % inodes_per_block) as usize * inode_size,
        )
    }
    /// Get data block by id
    pub fn get_data_block_id(&self, data_block_id: u32) -> u32 {
        self.data_area_start_block + data_block_id
    }
    /// Allocate a new inode
    pub fn alloc_inode(&mut self) -> u32 {
        self.inode_bitmap.alloc(&self.block_device).unwrap() as u32
    }
    /// Allocate a data block
    pub fn alloc_data(&mut self) -> u32 {
        self.data_bitmap.alloc(&self.block_device).unwrap() as u32 + self.data_area_start_block
    }
    /// Deallocate a data block
    pub fn dealloc_data(&mut self, block_id: u32) {
        // 根据指定的block_id 把这个磁盘块里面内容置零
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(0, |data_block: &mut DataBlock| {
                data_block.iter_mut().for_each(|p| {
                    *p = 0;
                })
            });
        // 释放对应的数据块位图
        self.data_bitmap.dealloc(
            &self.block_device,
            (block_id - self.data_area_start_block) as usize,
        )
    }

    /********Only from Testing and explore ****************/
    /// Open a block device and read out super block
    pub fn read_super_block(block_device: Arc<dyn BlockDevice>) -> SuperBlock {
        // 这里最核心的是参数block_id=0, 指定了超级块的编号也就是编号0.
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .read(0, |super_block: &SuperBlock| {
                assert!(super_block.is_valid(), "Error Loading EFS!"); //读出了超级块
                return SuperBlock::new(
                    super_block.magic,
                    super_block.total_blocks,
                    super_block.inode_bitmap_blocks,
                    super_block.inode_area_blocks,
                    super_block.data_bitmap_blocks,
                    super_block.data_area_blocks,
                );
            })
    }

    /// show inode all data BitmapBLock : [u64; 64]
    pub fn read_inode_bitmap(&self) -> BitmapBlock {
        return self
            .inode_bitmap
            .read_first_bitmap_block(&self.block_device);
    }

    /// show data all data BitmapBLock : [u64; 64]
    pub fn read_data_bitmap(&self) -> BitmapBlock {
        return self.data_bitmap.read_first_bitmap_block(&self.block_device);
    }

    /// show inode area
    /// TODO : add new input param : inode_area_blocks  
    /// TODO :  short-circuit when disk_inode size is zero
    pub fn read_available_inode_areas(&self) -> Vec<DiskInode> {
        let mut v = Vec::new();
        for i in 0..10 {
            // loop 10 times, show first 10 inode area block
            let (block_id, inode_offset) = self.get_disk_inode_pos(i);
            get_block_cache(block_id as usize, Arc::clone(&self.block_device))
                .lock()
                .read(inode_offset, |disk_inode: &DiskInode| {
                    if disk_inode.size != 0 {
                        // only add not empty inode area
                        v.push(disk_inode.clone());
                    }
                });
        }
        return v;
    }

    /// show data area by blockId
    pub fn read_data_area(&self, data_block_id: usize) -> DataBlock {
        let data_block = get_block_cache(data_block_id, Arc::clone(&self.block_device))
            .lock()
            .read(0, |data_block: &DataBlock| {
                return data_block.clone();
            });
        return data_block;
    }
    /// show indirect block by blockId
    pub fn read_indirect_block(&self, indirect_block_id: usize) -> IndirectBlock {
        let indirect_block = get_block_cache(indirect_block_id, Arc::clone(&self.block_device))
            .lock()
            .read(0, |indirect_block: &IndirectBlock| {
                return indirect_block.clone();
            });
        return indirect_block;
    }
}
