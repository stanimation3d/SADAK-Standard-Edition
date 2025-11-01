// src/btree.rs

#![allow(dead_code, unused_variables)]

use crate::block_device::BlockId;
use crate::cache::{CacheBlock, SysLock, BlockCache};
use crate::checksum;
use crate::sahne_syscalls::SyscallError;
use core::mem;
use core::cell::UnsafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;


// --- 1. Sabitler ve Türler ---

// B-Ağacı'nın her düğümünde tutulabilecek maksimum öğe sayısı.
// BLOCK_SIZE'a göre ayarlanmalıdır, şimdilik sabit bir sayı kullanalım.
pub const BTREE_NODE_ORDER: usize = 32;

// Düğümün disk üzerindeki boyutu (byte)
const BTREE_NODE_SIZE: usize = BLOCK_SIZE; 


// --- 2. Düğüm Başlığı Yapısı (Metadata) ---

/// Her B-Ağacı düğümünün başında yer alan metadata bilgisi.
/// Bu yapı, düğümün içeriğini tanımlar ve Checksum içerir.
#[repr(C)] // C uyumlu bellek düzenini zorlar
pub struct BTreeNodeHeader {
    /// Bu düğümün tipi (İç düğüm, Yaprak düğüm, vb.)
    pub node_type: u8, 
    /// Kullanımdaki öğe sayısı (key/değer çiftleri veya çocuk işaretçileri)
    pub num_entries: u16, 
    /// Ağacın köküne olan uzaklık (seviye)
    pub level: u8, 
    /// Düğümün BlockId'si (Self-referans için)
    pub block_id: BlockId, 
    /// Metadata bütünlüğü için CRC32C kontrol toplamı
    pub checksum: u32, 
    /// Doldurma baytları (padding)
    padding: [u8; 8], 
    // Toplam 8 + 4 + 2 + 1 + 1 = 16 byte
}


// --- 3. Düğüm Yapısı (Payload) ---

/// B-Ağacı düğümünün disk üzerindeki temsili. 
/// Bu, CacheBlock içindeki ham veriye (data: [u8; BLOCK_SIZE]) karşılık gelir.
#[repr(C)]
pub struct BTreeNode {
    pub header: BTreeNodeHeader,
    // Veri alanı (key'ler, değerler ve çocuk blok işaretçileri)
    // CoW dosya sistemlerinde, bu alan dinamik boyutta (key/değer çiftleri) olacaktır.
    // Kolaylık için, burada ham bayt dilimini temsil eden bir tür kullanalım.
    pub data_area: [u8; BTREE_NODE_SIZE - mem::size_of::<BTreeNodeHeader>()],
}


// --- 4. B-Ağacı Yönetim Yapısı (CoW İçin) ---

/// B-Ağacını yöneten ve CoW işlemlerini yürüten ana yapı.
pub struct BTree<D: BlockDevice> {
    /// Tüm I/O'yu yöneten ve blokları bellekte tutan önbellek.
    cache: Arc<BlockCache<D>>,
    /// Ağacın kök düğümünün diskteki ID'si. (Bu, CoW işleminde sıkça değişir)
    root_id: BlockId,
    // Düğüm işlemlerini eş zamanlı yapmak için kilit
    lock: SysLock, 
}

impl<D: BlockDevice> BTree<D> {
    
    // --- Başlatma ve Checksum İşlemleri ---

    /// B-Ağacını diskten yükler veya yeni bir ağaç oluşturur.
    pub fn new(cache: Arc<BlockCache<D>>, root_id: BlockId) -> Result<Self, D::Error> {
        Ok(BTree {
            cache,
            root_id,
            lock: SysLock::new().map_err(|e| D::Error::from(e))?, // Hata dönüşümünü kullan
        })
    }

    /// Bir düğüm bloğunun metadata Checksum'unu doğrular.
    ///
    /// # Parametreler
    /// * `node_block`: Önbellekten alınmış ham düğüm bloğu.
    pub fn verify_checksum(&self, node_block: &CacheBlock) -> bool {
        // Düğüm başlığını ve içeriğini almak için ham veriyi kullan.
        // Güvenli olmayan (unsafe) blokları okuma işlemi.
        let node_ptr = node_block.data.as_ptr() as *const BTreeNode;
        
        // Ham baytlar üzerinde Checksum hesaplamak için `data` dilimini kullan.
        let calculated_crc = checksum::checksum_data(node_block.data.as_ref());
        
        let stored_crc = unsafe { (*node_ptr).header.checksum };

        // CRC'nin eşleşip eşleşmediğini kontrol et.
        calculated_crc == stored_crc
    }

    // --- Basit Düğüm Okuma İşlemi ---

    /// Bir B-Ağacı düğümünü diskten okur, önbelleğe alır ve Checksum'u doğrular.
    pub fn get_node(&self, id: BlockId) -> Result<Arc<UnsafeCell<CacheBlock>>, D::Error> {
        let block_arc = self.cache.get_block(id)?;
        
        // Checksum doğrulaması
        let block_ref = unsafe { &*block_arc.get() };

        if !self.verify_checksum(block_ref) {
            // Checksum hatası durumunda kritik hata döndür.
            return Err(D::Error::from(SyscallError::EIO));
        }

        Ok(block_arc)
    }

    // TODO: CoW için ana işlevler: 
    // - `copy_on_write_node(id)`: Bloğu kopyalar ve yeni ID atar.
    // - `insert_entry(...)`: Ağaca yeni giriş ekler.
    // - `split_node(...)`: Düğümü ikiye böler.
}