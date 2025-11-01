// src/fs.rs

#![allow(dead_code, unused_variables)]

use crate::block_device::{BlockDevice, BlockId, BLOCK_SIZE};
use crate::cache::{BlockCache, SysLock};
use crate::allocator::{Allocator, AllocatorError};
use crate::btree::BTree;
use crate::checksum;
use crate::sahne_syscalls::{self, SyscallError}; // sahne_syscalls'ı ekledik

use core::mem;
use alloc::sync::Arc;
use alloc::boxed::Box;
use core::fmt::Debug;

// --- 1. Sabitler ve Türler ---

// Dosya sistemini tanımlayan sihirli sayı (Magic Number).
const SADAK_MAGIC: u64 = 0x5ADAKF5; 

// SADAK versiyonu
const SADAK_VERSION: u16 = 1;

// Ana Dosya Sistemi Hata Türü
#[derive(Debug)]
pub enum SadakFsError<D: BlockDevice> {
    Device(D::Error),
    Allocator(AllocatorError<D>),
    ChecksumError,
    InvalidSuperblock,
    Syscall(SyscallError),
    // Diğer hatalar...
}

// Hata dönüşümlerini kolaylaştır
impl<D: BlockDevice> From<D::Error> for SadakFsError<D> {
    fn from(e: D::Error) -> Self {
        SadakFsError::Device(e)
    }
}

impl<D: BlockDevice> From<AllocatorError<D>> for SadakFsError<D> {
    fn from(e: AllocatorError<D>) -> Self {
        SadakFsError::Allocator(e)
    }
}

impl<D: BlockDevice> From<SyscallError> for SadakFsError<D> {
    fn from(e: SyscallError) -> Self {
        SadakFsError::Syscall(e)
    }
}


// --- 2. Superblock Yapısı ---

/// Dosya sisteminin diskteki ilk bloğunda (BlockId 0) yer alan ana metadata.
#[repr(C)]
pub struct Superblock {
    pub magic: u64, // Sihirli sayı: SADAK_MAGIC
    pub version: u16,
    pub total_blocks: BlockId,
    /// Metadata B-Ağacının kök bloğunun ID'si (Dizinler, Dosyalar)
    pub metadata_root_id: BlockId, 
    /// Tahsis haritasının (Allocator) başlangıç bloğunun ID'si
    pub bitmap_start_id: BlockId, 
    /// Son commit zamanı (SYSCALL_GET_SYSTEM_TIME ile alınır)
    pub timestamp: u64,
    /// Superblock'un Checksum'u
    pub checksum: u32,
    
    // Superblock'u 4096 bayta tamamlamak için doldurma (padding)
    padding: [u8; BLOCK_SIZE - (mem::size_of::<u64>() * 3 + mem::size_of::<u16>() + mem::size_of::<u32>() * 2 + mem::size_of::<u64>() * 2 + mem::size_of::<u8>() * 0)], 
}


// --- 2.5. Inode Yapısı (Dosya/Dizin Metadata'sı) ---

/// Diskteki bir dosyayı veya dizini temsil eden metadata yapısı.
#[repr(C)]
pub struct Inode {
    pub file_size: u64, // Dosyanın bayt cinsinden boyutu
    pub block_count: u64, // Dosyanın kullandığı blok sayısı
    pub creation_time: u64,
    pub modification_time: u64,
    pub file_type: u8, // 1=Dosya, 2=Dizin
    // Dosya veri bloklarına işaret eden doğrudan işaretçiler (CoW B-Ağacı kökleri)
    pub data_tree_root: BlockId, 
    pub link_count: u32,
    pub checksum: u32,
    // Doldurma
    padding: [u8; 128], // Toplam 256 bayt varsayalım
}


// --- 3. SADAK Dosya Sistemi Ana Yapısı ---

