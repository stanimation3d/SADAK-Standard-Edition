// src/main.rs

// Standart kütüphaneyi kullanma
#![no_std]
// Normal main fonksiyonunu kullanma (özel giriş noktası kullanılacak)
#![no_main] 

extern crate alloc; 

use sadak_fs::{
    fs::SadakFs,
    block_device::{BlockDevice, BlockId, BLOCK_SIZE},
    sahne_syscalls::SyscallError,
    raid::Raid1Device
};
use core::panic::PanicInfo;
use alloc::sync::Arc;
use alloc::vec;

// --- Gerekli Globallar (no-std için) ---

// Sahne64 çekirdeğine bir mesaj gönderecek hayali bir makro/fonksiyon
extern "C" {
    fn sahne64_print(s: *const u8, len: usize);
    fn sahne64_exit(code: i32) -> !;
}

// Global bellek tahsis ediciyi tanımlamamız gerekiyor.
// SADAK'ta bu işlem SYSCALL_MEMORY_ALLOCATE ile yapıldığı için, 
// burada 'sahne64'e dayanan hayali bir tahsis edici tanımlamamız gerekir.
// Basitlik için, Rust'ın alloc'u kullanmasına izin verecek hayali bir implementasyon kullanalım.
#[global_allocator]
static ALLOCATOR: dummy_allocator::DummyAllocator = dummy_allocator::DummyAllocator;

mod dummy_allocator {
    // Rust'ın Allocator trait'ini kullanır
    use core::alloc::{GlobalAlloc, Layout};
    
    pub struct DummyAllocator;
    
    // Gerçek bir uygulamada, bu SYSCALL_MEMORY_ALLOCATE/RELEASE çağrılarını yapmalıdır.
    unsafe impl GlobalAlloc for DummyAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SYSCALL_MEMORY_ALLOCATE çağrısının ham implementasyonu buraya gelmeli.
            // Şimdilik null döndürerek tahsis hatasını simüle edelim.
            core::ptr::null_mut()
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SYSCALL_MEMORY_RELEASE çağrısının ham implementasyonu buraya gelmeli.
        }
    }
}


// --- 1. Simüle Edilmiş Fiziksel Cihaz (Test amaçlı) ---

// Gerçek Sahne64Device'ın kullanılması gereklidir, ancak derleme için
// BlockDevice trait'ini uygulayan bir yapıya ihtiyacımız var.
pub struct MockDevice;

// NOT: Gerçek SADAK uygulamasında bunun yerine Sahne64Device kullanılmalıdır.
impl BlockDevice for MockDevice {
    // Hata tipi olarak doğrudan SyscallError kullanılsın
    type Error = SyscallError;

    fn read_block(&self, id: BlockId, buffer: &mut [u8]) -> Result<(), Self::Error> {
        // ... Sahne64Device çağrıları burada simüle edilmeli.
        Ok(()) 
    }
    fn write_block(&self, id: BlockId, data: &[u8]) -> Result<(), Self::Error> {
        // ...
        Ok(())
    }
    fn total_blocks(&self) -> BlockId {
        // 1GB (1000 * 1024 * 1024 / 4096) blok simülasyonu
        262144 
    }
}


// --- 2. Giriş Noktası (Sahne64 Çekirdeği tarafından çağrılan) ---

// Çekirdeğinizin çağıracağı özel giriş noktası
#[no_mangle] 
pub extern "C" fn sadak_entry_point() -> ! {
    let msg = b"SADAK Dosya Sistemi Başlatılıyor...\n";
    unsafe { sahne64_print(msg.as_ptr(), msg.len()); }

    // Mock/Sahne64 cihazını oluştur
    let device1 = Arc::new(MockDevice);
    let device2 = Arc::new(MockDevice);
    
    // RAID-1 Cihazını başlat
    match Raid1Device::new(vec![device1, device2]) {
        Ok(raid_device) => {
            // RAID cihazı üzerinde SADAK'ı formatla
            let sadak_result = SadakFs::format(raid_device);
            
            match sadak_result {
                Ok(fs) => {
                    let success_msg = b"SADAK FS başarıyla formatlandı ve monte edildi.\n";
                    unsafe { sahne64_print(success_msg.as_ptr(), success_msg.len()); }
                    // Başarılıysa çekirdekten çıkış yap
                    unsafe { sahne64_exit(0); }
                }
                Err(e) => {
                    let err_msg = b"SADAK FS BAŞARISIZ: Formatlama veya montaj hatası.\n";
                    unsafe { sahne64_print(err_msg.as_ptr(), err_msg.len()); }
                    // Hata durumunda hata koduyla çıkış yap
                    unsafe { sahne64_exit(1); }
                }
            }
        }
        Err(_) => {
            let err_msg = b"RAID Başlatma Hatası: Yeterli cihaz yok veya boyut uyumsuzluğu.\n";
            unsafe { sahne64_print(err_msg.as_ptr(), err_msg.len()); }
            unsafe { sahne64_exit(1); }
        }
    }
}


// --- 3. Panic İşleyicisi (no-std için zorunlu) ---

/// Kritik hata durumunda çağrılır. Normalde sonsuz döngüye girer veya çekirdeğe hata koduyla döner.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let panic_msg = b"SADAK PANİK ETTİ!\n";
    unsafe { sahne64_print(panic_msg.as_ptr(), panic_msg.len()); }
    
    // Sonsuz döngü (kapanma çekirdek tarafından yönetilmelidir)
    loop {} 
}