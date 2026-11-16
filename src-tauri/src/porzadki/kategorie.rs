//! Co zajmuje dysk wedlug rodzaju (wideo, gry, obrazy, archiwa, instalatory, kod, system)
//! oraz duze pliki dawno nieotwierane. Liczy z drzewa ostatniego skanu.
//!
//! Rodzaj pliku: najpierw katalog, w ktorym lezy (wszystko w steamapps to gry, wszystko
//! w C:\Windows to system, wszystko w node_modules to kod), potem rozszerzenie.

use crate::skan::drzewo::{Drzewo, Wiersz, RODZAJ_KATALOG, RODZAJ_PLIK};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Kategoria {
    Wideo,
    Gry,
    Obrazy,
    Muzyka,
    Dokumenty,
    Archiwa,
    Instalatory,
    Kod,
    System,
    Inne,
}

pub const WSZYSTKIE: [Kategoria; 10] = [
    Kategoria::Wideo,
    Kategoria::Gry,
    Kategoria::Obrazy,
    Kategoria::Muzyka,
    Kategoria::Dokumenty,
    Kategoria::Archiwa,
    Kategoria::Instalatory,
    Kategoria::Kod,
    Kategoria::System,
    Kategoria::Inne,
];

impl Kategoria {
    pub fn nazwa(self) -> &'static str {
        match self {
            Kategoria::Wideo => "Filmy",
            Kategoria::Gry => "Gry",
            Kategoria::Obrazy => "Zdjęcia i obrazy",
            Kategoria::Muzyka => "Muzyka",
            Kategoria::Dokumenty => "Dokumenty",
            Kategoria::Archiwa => "Archiwa",
            Kategoria::Instalatory => "Instalatory",
            Kategoria::Kod => "Kod i narzędzia programisty",
            Kategoria::System => "System",
            Kategoria::Inne => "Inne",
        }
    }
    fn indeks(self) -> usize {
        WSZYSTKIE.iter().position(|&k| k == self).unwrap_or(9)
    }
}

const WIDEO: &[&str] = &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "m2ts", "mts", "3gp", "vob"];
const OBRAZY: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "heic", "heif", "tif", "tiff", "raw", "cr2", "cr3", "nef", "arw", "dng",
    "psd", "svg", "avif", "jxl",
];
const MUZYKA: &[&str] = &["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "aiff", "mid"];
const DOKUMENTY: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "rtf", "epub", "csv", "md",
];
const ARCHIWA: &[&str] = &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst"];
const INSTALATORY: &[&str] = &["msi", "msix", "msixbundle", "appx", "appxbundle", "iso", "img"];
const KOD: &[&str] = &[
    "rs", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "pyc", "c", "h", "cpp", "hpp", "cc", "cs", "java", "class", "kt", "go",
    "rb", "php", "html", "css", "scss", "vue", "svelte", "swift", "lua", "sql", "rlib", "pdb",
];
const GRY: &[&str] = &["pak", "vpk", "bsa", "ba2", "ucas", "utoc", "upk", "wad"];
const SYSTEM: &[&str] = &["sys", "msu", "etl", "evtx", "dmp"];

/// Katalogi, w ktorych wszystko jest gra.
const KATALOGI_GIER: &[&str] = &[
    "steamapps", "epic games", "riot games", "xboxgames", "gog games", "gog galaxy", "ubisoft game launcher",
    "ea games", "origin games", "battle.net", "rockstar games",
];
/// Katalogi, w ktorych wszystko jest kodem albo paczkami programisty.
const KATALOGI_KODU: &[&str] = &[
    "node_modules", ".git", "__pycache__", ".venv", "venv", "site-packages", ".gradle", ".m2", ".cargo", ".rustup",
    ".nuget", "npm-cache",
];
/// Katalogi systemu, gdy leza w korzeniu dysku.
const KATALOGI_SYSTEMU: &[&str] = &["windows", "system volume information", "$winreagent", "recovery", "$recycle.bin"];
const PLIKI_SYSTEMU: &[&str] = &["hiberfil.sys", "pagefile.sys", "swapfile.sys"];