/// SADAK Dosya Sistemi. Tüm temel bileşenleri bir araya getirir.
pub struct SadakFs<D: BlockDevice> {
    /// Fiziksel disk I/O'sunu yöneten önbellek katmanı.
    cache: Arc<BlockCache<D>>,
    /// Disk üzerindeki boş/dolu blokları yöneten.
    allocator: Allocator<D>,
    /// Dosya ve dizin yapısını tutan B-Ağacı (CoW).
    metadata_tree: BTree<D>,
    /// Dosya sistemi yapısını eş zamanlı koruyan kilit.
    lock: SysLock,
    /// Dosya sistemi yapısının en son hali
    superblock: Superblock,
}

impl<D: BlockDevice> SadakFs<D>
where
    D: Debug + 'static, // BlockDevice'ın hata ayıklama yeteneği ve statik ömrü olsun
{
    // --- Başlatma ve Montaj İşlemleri ---

    /// Mevcut bir diskten SADAK dosya sistemini yükler (Montaj).
    pub fn mount(device: D) -> Result<Self, SadakFsError<D>> {
        let cache = Arc::new(BlockCache::new(Arc::new(device))?);
        
        // 1. Superblock'u oku (Her zaman BlockId 0'da)
        let sb_block = cache.get_block(0)?;
        let sb_ref = unsafe { &mut *sb_block.get() };
        
        // Ham veriyi Superblock yapısına dönüştür (unsafe, tip dönüşümü)
        let sb_ptr = sb_ref.data.as_ptr() as *const Superblock;
        let superblock = unsafe { sb_ptr.read() };
        
        // 2. Superblock Checksum Doğrulaması
        let calculated_crc = checksum::checksum_data(sb_ref.data.as_ref());

        if superblock.magic != SADAK_MAGIC || calculated_crc != superblock.checksum {
            // Checksum veya Magic Number uyuşmazlığı, veri bozulması.
            return Err(SadakFsError::InvalidSuperblock);
        }
        
        // 3. Alt Sistemleri Başlat
        let allocator = Allocator::new(cache.clone(), superblock.bitmap_start_id)?;
        let metadata_tree = BTree::new(cache.clone(), superblock.metadata_root_id)?;
        
        Ok(SadakFs {
            cache,
            allocator,
            metadata_tree,
            lock: SysLock::new()?,
            superblock,
        })
    }
    
    /// Bir dosya sistemini diske biçimlendirir ve ilk Superblock'u yazar.
    pub fn format(device: D) -> Result<Self, SadakFsError<D>> {
        // Kilit oluşturma
        let fs_lock = SysLock::new()?;
        fs_lock.acquire(); // İşlem atomik olmalı
        
        let total_blocks = device.total_blocks();
        let cache = Arc::new(BlockCache::new(Arc::new(device))?);

        // 1. Tahsis Yöneticisini Başlat
        let bitmap_start_id = 1; 
        let allocator = Allocator::new(cache.clone(), bitmap_start_id)?;
        
        // 2. Kök Ağaçları Oluştur (Metadata B-Tree)
        let metadata_root_id = allocator.allocate_block()?; 
        let metadata_tree = BTree::new(cache.clone(), metadata_root_id)?;
        
        // 3. Superblock Oluştur
        let mut new_sb = Superblock {
            magic: SADAK_MAGIC,
            version: SADAK_VERSION,
            total_blocks,
            metadata_root_id,
            bitmap_start_id,
            timestamp: 0, // İlk başta 0
            checksum: 0,
            padding: [0u8; BLOCK_SIZE - (mem::size_of::<u64>() * 3 + mem::size_of::<u16>() + mem::size_of::<u32>() * 2 + mem::size_of::<u64>() * 2 + mem::size_of::<u8>() * 0)],
        };
        
        // 4. Superblock'u Disk Üzerinde Hazırla
        let sb_block_arc = cache.get_block(0)?; // Blok 0'ı al
        let sb_block_mut = unsafe { &mut *sb_block_arc.get() };
        
        // Superblock'u bloğun ham verisine kopyala
        let sb_data_slice: &mut [u8] = sb_block_mut.data.as_mut();
        unsafe {
            let sb_ptr = sb_data_slice.as_mut_ptr() as *mut Superblock;
            *sb_ptr = new_sb;
        }

        // 5. Checksum Hesapla ve Kaydet
        new_sb.checksum = checksum::checksum_data(sb_data_slice);
        // Checksum'ı tekrar bloğa yaz
        unsafe {
            let sb_ptr = sb_data_slice.as_mut_ptr() as *mut Superblock;
            *sb_ptr = new_sb;
        }

        // 6. Superblock'u "kirli" olarak işaretle ve diske yazılmasını zorla
        sb_block_mut.is_dirty = true;
        cache.device.flush()?; // Değişiklikleri kalıcı yap

        fs_lock.release(); // Kilidi bırak.

        Ok(SadakFs {
            cache,
            allocator,
            metadata_tree,
            lock: fs_lock,
            superblock: new_sb,
        })
    }
    
    // --- Dosya Sistemi İşlemleri ---

    /// Basit bir dosyayı (inode) B-Ağacında oluşturur.
    pub fn create_file(&self, file_size: u64) -> Result<Inode, SadakFsError<D>> {
        self.lock.acquire(); // Atomik işlem için kilidi al

        // 1. Yeni bir Inode için blok tahsis et.
        let inode_block_id = self.allocator.allocate_block().map_err(SadakFsError::Allocator)?;

        // 2. Yeni bir Veri B-Ağacı Kökü tahsis et (Dosya verileri için)
        let data_root_id = self.allocator.allocate_block().map_err(SadakFsError::Allocator)?;
        
        // 3. Inode yapısını oluştur
        let mut new_inode = Inode {
            file_size,
            block_count: 0, 
            creation_time: self.get_system_time()?, 
            modification_time: self.get_system_time()?,
            file_type: 1, // Dosya
            data_tree_root: data_root_id, 
            link_count: 1,
            checksum: 0, 
            padding: [0; 128],
        };
        
        // 4. Inode'u önbelleğe al ve diske yazılmaya hazırla (CoW bloğu)
        let inode_arc = self.cache.get_block(inode_block_id)?;
        let inode_block_mut = unsafe { &mut *inode_arc.get() };
        
        let inode_data_slice: &mut [u8] = inode_block_mut.data.as_mut();
        
        // Kopyalama için ham pointer kullan
        unsafe {
            let inode_ptr = inode_data_slice.as_mut_ptr() as *mut Inode;
            *inode_ptr = new_inode;
        }

        // 5. Checksum Hesapla ve Kaydet
        new_inode.checksum = checksum::checksum_data(inode_data_slice);
        // Checksum'ı tekrar bloğa yaz
        unsafe {
            let inode_ptr = inode_data_slice.as_mut_ptr() as *mut Inode;
            *inode_ptr = new_inode;
        }
        
        // Bloğu kirli olarak işaretle (CoW işlemi için önemli)
        inode_block_mut.is_dirty = true;
        
        self.lock.release(); // Kilidi bırak.
        
        Ok(new_inode)
    }

    /// Superblock'u güncelleyip tüm kirli (dirty) blokları diske yazar (Atomik Commit).
    pub fn sync(&self) -> Result<(), SadakFsError<D>> {
        self.lock.acquire();
        
        // Gerçek implementasyonda: Tüm kirli CoW blokları diske yazılır.
        // Bu işlem, yeni kök işaretçilerini belirler.
        // Superblock'u yeni kök işaretçileri ve zaman damgasıyla günceller.
        
        // 1. Superblock'u Block 0'a yazar (En son işlem)
        self.cache.device.flush()?;

        self.lock.release();
        Ok(())
    }
    
    // --- Yardımcı Fonksiyonlar ---

    /// Sahne64 çekirdeğinden sistem zamanını alır.
    fn get_system_time(&self) -> Result<u64, SadakFsError<D>> {
        let result = unsafe { 
            sahne_syscalls::raw_syscall(sahne_syscalls::SYSCALL_GET_SYSTEM_TIME, 0, 0, 0, 0, 0, 0) 
        };
        if result < 0 {
            Err(SadakFsError::Syscall(SyscallError::from_raw(result)))
        } else {
            Ok(result as u64)
        }
    }
}