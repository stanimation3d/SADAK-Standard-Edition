// src/lib.rs

// --- 1. no-std ve Kısıtlama Ayarları ---

// Standart kütüphaneyi (std) kullanma. Bu, projenizin çekirdek/OS bağımsız çalışmasını sağlar.
#![no_std]
// Normal main fonksiyonunu kullanma (ikili dosya için main.rs'de no_main kullanıldı).
#![no_main] 
// Geliştirme aşamasında yardımcı olması için kullanılmayan uyarıları kaldır.
#![allow(dead_code, unused_variables)] 

// 'alloc' kütüphanesini dahil et. Bu, Vec, Box, Arc gibi dinamik koleksiyonları
// no-std ortamında kullanmamızı sağlar (ancak tahsisçinin çekirdek tarafından sağlanması gerekir).
extern crate alloc; 


// --- 2. Modül Tanımlamaları ---

// Sahne64 sistem çağrılarını sarmalayan düşük seviyeli I/O modülü.
pub mod sahne_syscalls;

// Disk I/O'yu soyutlayan temel katman (HDD, SSD, vb.).
pub mod block_device;

// RAID-1 (Mirroring) uygulamasını BlockDevice trait'i üzerine kurar.
pub mod raid;

// Metadata bütünlüğü için CRC32C Checksum hesaplama modülü.
pub mod checksum;

// Blok önbelleği, kilit yönetimi ve bellek tahsisini yöneten modül.
pub mod cache; 

// Copy-on-Write için temel B-Ağacı (B-Tree) yapıları.
pub mod btree;

// Disk üzerindeki boş/dolu blokların yönetimini yapan Tahsis Yöneticisi.
pub mod allocator;

// SADAK'ın ana yapısını, Superblock'u ve dosya sistemi API'lerini içerir.
pub mod fs;