fn rozszerzenie(nazwa: &str) -> String {
    nazwa.rsplit_once('.').map(|(_, r)| r.to_lowercase()).unwrap_or_default()
}

/// Rodzaj nadany przez katalog (dziedziczony przez cale poddrzewo).
pub fn kontekst_katalogu(nazwa: &str, w_korzeniu: bool) -> Option<Kategoria> {
    let n = nazwa.to_lowercase();
    if KATALOGI_GIER.contains(&n.as_str()) {
        Some(Kategoria::Gry)
    } else if KATALOGI_KODU.contains(&n.as_str()) {
        Some(Kategoria::Kod)
    } else if w_korzeniu && KATALOGI_SYSTEMU.contains(&n.as_str()) {
        Some(Kategoria::System)
    } else if n == "package cache" {
        Some(Kategoria::Instalatory)
    } else {
        None
    }
}

/// Rodzaj pliku po nazwie, gdy katalog nic nie mowi.
pub fn kategoria_pliku(nazwa: &str) -> Kategoria {
    let male = nazwa.to_lowercase();
    if PLIKI_SYSTEMU.contains(&male.as_str()) {
        return Kategoria::System;
    }
    let r = rozszerzenie(&male);
    let r = r.as_str();
    if WIDEO.contains(&r) {
        Kategoria::Wideo
    } else if OBRAZY.contains(&r) {
        Kategoria::Obrazy
    } else if MUZYKA.contains(&r) {
        Kategoria::Muzyka
    } else if DOKUMENTY.contains(&r) {
        Kategoria::Dokumenty
    } else if ARCHIWA.contains(&r) {
        Kategoria::Archiwa
    } else if INSTALATORY.contains(&r)
        || (r == "exe" && (male.contains("setup") || male.contains("install") || male.contains("instal")))
    {
        Kategoria::Instalatory
    } else if KOD.contains(&r) {
        Kategoria::Kod
    } else if GRY.contains(&r) {
        Kategoria::Gry
    } else if SYSTEM.contains(&r) {
        Kategoria::System
    } else {
        Kategoria::Inne
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct WynikKategorii {
    pub kategoria: Kategoria,
    pub nazwa: &'static str,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pliki: u64,
    /// Najwieksze pliki tej kategorii, ze sciezkami.
    pub najwieksze: Vec<Wiersz>,
}

/// Ile najwiekszych plikow pokazujemy przy kategorii.
pub const NAJWIEKSZE_W_KATEGORII: usize = 10;

/// Podzial calego skanu na kategorie, od najwiekszej. Puste kategorie pomijamy.
pub fn policz(d: &Drzewo) -> Vec<WynikKategorii> {
    let mut sumy = [(0u64, 0u64, 0u64); 10];
    let mut kopce: Vec<BinaryHeap<Reverse<(u64, u32)>>> = (0..10).map(|_| BinaryHeap::new()).collect();
    if d.korzen().is_none() {
        return Vec::new();
    }
    // Korzen trzyma cala sciezke skanu: liczy sie jego ostatni czlon (skan C:\Windows to system).
    let nazwa_korzenia = d.nazwa(0);
    let ostatni = nazwa_korzenia.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("").to_string();
    let mut stos: Vec<(u32, Option<Kategoria>, bool)> = vec![(0, kontekst_katalogu(&ostatni, false), true)];
    while let Some((id, kontekst, korzen)) = stos.pop() {
        for dziecko in d.id_dzieci(id) {
            let Some(w) = d.wezel(dziecko) else { continue };
            if w.usuniety() {
                continue;
            }
            let nazwa = d.nazwa(dziecko);
            match w.rodzaj() {
                RODZAJ_KATALOG => {
                    stos.push((dziecko, kontekst.or_else(|| kontekst_katalogu(&nazwa, korzen)), false));
                }
                RODZAJ_PLIK if !w.duplikat() => {
                    let k = kontekst.unwrap_or_else(|| kategoria_pliku(&nazwa)).indeks();
                    sumy[k].0 += w.rozmiar;
                    sumy[k].1 += w.na_dysku;
                    sumy[k].2 += 1;
                    let kopiec = &mut kopce[k];
                    kopiec.push(Reverse((w.na_dysku, dziecko)));
                    if kopiec.len() > NAJWIEKSZE_W_KATEGORII {
                        kopiec.pop();
                    }
                }
                _ => {}
            }
        }
    }
    let mut wynik: Vec<WynikKategorii> = WSZYSTKIE
        .iter()
        .zip(sumy.iter().zip(kopce))
        .filter(|(_, ((_, _, pliki), _))| *pliki > 0)
        .map(|(&kategoria, (&(rozmiar, na_dysku, pliki), kopiec))| {
            let mut ids: Vec<(u64, u32)> = kopiec.into_iter().map(|r| r.0).collect();
            ids.sort_unstable_by(|a, b| b.cmp(a));
            WynikKategorii {
                kategoria,
                nazwa: kategoria.nazwa(),
                rozmiar,
                na_dysku,
                pliki,
                najwieksze: ids.into_iter().filter_map(|(_, i)| d.wiersz(i, true)).collect(),
            }
        })
        .collect();
    wynik.sort_by(|a, b| b.na_dysku.cmp(&a.na_dysku));
    wynik
}

#[derive(Serialize, Clone, Debug)]
pub struct StaryPlik {
    pub sciezka: String,
    pub rozmiar: u64,
    pub na_dysku: u64,
    /// Ostatnia zmiana, sekundy od 1970.
    pub zmieniony: i64,
    /// Ostatni dostep, gdy system go zapisuje (Windows czesto nie aktualizuje go na duzych dyskach).
    pub otwarty: Option<i64>,
    /// Ile pelnych dni minelo od pozniejszej z dat: zmiany i dostepu.
    pub dni: u64,
}

fn sekundy(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// Pliki co najmniej `min_rozmiar` bajtow, nieruszane (ani zmiana, ani dostep) od `min_dni`,
/// od najwiekszego, najwyzej `n`. Data zmiany jest w drzewie, data dostepu czytana z dysku.
pub fn stare_duze(d: &Drzewo, min_rozmiar: u64, min_dni: u64, teraz: i64, n: usize) -> Vec<StaryPlik> {
    let prog = teraz - (min_dni as i64) * 86_400;
    let mut kandydaci: Vec<u32> = (0..d.liczba_wezlow() as u32)
        .filter(|&i| {
            d.wezel(i).is_some_and(|w| {
                w.rodzaj() == RODZAJ_PLIK && !w.duplikat() && !w.usuniety() && w.rozmiar >= min_rozmiar && w.mtime <= prog
            })
        })
        .collect();
    kandydaci.sort_unstable_by_key(|&i| Reverse(d.wezel(i).map_or(0, |w| w.na_dysku)));
    let mut wynik = Vec::new();
    for i in kandydaci {
        if wynik.len() >= n {
            break;
        }
        let Some(w) = d.wezel(i) else { continue };
        let sciezka = d.sciezka(i);
        let otwarty = std::fs::symlink_metadata(&sciezka).ok().and_then(|m| sekundy(m.accessed()));
        let ostatnio = otwarty.map_or(w.mtime, |a| a.max(w.mtime));
        if ostatnio > prog {
            continue;
        }
        wynik.push(StaryPlik {
            sciezka,
            rozmiar: w.rozmiar,
            na_dysku: w.na_dysku,
            zmieniony: w.mtime,
            otwarty,
            dni: ((teraz - ostatnio).max(0) / 86_400) as u64,
        });
    }
    wynik
}
