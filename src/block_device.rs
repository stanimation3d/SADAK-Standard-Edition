// src/block_device.rs

// sahne_syscalls modülünü dahil et (Bir önceki adımda oluşturuldu)
use crate::sahne_syscalls::{
    self, ResourceHandle, SyscallError,
    SYSCALL_RESOURCE_READ, SYSCALL_RESOURCE_WRITE,
    raw_syscall
};
use core::fmt::Debug;

// --- 1. Sabit Tanımlamaları ---
// Sektör/Blok boyutu (genellikle 4096 bayt). 
pub const BLOCK_SIZE: usize = 4096;

// Blok Numarası türü (LBA'yı temsil eder)
pub type BlockId = u64;


// --- 2. Hata Trait'i ---
// Tüm blok aygıt hatalarının uygulayacağı genel bir hata trait'i.
pub trait BlockDeviceError: Debug {}
impl BlockDeviceError for SyscallError {}


// --- 3. Blok Aygıt Trait'i ---
/// SADAK dosya sisteminin temel disk I/O işlemlerini soyutlayan trait.
/// Bu trait, her türlü fiziksel sürücü (HDD, SSD, SD kart) için uygulanmalıdır.
pub trait BlockDevice {
    /// Bu aygıtın I/O işlemlerinden dönebilecek hata türü.
    type Error: BlockDeviceError;

    /// Fiziksel aygıttan belirli bir blok okur.
    ///
    /// # Parametreler
    /// * `id`: Okunacak mantıksal blok numarası (LBA).
    /// * `buffer`: Okunan verilerin yerleştirileceği, `BLOCK_SIZE` boyutunda arabellek.
    fn read_block(&self, id: BlockId, buffer: &mut [u8]) -> Result<(), Self::Error>;

    /// Fiziksel aygıta belirli bir blok yazar.
    ///
    /// # Parametreler
    /// * `id`: Yazılacak mantıksal blok numarası (LBA).
    /// * `data`: Yazılacak `BLOCK_SIZE` boyutunda veri.
    fn write_block(&self, id: BlockId, data: &[u8]) -> Result<(), Self::Error>;

    /// Aygıtın toplam blok sayısını döndürür.
    fn total_blocks(&self) -> BlockId;

    /// (Opsiyonel) Verilerin kalıcı olarak diske yazılmasını zorlar (fsync).
    fn flush(&self) -> Result<(), Self::Error> {
        Ok(()) // Varsayılan olarak hiçbir şey yapmaz
    }
}


// --- 4. Sahne64 Tabanlı Blok Aygıt Uygulaması ---

/// Sahne64 çekirdeğinin Resource Handle'ını kullanarak BlockDevice trait'ini uygular.
/// Bu, bir SATA/NVMe sürücüsü gibi davranan bir kaynağı temsil eder.
pub struct Sahne64Device {
    /// Çekirdekten alınan fiziksel sürücüyü temsil eden handle.
    handle: ResourceHandle,
    /// Sürücünün toplam kapasitesi (blok cinsinden).
    capacity_blocks: BlockId,
}

impl Sahne64Device {
    /// Yeni bir Sahne64Device örneği oluşturur ve kaynağı edinir.
    pub fn new(resource_path: &str, capacity: BlockId) -> Result<Self, SyscallError> {
        let handle = sahne_syscalls::resource_acquire(
            resource_path.as_ptr(), 
            resource_path.len()
        )?;

        // Gerçekte burada SYSCALL_RESOURCE_STAT/CONTROL ile kapasite sorgulanmalıdır.
        Ok(Sahne64Device {
            handle,
            capacity_blocks: capacity,
        })
    }
}

impl BlockDevice for Sahne64Device {
    type Error = SyscallError;

    fn read_block(&self, id: BlockId, buffer: &mut [u8]) -> Result<(), Self::Error> {
        if buffer.len() != BLOCK_SIZE {
            return Err(SyscallError::EINVAL); // Geçersiz arabellek boyutu
        }

        // 1. Kaynakta konumlan (SYSCALL_RESOURCE_SEEK'i kullanır)
        let offset = id * BLOCK_SIZE as u64;
        let seek_result = unsafe {
            raw_syscall(
                sahne_syscalls::SYSCALL_RESOURCE_SEEK,
                self.handle,
                offset, // Konum (offset)
                0, // seek_type: 0 = Absolute (varsayım)
                0, 0, 0,
            )
        };
        if seek_result < 0 {
            return Err(SyscallError::from_raw(seek_result));
        }

        // 2. Veriyi oku (SYSCALL_RESOURCE_READ'i kullanır)
        let read_len = unsafe {
            raw_syscall(
                SYSCALL_RESOURCE_READ,
                self.handle,
                buffer.as_mut_ptr() as u64,
                BLOCK_SIZE as u64,
                0, 0, 0,
            )
        };
        
        // Başarılı okuma, okunan bayt sayısını döndürmelidir.
        if read_len as usize != BLOCK_SIZE {
            if read_len < 0 {
                return Err(SyscallError::from_raw(read_len));
            } else {
                return Err(SyscallError::EIO); // Tam blok okunamadı
            }
        }

        Ok(())
    }

    fn write_block(&self, id: BlockId, data: &[u8]) -> Result<(), Self::Error> {
        if data.len() != BLOCK_SIZE {
            return Err(SyscallError::EINVAL);
        }

        // 1. Kaynakta konumlan (SYSCALL_RESOURCE_SEEK'i kullanır)
        let offset = id * BLOCK_SIZE as u64;
        let seek_result = unsafe {
            raw_syscall(
                sahne_syscalls::SYSCALL_RESOURCE_SEEK,
                self.handle,
                offset, 
                0, 0, 0, 0,
            )
        };
        if seek_result < 0 {
            return Err(SyscallError::from_raw(seek_result));
        }

        // 2. Veriyi yaz (SYSCALL_RESOURCE_WRITE'ı kullanır)
        let write_len = unsafe {
            raw_syscall(
                SYSCALL_RESOURCE_WRITE,
                self.handle,
                data.as_ptr() as u64,
                BLOCK_SIZE as u64,
                0, 0, 0,
            )
        };
        
        if write_len as usize != BLOCK_SIZE {
             if write_len < 0 {
                return Err(SyscallError::from_raw(write_len));
            } else {
                return Err(SyscallError::EIO); // Tam blok yazılamadı
            }
        }

        Ok(())
    }

    fn total_blocks(&self) -> BlockId {
        self.capacity_blocks
    }
}