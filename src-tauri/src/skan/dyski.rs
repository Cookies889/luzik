//! Lista dyskow: Windows przez GetLogicalDrives i GetVolumeInformationW,
//! Linux przez /proc/mounts i statvfs (tylko prawdziwe urzadzenia z /dev).

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Dysk {
    /// Windows: "C:". Linux: punkt montowania.
    pub litera: String,
    /// Sciezka, ktora mozna podac do `skanuj`.
    pub sciezka: String,
    pub etykieta: String,
    pub rozmiar: u64,
    pub wolne: u64,
    pub system_plikow: String,
    /// "staly", "wymienny", "sieciowy", "optyczny", "ram" albo "inny".
    pub rodzaj: &'static str,
}

#[cfg(windows)]
pub fn lista_dyskow() -> Vec<Dysk> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    fn tekst(bufor: &[u16]) -> String {
        let koniec = bufor.iter().position(|&c| c == 0).unwrap_or(bufor.len());
        String::from_utf16_lossy(&bufor[..koniec])
    }

    let mut wynik = Vec::new();
    // SAFETY: wszystkie bufory maja podana dlugosc, sciezki koncza sie zerem.
    unsafe {
        let maska = GetLogicalDrives();
        for i in 0..26u32 {
            if maska & (1 << i) == 0 {
                continue;
            }
            let litera = (b'A' + i as u8) as char;
            let korzen: Vec<u16> = format!("{litera}:\\").encode_utf16().chain(std::iter::once(0)).collect();
            let rodzaj = match GetDriveTypeW(korzen.as_ptr()) {
                2 => "wymienny",
                3 => "staly",
                4 => "sieciowy",
                5 => "optyczny",
                6 => "ram",
                _ => "inny",
            };
            let (mut dla_mnie, mut razem, mut wolne) = (0u64, 0u64, 0u64);
            if GetDiskFreeSpaceExW(korzen.as_ptr(), &mut dla_mnie, &mut razem, &mut wolne) == 0 {
                // Pusty naped (czytnik bez plyty, odlaczony zasob): nie ma czego skanowac.
                continue;
            }
            let mut etykieta = [0u16; 261];
            let mut system = [0u16; 261];
            let (mut numer, mut dlugosc, mut flagi) = (0u32, 0u32, 0u32);
            GetVolumeInformationW(
                korzen.as_ptr(),
                etykieta.as_mut_ptr(),
                etykieta.len() as u32,
                &mut numer,
                &mut dlugosc,
                &mut flagi,
                system.as_mut_ptr(),
                system.len() as u32,
            );
            wynik.push(Dysk {
                litera: format!("{litera}:"),
                sciezka: format!("{litera}:\\"),
                etykieta: tekst(&etykieta),
                rozmiar: razem,
                wolne: dla_mnie,
                system_plikow: tekst(&system),
                rodzaj,
            });
        }
    }
    wynik
}

#[cfg(unix)]
pub fn lista_dyskow() -> Vec<Dysk> {
    use std::collections::HashSet;
    use std::ffi::CString;

    /// /proc/mounts zapisuje spacje i tabulatory jako \040 i \011.
    fn odkoduj(s: &str) -> String {
        s.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\")
    }

    let tabela = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut widziane = HashSet::new();
    let mut wynik = Vec::new();
    for linia in tabela.lines() {
        let mut pola = linia.split_whitespace();
        let (Some(urzadzenie), Some(punkt), Some(system)) = (pola.next(), pola.next(), pola.next()) else {
            continue;
        };
        if !urzadzenie.starts_with("/dev/") || !widziane.insert(urzadzenie.to_string()) {
            continue;
        }
        let punkt = odkoduj(punkt);
        let Ok(c) = CString::new(punkt.as_bytes()) else { continue };
        // SAFETY: statvfs dostaje poprawny napis C i wskaznik na wyzerowana strukture.
        let (rozmiar, wolne) = unsafe {
            let mut s: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c.as_ptr(), &mut s) != 0 {
                continue;
            }
            (s.f_blocks as u64 * s.f_frsize as u64, s.f_bavail as u64 * s.f_frsize as u64)
        };
        if rozmiar == 0 {
            continue;
        }
        wynik.push(Dysk {
            litera: punkt.clone(),
            sciezka: punkt,
            etykieta: urzadzenie.trim_start_matches("/dev/").to_string(),
            rozmiar,
            wolne,
            system_plikow: system.to_string(),
            rodzaj: "staly",
        });
    }
    wynik
}
