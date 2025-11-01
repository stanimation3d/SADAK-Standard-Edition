// src/allocator.rs

#![allow(dead_code, unused_variables)]

use crate::block_device::{BlockDevice, BlockId, BLOCK_SIZE};
use crate::cache::{BlockCache, SysLock};
use crate::sahne_syscalls::SyscallError;
use core::fmt::Debug;
use alloc::sync::Arc;
use alloc::vec::Vec;


// --- 1. Sabitler ve Türler ---

// Bir tahsis haritası bloğunun (Bitmap Block) temsil edebileceği blok sayısı.
// BLOCK_SIZE (4096 bayt) * 8 bit/bayt = 32768 bloğu temsil edebilir.
const BLOCKS_PER_BITMAP_BLOCK: BlockId = (BLOCK_SIZE * 8) as BlockId;

// Tahsis Hatası Türü
#[derive(Debug)]
pub enum AllocatorError<D: BlockDevice> {
    DeviceError(D::Error),
    OutOfSpace,
    Syscall(SyscallError),
}

impl<D: BlockDevice> From<D::Error> for AllocatorError<D> {
    fn from(e: D::Error) -> Self {
        AllocatorError::DeviceError(e)
    }
}

impl<D: BlockDevice> From<SyscallError> for AllocatorError<D> {
    fn from(e: SyscallError) -> Self {
        AllocatorError::Syscall(e)
    }
}


// --- 2. Tahsis Yöneticisi Yapısı ---

/// Disk üzerindeki blokların tahsis durumunu yönetir.
/// SADAK'ın boş blok bulmasını sağlar.
pub struct Allocator<D: BlockDevice> {
    /// Tüm I/O'yu yöneten ve blokları bellekte tutan önbellek.
    cache: Arc<BlockCache<D>>,
    /// Disk üzerindeki toplam blok sayısı.
    total_blocks: BlockId,
    /// Boş blokları yönetmek için kullanılan kilit.
    lock: SysLock,
    
    // Tahsis haritasının (bitmap'in) diskteki başlangıç BlockId'si
    // Bu, dosya sisteminin Superblock'unda tutulur.
    bitmap_start_id: BlockId, 
    /// Tahsis haritasının kaç blok kapladığı.
    bitmap_block_count: BlockId,
}

impl<D: BlockDevice> Allocator<D> {
    
    /// Yeni bir Tahsis Yöneticisi örneği oluşturur.
    pub fn new(cache: Arc<BlockCache<D>>, bitmap_start_id: BlockId) -> Result<Self, AllocatorError<D>> {
        let device = cache.device.clone();
        let total_blocks = device.total_blocks();
        
        // Tahsis haritasının ihtiyaç duyduğu blok sayısını hesapla:
        // Toplam blok / (Blok başına bit)
        let bitmap_block_count = (total_blocks + BLOCKS_PER_BITMAP_BLOCK - 1) / BLOCKS_PER_BITMAP_BLOCK;

        Ok(Allocator {
            cache,
            total_blocks,
            lock: SysLock::new().map_err(AllocatorError::Syscall)?,
            bitmap_start_id,
            bitmap_block_count,
        })
    }

    /// Yeni, boş bir disk bloğu tahsis eder (CoW için kritik).
    ///
    /// # Döndürür
    /// Tahsis edilen bloğun ID'si.
    pub fn allocate_block(&self) -> Result<BlockId, AllocatorError<D>> {
        self.lock.acquire(); // Eş zamanlı tahsisleri engellemek için kilidi al.

        // Basitleştirilmiş Arama Mantığı (Gerçek FS'de bu karmaşıktır)
        // 1. Bitmap bloklarını tek tek tara.
        for i in 0..self.bitmap_block_count {
            let bitmap_block_id = self.bitmap_start_id + i;
            
            // Önbellekten bitmap bloğunu oku (Cache/I/O)
            let bitmap_arc = self.cache.get_block(bitmap_block_id)?;
            let bitmap_block = unsafe { &mut *bitmap_arc.get() };

            // Bitmap içinde ilk boş biti (bloğu) ara.
            if let Some((byte_index, bit_index)) = self.find_free_bit(&mut bitmap_block.data.as_mut()) {
                
                // 2. Bloğu tahsis et (Bit'i 1 olarak işaretle)
                let byte_mut = &mut bitmap_block.data.as_mut()[byte_index];
                *byte_mut |= 1 << bit_index;
                
                // Bloğu "kirli" (dirty) olarak işaretle ki önbellek dışına atılırken diske yazılsın.
                bitmap_block.is_dirty = true;
                
                // 3. Tahsis edilen bloğun global ID'sini hesapla
                let block_offset_in_bitmap = (i * BLOCKS_PER_BITMAP_BLOCK) + 
                                             (byte_index as BlockId * 8) + 
                                             (bit_index as BlockId);
                
                // Tahsis edilmiş blok ID'si
                self.lock.release(); // Kilidi bırak.
                return Ok(block_offset_in_bitmap);
            }
        }

        self.lock.release(); // Kilidi bırak.
        Err(AllocatorError::OutOfSpace) // Boş blok bulunamadı
    }
    
    // --- Yardımcı Fonksiyonlar ---
    
    /// Verilen bitmap diliminde ilk boş (0) biti bulur.
    fn find_free_bit(&self, bitmap: &mut [u8]) -> Option<(usize, u8)> {
        for (byte_index, &byte) in bitmap.iter().enumerate() {
            // Eğer byte tamamen dolu DEĞİLSE (~0xFF), boş bit vardır.
            if byte != 0xFF {
                // Byte içindeki bitleri tara
                for bit_index in 0..8 {
                    if (byte & (1 << bit_index)) == 0 {
                        // Boş bit bulundu (değeri 0)
                        return Some((byte_index, bit_index as u8));
                    }
                }
            }
        }
        None // Tamamen dolu
    }

    /// Tahsis edilmiş bir bloğu serbest bırakır (Bit'i 0 olarak işaretler).
    pub fn free_block(&self, id: BlockId) -> Result<(), AllocatorError<D>> {
        // Bu fonksiyon, id'yi kullanarak ilgili bitmap bloğunu bulur,
        // önbelleğe alır, biti 0 yapar ve bloğu kirli olarak işaretler.
        // Implementasyon için allocate_block'un tersi mantık gereklidir.
        // ...
        Ok(())
    }
}
