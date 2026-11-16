//! Silnik skanu: rownolegly przeglad katalogu i zwarte drzewo wyniku.
//!
//! - rozmiar logiczny i miejsce na dysku osobno (pliki rzadkie i skompresowane),
//! - twardy link liczony raz, po (wolumin, numer pliku),
//! - do dowiazan symbolicznych i junctionow skan nie wchodzi, liczy je osobno,
//! - brak dostepu nie przerywa skanu: trafia do licznika i na liste sciezek,
//! - postep co 100 ms i anulowanie flaga,
//! - Windows, caly dysk NTFS, uprawnienia administratora: zamiast chodzic po katalogach
//!   skan czyta ciagiem tablice plikow (`mft`); bez uprawnien idzie zwykla droga.
//!
//! Modul nie zna Tauri: te same funkcje wola aplikacja, `luzik-cli` i testy.

pub mod drzewo;
pub mod dyski;
#[cfg_attr(not(windows), allow(dead_code))]
mod mft;
pub(crate) mod platforma;
mod przeglad;
#[cfg(all(test, unix))]
mod testy;
#[cfg(test)]
mod testy_wspolne;

pub use drzewo::{Drzewo, Wezel, Wiersz};
pub use dyski::{lista_dyskow, Dysk};
pub use przeglad::skanuj;

use serde::Serialize;

/// Ile sciezek z bledem dostepu trzymamy w pamieci (licznik liczy wszystkie).
pub const LIMIT_LISTY_BLEDOW: usize = 5000;
/// Odstep miedzy zdarzeniami postepu.
pub const ODSTEP_POSTEPU_MS: u64 = 100;

#[derive(Clone, Debug, Default)]
pub struct Opcje {
    /// Liczba watkow; 0 = dobierz sam.
    pub watki: usize,
    /// Linux: wchodz tez w inne systemy plikow (punkty montowania). Domyslnie nie,
    /// zeby skan `/` nie liczyl /proc, /sys ani podpietych dyskow.
    pub przez_systemy: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Postep {
    pub pliki: u64,
    pub katalogi: u64,
    /// Bajty logiczne policzone do tej pory.
    pub bajty: u64,
    pub bledy: u64,
    pub pliki_na_s: u64,
    pub czas_ms: u64,
    pub biezacy: String,
    pub koniec: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Blad {
    pub sciezka: String,
    pub powod: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Statystyki {
    /// Wpisy inne niz katalogi: pliki, dowiazania, urzadzenia.
    pub pliki: u64,
    pub katalogi: u64,
    /// Dowiazania symboliczne i junctiony (policzone, nieprzegladane).
    pub dowiazania: u64,
    /// Kolejne nazwy tego samego pliku (twarde linki) pominiete w sumie.
    pub twarde_linki_pominiete: u64,
    /// Katalogi z innych systemow plikow, do ktorych skan nie wszedl.
    pub inne_systemy: u64,
    pub bledy_razem: u64,
    pub bledy: Vec<Blad>,
    pub czas_ms: u64,
    pub anulowano: bool,
}

pub struct Wynik {
    pub drzewo: Drzewo,
    pub statystyki: Statystyki,
}

impl Wynik {
    /// Suma rozmiarow logicznych calego skanu.
    pub fn suma(&self) -> u64 {
        self.drzewo.korzen().map_or(0, |w| w.rozmiar)
    }
    /// Suma miejsca zajetego na dysku.
    pub fn na_dysku(&self) -> u64 {
        self.drzewo.korzen().map_or(0, |w| w.na_dysku)
    }
}
