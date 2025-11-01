// src/cache.rs

#![allow(dead_code, unused_variables)]

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::UnsafeCell;

use crate::block_device::{BlockDevice, BlockId, BLOCK_SIZE};
use crate::sahne_syscalls::{
    self, ResourceHandle, SyscallError,
    SYSCALL_LOCK_CREATE, SYSCALL_LOCK_ACQUIRE, SYSCALL_LOCK_RELEASE,
    SYSCALL_MEMORY_ALLOCATE, SYSCALL_MEMORY_RELEASE,
    raw_syscall
};


// --- 1. Bellek ve Kilit Sarmalayıcıları (Sahne64'e Özel) ---

// Sahne64'ün kilit handle'ı için bir tür alias'ı
pub type LockHandle = u64;

/// Sahne64 çekirdeğinin kilitlerini sarmalayan temel bir yapı.
/// Bu, önbelleğe eş zamanlı erişimi yönetmek için kullanılacaktır.
pub struct SysLock {
    handle: LockHandle,
}

impl SysLock {
    /// Yeni bir çekirdek kilidi oluşturur.
    pub fn new() -> Result<Self, SyscallError> {
        let result = unsafe { raw_syscall(SYSCALL_LOCK_CREATE, 0, 0, 0, 0, 0, 0) };
        if result < 0 {
            Err(SyscallError::from_raw(result))
        } else {
            Ok(SysLock { handle: result as LockHandle })
        }
    }

    /// Kilidi alır (Bloklayabilir).
    pub fn acquire(&self) {
        unsafe { raw_syscall(SYSCALL_LOCK_ACQUIRE, self.handle, 0, 0, 0, 0, 0) };
    }

    /// Kilidi serbest bırakır.
    pub fn release(&self) {
        unsafe { raw_syscall(SYSCALL_LOCK_RELEASE, self.handle, 0, 0, 0, 0, 0) };
    }
}

// NOT: `Drop` trait'i, kilit handle'ını serbest bırakmak için uygulanmalıdır.


// --- 2. Önbellek Yapıları ---

/// Diskten okunan/diske yazılacak tek bir bloğu temsil eder.
/// `Arc` ve `UnsafeCell`, CoW için gereken Paylaşımlı Mutluluk (Shared Mutability) sağlar.
pub struct CacheBlock {
    /// Bloğun ham bayt verisi (BLOCK_SIZE boyutunda).
    data: Box<[u8; BLOCK_SIZE]>, 
    /// Diskteki mantıksal blok numarası (eğer tahsis edilmişse).
    block_id: BlockId,
    /// Blok değiştirildi mi? (Diske yazılması gerekiyor mu?)
    is_dirty: bool,
}

impl CacheBlock {
    /// Yeni, boş (sıfırlanmış) bir önbellek bloğu oluşturur.
    /// Sahne64'ün bellek tahsis çağrısını kullanır.
    pub fn new_empty(id: BlockId) -> Result<Arc<UnsafeCell<Self>>, SyscallError> {
        // Blok için dinamik olarak bellek tahsis et (Sahne64 çağrısı)
        let total_size = BLOCK_SIZE;
        let mem_ptr = unsafe {
            raw_syscall(SYSCALL_MEMORY_ALLOCATE, total_size as u64, 0, 0, 0, 0, 0)
        };

        if mem_ptr == 0 {
            return Err(SyscallError::ENOMEM);
        }

        // Tahsis edilen ham bellek alanını Box<[u8; BLOCK_SIZE]> 'a dönüştür.
        let data_box = unsafe {
             // Tahsis edilen alanı *mut [u8; BLOCK_SIZE] olarak varsay
            let slice_ptr: *mut [u8; BLOCK_SIZE] = mem_ptr as *mut [u8; BLOCK_SIZE];
            
            // Veriyi sıfırla (güvenlik için)
            core::ptr::write_bytes(slice_ptr as *mut u8, 0, BLOCK_SIZE);

            // Box'a dönüştür (ownership'i Rust'a ver)
            Box::from_raw(slice_ptr)
        };


        Ok(Arc::new(UnsafeCell::new(CacheBlock {
            data: data_box,
            block_id: id,
            is_dirty: true, // Yeni blok tahsis edildiği için kirli sayılır
        })))
    }
    
    // NOT: `Drop` trait'i, `Box<...>` serbest bırakıldığında `SYSCALL_MEMORY_RELEASE` 
    // çağırmak üzere dikkatlice uygulanmalıdır.
}


/// SADAK'ın blok I/O'sunu yöneten ana önbellek yapısı.
/// Bu, CoW için kritik olan "blokları diskte değil, bellekte tutma" görevini üstlenir.
pub struct BlockCache<D: BlockDevice> {
    device: Arc<D>,
    // TODO: Önbellek haritası (örneğin, BlockId -> Arc<UnsafeCell<CacheBlock>>) 
    // Lock ile korunan bir HashTable veya BTreeMap olmalıdır.
    // cache_map: Locked<HashMap<BlockId, Arc<UnsafeCell<CacheBlock>>>>,
    lock: SysLock,
}

impl<D: BlockDevice> BlockCache<D> {
    pub fn new(device: Arc<D>) -> Result<Self, SyscallError> {
        Ok(BlockCache {
            device,
            lock: SysLock::new()?, // Önbellek erişimi için kilidi oluştur
        })
    }
    
    /// Belirli bir blok numarasını önbellekten alır veya diskten okur.
    pub fn get_block(&self, id: BlockId) -> Result<Arc<UnsafeCell<CacheBlock>>, D::Error> {
        // Basitleştirilmiş implementasyon: Her zaman diskten oku.
        // Gerçek implementasyonda önce 'lock' alınır ve 'cache_map' kontrol edilir.
        
        // 1. Bellek Tahsis Et (SYSCALL_MEMORY_ALLOCATE kullanılarak)
        let block_arc = match CacheBlock::new_empty(id) {
            Ok(b) => b,
            Err(e) => {
                // Sahne64 sistem çağrısı hatasını yay
                // D::Error'a dönüşüm için `from` kullanıyoruz, D'nin bu trait'i uygulaması gerekir.
                return Err(D::Error::from(e)); 
            }
        };
        
        // 2. Diske I/O Yap (BlockDevice kullanılarak)
        // Güvenli olmayan (unsafe) alana erişim. CoW ve kilitleme mekaniği budur.
        let block_mut = unsafe { &mut *block_arc.get() };
        
        // Cihazdan veriyi okur ve bloğun ham verisine yazar.
        // SYSCALL_RESOURCE_READ/SEEK, BlockDevice içinde sarmalandı.
        self.device.read_block(id, block_mut.data.as_mut())?;
        
        // Okunan blok temizdir (kirli: false)
        block_mut.is_dirty = false;
        
        Ok(block_arc)
    }
    
    // TODO: `release_block` (kirli ise diske yazar) ve `new_allocated_block` (yeni blok tahsis eder) 
    // fonksiyonları bu yapıya eklenecektir.
}