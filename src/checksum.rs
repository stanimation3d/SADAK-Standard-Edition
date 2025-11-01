// src/checksum.rs

#![allow(dead_code)] // Şimdilik sadece fonksiyonları tanımlıyoruz

use core::u32;

// --- 1. CRC32C Sabitleri ---
// CRC32C (Castagnoli) polinomu: x^32 + x^28 + x^27 + x^26 + x^25 + x^23 + x^22 + x^20 + x^19 + x^18 + x^14 + x^13 + x^11 + x^10 + x^9 + x^8 + x^6 + x^0
const CRC32C_POLY: u32 = 0x82F63B78;
const CRC32C_INITIAL: u32 = 0xFFFFFFFF;


// --- 2. CRC32C Arama Tablosu (Lookup Table) ---
// CRC hesaplamasını bayt bazında hızlandırmak için 256 elemanlı bir tablo.
const CRC32C_TABLE: [u32; 256] = init_crc32c_table();

/// Derleme zamanında (veya sabit olarak) CRC32C arama tablosunu hesaplar.
/// Bu, çalışma zamanında pahalı hesaplamayı önler.
const fn init_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 == 1 {
                // Eğer en düşük bit 1 ise, CRC'yi sağa kaydır ve polinom ile XOR'la.
                crc = (crc >> 1) ^ CRC32C_POLY;
            } else {
                // Değilse, sadece sağa kaydır.
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}


// --- 3. CRC Hesaplama Fonksiyonu ---

/// Verilen bayt dizisinin CRC32C kontrol toplamını hesaplar.
///
/// # Parametreler
/// * `data`: Kontrol edilecek veri dilimi (`&[u8]`).
/// * `initial_crc`: Önceki verilerden devam etmek için başlangıç CRC değeri (genellikle 0xFFFFFFFF).
///
/// # Döndürür
/// Hesaplanan 32-bit CRC32C değeri.
pub fn calculate_crc32c(data: &[u8], initial_crc: u32) -> u32 {
    // CRC başlangıç değerini ters çevir (algoritmanın gerektirdiği XOR ile)
    let mut crc = initial_crc ^ CRC32C_INITIAL;

    // Arama tablosu kullanarak hızlı hesaplama
    for &byte in data {
        // Geçerli CRC'nin en düşük 8 bitini al
        let index = (crc as u8) ^ byte; 
        
        // CRC'yi 8 bit sağa kaydır
        crc = (crc >> 8) ^ CRC32C_TABLE[index as usize];
    }

    // Nihai sonucu ters çevirip döndür (algoritmanın gerektirdiği XOR ile)
    crc ^ CRC32C_INITIAL
}


// --- 4. Kolaylık Fonksiyonu ---

/// Verilen bayt dizisinin varsayılan başlangıç değeri (0xFFFFFFFF) ile CRC32C'sini hesaplar.
pub fn checksum_data(data: &[u8]) -> u32 {
    calculate_crc32c(data, CRC32C_INITIAL)
}