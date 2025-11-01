// src/sahne_syscalls.rs

#![allow(unused)] // Kullanılmayan kod uyarılarını şimdilik bastırır

// --- 1. Sabit Tanımlamaları ---
// Bu sabitler, Sahne64 sistem çağrı numaralarını temsil eder.
// Bu numaralar, çekirdekteki karşılıklarıyla eşleşmelidir.
pub const SYSCALL_MEMORY_ALLOCATE: u64 = 1;
pub const SYSCALL_MEMORY_RELEASE: u64 = 2;
// Görev/İş parçacığı çağrıları (Kullanılmayacak, ancak referans için tutuluyor)
pub const SYSCALL_TASK_SPAWN: u64 = 3;
pub const SYSCALL_TASK_EXIT: u64 = 4;
// Kaynak (Resource/Aygıt) Yönetimi: SADAK için KRİTİK
pub const SYSCALL_RESOURCE_ACQUIRE: u64 = 5; 
pub const SYSCALL_RESOURCE_READ: u64 = 6;
pub const SYSCALL_RESOURCE_WRITE: u64 = 7;
pub const SYSCALL_RESOURCE_RELEASE: u64 = 8;
pub const SYSCALL_GET_TASK_ID: u64 = 9;
pub const SYSCALL_TASK_SLEEP: u64 = 10;
// Senkronizasyon (Kilit Yönetimi): SADAK için KRİTİK
pub const SYSCALL_LOCK_CREATE: u64 = 11;
pub const SYSCALL_LOCK_ACQUIRE: u64 = 12;
pub const SYSCALL_LOCK_RELEASE: u64 = 13;
pub const SYSCALL_GET_SYSTEM_TIME: u64 = 16;
// Yeni I/O ve Kontrol Çağrıları
pub const SYSCALL_RESOURCE_CONTROL: u64 = 102;
pub const SYSCALL_RESOURCE_SEEK: u64 = 103;   


// --- 2. Temel Veri Tipleri ---
// Sahne64'ün kullandığı temel I/O türleri.
pub type ResourceHandle = u64;
pub type Offset = u64;
pub type Length = usize;
pub type SyscallResult = isize; // Sistem çağrılarından dönen sonuç.

// --- 3. Düşük Seviyeli Çağırma Fonksiyonu (Platforma Özel) ---
// Bu fonksiyon, belirli bir mimariye (x86_64, riscv64 vb.) özel montaj (assembly) 
// kodu ile çekirdeğe atlama (trap) işlemini gerçekleştirir. 
// SADAK'ta bu çekirdek tarafından sağlanmalıdır.
extern "C" {
    /// Sistem çağrısı ID'sini ve altı adede kadar argümanı alır.
    pub fn raw_syscall(
        syscall_id: u64,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
        arg6: u64,
    ) -> SyscallResult;
}


// --- 4. Hata Türü Tanımı ---

/// Ham sistem çağrısı hatalarını Rust'ta temsil eden enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallError {
    EIO,        // I/O Hatası
    EINVAL,     // Geçersiz Argüman
    ENOMEM,     // Bellek Hatası
    EAGAIN,     // Tekrar Dene
    Unknown(isize), // Bilinmeyen hata
}

impl SyscallError {
    /// Ham sistem çağrısı sonucunu uygun hata türüne dönüştürür.
    pub fn from_raw(raw_code: isize) -> Self {
        // Negatif değerler hata kodunu gösterir (POSIX geleneği varsayımı).
        match raw_code.abs() {
            1001 => SyscallError::EIO,
            1002 => SyscallError::EINVAL,
            1003 => SyscallError::ENOMEM,
            1004 => SyscallError::EAGAIN,
            _ => SyscallError::Unknown(raw_code),
        }
    }
}


// --- 5. Yüksek Seviyeli Sarmalayıcı Fonksiyonlar (Örnek) ---

/// Fiziksel bir kaynağa (sürücüye) erişim tanıtıcısı (handle) alır.
pub fn resource_acquire(path_ptr: *const u8, path_len: Length) -> Result<ResourceHandle, SyscallError> {
    let result = unsafe {
        raw_syscall(
            SYSCALL_RESOURCE_ACQUIRE,
            path_ptr as u64,
            path_len as u64,
            0, 0, 0, 0,
        )
    };
    if result < 0 {
        Err(SyscallError::from_raw(result))
    } else {
        Ok(result as ResourceHandle)
    }
}

/// Örnek: Kaynaktan veri okur. (Diğer I/O fonksiyonları da benzer şekilde tanımlanmıştır)
pub fn resource_read(handle: ResourceHandle, buffer_ptr: *mut u8, length: Length) -> Result<Length, SyscallError> {
    let result = unsafe {
        raw_syscall(
            SYSCALL_RESOURCE_READ,
            handle,
            buffer_ptr as u64,
            length as u64,
            0, 0, 0,
        )
    };
    if result < 0 {
        Err(SyscallError::from_raw(result))
    } else {
        Ok(result as Length) // Başarıyla okunan bayt sayısı
    }
}