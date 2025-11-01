// src/raid.rs

#![allow(dead_code, unused_variables)]

use crate::block_device::{BlockDevice, BlockId, BLOCK_SIZE, BlockDeviceError};
use crate::sahne_syscalls::SyscallError;
use core::fmt::{self, Debug};
use alloc::vec::Vec;
use alloc::sync::Arc;


// --- 1. RAID Hata Türü ---

/// RAID işlemleri sırasında ortaya çıkabilecek hatalar.
#[derive(Debug)]
pub enum RaidError<D: BlockDevice> {
    /// En az bir aygıtta I/O hatası oluştu.
    IoError(Vec<D::Error>),
    /// Gerekli minimum aygıt sayısı sağlanmadı (RAID-1 için en az 2).
    NotEnoughDevices,
    /// Aygıtların boyutları (blok sayısı) birbirini tutmuyor.
    SizeMismatch,
    /// Dahili kilitlenme veya sistem çağrısı hatası.
    Syscall(SyscallError),
}

impl<D: BlockDevice> BlockDeviceError for RaidError<D> {}

impl<D: BlockDevice> From<SyscallError> for RaidError<D> {
    fn from(e: SyscallError) -> Self {
        RaidError::Syscall(e)
    }
}


// --- 2. RAID-1 Yapısı ---

/// İki veya daha fazla fiziksel diski tek bir mantıksal disk gibi yöneten
/// RAID-1 (Mirroring/Yansıtma) implementasyonu.
/// SADAK, bu yapıyı temel BlockDevice olarak kullanacaktır.
pub struct Raid1Device<D: BlockDevice> {
    /// Verinin kopyalanacağı fiziksel disklerin listesi.
    devices: Vec<Arc<D>>,
    /// En küçük aygıtın toplam blok sayısı (Tüm diskler bu boyutta görünür).
    total_blocks: BlockId,
}

impl<D: BlockDevice> Raid1Device<D> {
    /// Yeni bir RAID-1 dizisi oluşturur.
    pub fn new(devices: Vec<Arc<D>>) -> Result<Self, RaidError<D>> {
        if devices.len() < 2 {
            return Err(RaidError::NotEnoughDevices);
        }
        
        let min_blocks = devices.iter()
            .map(|d| d.total_blocks())
            .min()
            // Zaten en az iki cihaz olduğu için unwrap güvenlidir.
            .unwrap(); 

        // Boyut uyumsuzluğunu kontrol et (İsteğe bağlı, ancak önemlidir)
        let mismatch = devices.iter()
            .any(|d| d.total_blocks() != min_blocks);
        
        if mismatch {
            // RAID, en küçük aygıtın boyutunu kullanmak yerine, boyut uyumsuzluğunu bir hata olarak ele alabilir.
            // Biz şimdilik hata döndürelim.
            return Err(RaidError::SizeMismatch);
        }

        Ok(Raid1Device {
            devices,
            total_blocks: min_blocks,
        })
    }
}


// --- 3. BlockDevice Trait'inin Uygulanması ---

impl<D: BlockDevice + Sync + Send + 'static> BlockDevice for Raid1Device<D> {
    
    // RAID-1 kendi hata türünü kullanır.
    type Error = RaidError<D>; 

    /// RAID-1 Okuma: Herhangi bir kopyadan başarılı okuma yeterlidir.
    fn read_block(&self, id: BlockId, buffer: &mut [u8]) -> Result<(), Self::Error> {
        let mut errors = Vec::new();

        // Cihazları sırayla oku. İlk başarılı okuma yeterlidir.
        for device in self.devices.iter() {
            match device.read_block(id, buffer) {
                Ok(_) => return Ok(()), // Başarılı, hemen dön
                Err(e) => {
                    // Hatayı kaydet ve bir sonraki diski dene.
                    errors.push(e);
                }
            }
        }

        // Tüm okuma denemeleri başarısız olduysa
        Err(RaidError::IoError(errors))
    }

    /// RAID-1 Yazma: Tüm disklere yazma işlemi başarılı olmalıdır.
    fn write_block(&self, id: BlockId, data: &[u8]) -> Result<(), Self::Error> {
        let mut errors = Vec::new();
        let mut successful_writes = 0;

        // Tüm cihazlara yaz.
        for device in self.devices.iter() {
            match device.write_block(id, data) {
                Ok(_) => successful_writes += 1,
                Err(e) => {
                    // Yazma hatasını kaydet.
                    errors.push(e);
                }
            }
        }
        
        // RAID-1 için: Tüm kopyaların yazılması idealdir.
        if successful_writes < self.devices.len() {
            // Yazma başarısız oldu, hata döndürülmeli.
            Err(RaidError::IoError(errors)) 
        } else {
            Ok(())
        }
    }

    /// RAID-1'in mantıksal toplam blok sayısını döndürür.
    fn total_blocks(&self) -> BlockId {
        self.total_blocks
    }

    /// Tüm disklere kalıcılık (flush) komutunu gönderir.
    fn flush(&self) -> Result<(), Self::Error> {
        let mut errors = Vec::new();
        let mut successful_flushes = 0;

        for device in self.devices.iter() {
            match device.flush() {
                Ok(_) => successful_flushes += 1,
                Err(e) => errors.push(e),
            }
        }
        
        if successful_flushes < self.devices.len() {
             Err(RaidError::IoError(errors)) 
        } else {
            Ok(())
        }
    }
}