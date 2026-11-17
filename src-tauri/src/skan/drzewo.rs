//! Drzewo wyniku skanu: jedna tablica wezlow (arena) i jeden bufor na wszystkie nazwy.
//! Wezel ma 48 bajtow, nazwa srednio ok. 25, wiec 2 mln plikow to ok. 150 MB.

use serde::Serialize;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const BRAK: u32 = u32::MAX;

pub const RODZAJ_PLIK: u8 = 0;
pub const RODZAJ_KATALOG: u8 = 1;
pub const RODZAJ_DOWIAZANIE: u8 = 2;
pub const RODZAJ_INNY: u8 = 3;
const MASKA_RODZAJU: u8 = 0b111;
/// Kolejne wystapienie twardego linku: rozmiar zostaje w wezle, ale nie wchodzi do sum katalogow.
pub const FLAGA_DUPLIKAT: u8 = 1 << 3;
/// Katalogu albo wpisu nie udalo sie odczytac (brak uprawnien).
pub const FLAGA_NIEDOSTEPNY: u8 = 1 << 4;
/// Katalog z innego systemu plikow (punkt montowania): policzony, ale nieprzegladany.
pub const FLAGA_INNY_SYSTEM: u8 = 1 << 5;
/// Wpis usuniety przez Luzika po skanie: zostaje w tablicy (numery innych wezlow sie nie zmieniaja),
/// ale znika z list i sum.
pub const FLAGA_USUNIETY: u8 = 1 << 6;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Wezel {
    /// Rozmiar logiczny w bajtach; dla katalogu suma calego poddrzewa.
    pub rozmiar: u64,
    /// Miejsce zajete na dysku w bajtach; dla katalogu suma calego poddrzewa.
    pub na_dysku: u64,
    /// Czas zmiany, sekundy od 1970.
    pub mtime: i64,
    pub rodzic: u32,
    pub pierwsze_dziecko: u32,
    pub liczba_dzieci: u32,
    /// Liczba wpisow innych niz katalogi w poddrzewie (dla pliku 1).
    pub pliki: u32,
    pub nazwa_od: u32,
    pub nazwa_dl: u16,
    pub flagi: u8,
}

impl Wezel {
    pub fn rodzaj(&self) -> u8 {
        self.flagi & MASKA_RODZAJU
    }
    pub fn katalog(&self) -> bool {
        self.rodzaj() == RODZAJ_KATALOG
    }
    pub fn duplikat(&self) -> bool {
        self.flagi & FLAGA_DUPLIKAT != 0
    }
    pub fn usuniety(&self) -> bool {
        self.flagi & FLAGA_USUNIETY != 0
    }
}

#[derive(Default, Clone)]
pub struct Drzewo {
    pub(crate) wezly: Vec<Wezel>,
    pub(crate) nazwy: Vec<u8>,
}

/// Wiersz listy dla interfejsu.
#[derive(Serialize, Clone, Debug)]
pub struct Wiersz {
    pub id: u32,
    pub nazwa: String,
    pub rodzaj: &'static str,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pliki: u32,
    pub mtime: i64,
    pub liczba_dzieci: u32,
    pub duplikat: bool,
    pub niedostepny: bool,
    pub inny_system: bool,
    /// Pelna sciezka; wypelniana tylko tam, gdzie lista jest plaska (najwieksze pliki).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sciezka: Option<String>,
}

fn nazwa_rodzaju(r: u8) -> &'static str {
    match r {
        RODZAJ_PLIK => "plik",
        RODZAJ_KATALOG => "katalog",
        RODZAJ_DOWIAZANIE => "dowiazanie",
        _ => "inny",
    }
}

impl Drzewo {
    pub fn liczba_wezlow(&self) -> usize {
        self.wezly.len()
    }

    pub fn wezel(&self, id: u32) -> Option<&Wezel> {
        self.wezly.get(id as usize)
    }

    pub fn korzen(&self) -> Option<&Wezel> {
        self.wezly.first()
    }

    pub fn nazwa(&self, id: u32) -> Cow<'_, str> {
        match self.wezly.get(id as usize) {
            Some(w) => {
                let od = w.nazwa_od as usize;
                String::from_utf8_lossy(&self.nazwy[od..od + w.nazwa_dl as usize])
            }
            None => Cow::Borrowed(""),
        }
    }

    /// Pelna sciezka wezla, zlozona z nazw przodkow (korzen trzyma cala sciezke skanu).
    pub fn sciezka(&self, id: u32) -> String {
        let mut lancuch = Vec::new();
        let mut i = id;
        while let Some(w) = self.wezly.get(i as usize) {
            lancuch.push(i);
            if w.rodzic == BRAK {
                break;
            }
            i = w.rodzic;
        }
        let mut s = String::new();
        for (n, i) in lancuch.iter().rev().enumerate() {
            if n > 0 && !s.ends_with(std::path::MAIN_SEPARATOR) {
                s.push(std::path::MAIN_SEPARATOR);
            }
            s.push_str(&self.nazwa(*i));
        }
        s
    }

    pub fn wiersz(&self, id: u32, ze_sciezka: bool) -> Option<Wiersz> {
        let w = self.wezly.get(id as usize)?;
        Some(Wiersz {
            id,
            nazwa: self.nazwa(id).into_owned(),
            rodzaj: nazwa_rodzaju(w.rodzaj()),
            rozmiar: w.rozmiar,
            na_dysku: w.na_dysku,
            pliki: w.pliki,
            mtime: w.mtime,
            liczba_dzieci: self.id_dzieci(id).filter(|&d| !self.wezly[d as usize].usuniety()).count() as u32,
            duplikat: w.duplikat(),
            niedostepny: w.flagi & FLAGA_NIEDOSTEPNY != 0,
            inny_system: w.flagi & FLAGA_INNY_SYSTEM != 0,
            sciezka: ze_sciezka.then(|| self.sciezka(id)),
        })
    }

    /// Identyfikatory dzieci wezla (leza w tablicy obok siebie).
    pub fn id_dzieci(&self, id: u32) -> std::ops::Range<u32> {
        match self.wezly.get(id as usize) {
            Some(w) if w.liczba_dzieci > 0 => w.pierwsze_dziecko..w.pierwsze_dziecko + w.liczba_dzieci,
            _ => 0..0,
        }
    }

    /// Dzieci wezla od najwiekszego (po miejscu na dysku, potem po rozmiarze logicznym).
    pub fn dzieci(&self, id: u32) -> Vec<Wiersz> {
        let mut ids: Vec<u32> = self.id_dzieci(id).filter(|&i| !self.wezly[i as usize].usuniety()).collect();
        ids.sort_unstable_by_key(|&i| {
            let w = &self.wezly[i as usize];
            Reverse((w.na_dysku, w.rozmiar))
        });
        ids.into_iter().filter_map(|i| self.wiersz(i, false)).collect()
    }

    /// `n` najwiekszych plikow calego skanu, od najwiekszego. Powtorki twardych linkow pomija.
    pub fn najwieksze_pliki(&self, n: usize) -> Vec<Wiersz> {
        if n == 0 {
            return Vec::new();
        }
        let mut kopiec: BinaryHeap<Reverse<(u64, u64, u32)>> = BinaryHeap::with_capacity(n + 1);
        for (i, w) in self.wezly.iter().enumerate() {
            if w.rodzaj() != RODZAJ_PLIK || w.duplikat() || w.usuniety() {
                continue;
            }
            let klucz = Reverse((w.na_dysku, w.rozmiar, i as u32));
            if kopiec.len() < n {
                kopiec.push(klucz);
            } else if let Some(min) = kopiec.peek() {
                if klucz.0 > min.0 {
                    kopiec.pop();
                    kopiec.push(klucz);
                }
            }
        }
        let mut v: Vec<(u64, u64, u32)> = kopiec.into_iter().map(|r| r.0).collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.into_iter().filter_map(|(_, _, i)| self.wiersz(i, true)).collect()
    }

    /// Numer wezla o tej pelnej sciezce (bez wielkosci liter na Windows) albo None, gdy sciezka
    /// lezy poza skanem albo jej wpis zostal juz usuniety.
    pub fn znajdz(&self, sciezka: &str) -> Option<u32> {
        let czlony = |s: &str| -> Vec<String> {
            s.split(['\\', '/'])
                .filter(|c| !c.is_empty() && *c != ".")
                .map(|c| if cfg!(windows) { c.to_lowercase() } else { c.to_string() })
                .collect()
        };
        let korzen = czlony(&self.nazwa(0));
        let cel = czlony(sciezka);
        if self.wezly.is_empty() || cel.len() < korzen.len() || cel[..korzen.len()] != korzen[..] {
            return None;
        }
        let mut id = 0u32;
        for czlon in &cel[korzen.len()..] {
            id = self.id_dzieci(id).find(|&d| {
                let n = self.nazwa(d);
                !self.wezly[d as usize].usuniety() && if cfg!(windows) { n.to_lowercase() == *czlon } else { *n == **czlon }
            })?;
        }
        (!self.wezly[id as usize].usuniety()).then_some(id)
    }

    /// Usuwa wezel z wyniku bez ponownego skanu: oznacza cale poddrzewo i odejmuje jego
    /// rozmiar od przodkow. Korzenia nie usuwa. Zwraca false, gdy nie bylo czego usunac.
    pub fn usun(&mut self, id: u32) -> bool {
        let Some(w) = self.wezly.get(id as usize).copied() else { return false };
        if id == 0 || w.usuniety() {
            return false;
        }
        let mut stos = vec![id];
        while let Some(i) = stos.pop() {
            self.wezly[i as usize].flagi |= FLAGA_USUNIETY;
            stos.extend(self.id_dzieci(i));
        }
        if !w.duplikat() {
            let mut r = w.rodzic;
            while let Some(p) = self.wezly.get_mut(r as usize) {
                p.rozmiar = p.rozmiar.saturating_sub(w.rozmiar);
                p.na_dysku = p.na_dysku.saturating_sub(w.na_dysku);
                p.pliki = p.pliki.saturating_sub(w.pliki);
                r = p.rodzic;
            }
        }
        true
    }

    /// Wezly, ktorych juz nie ma na dysku: sam wezel `id` i jego bezposrednie dzieci
    /// (np. znajomy skasowal je w Eksploratorze po skanie). Gdy brakuje samego wezla, dzieci
    /// nie sprawdza, bo i tak zniknely razem z nim.
    pub fn znikniete(&self, id: u32) -> Vec<u32> {
        let jest = |i: u32| std::fs::symlink_metadata(self.sciezka(i)).is_ok();
        match self.wezly.get(id as usize) {
            None => Vec::new(),
            Some(w) if w.usuniety() => Vec::new(),
            Some(_) if id != 0 && !jest(id) => vec![id],
            Some(_) => self.id_dzieci(id).filter(|&d| !self.wezly[d as usize].usuniety() && !jest(d)).collect(),
        }
    }

    /// `znajdz` i `usun` naraz.
    pub fn usun_sciezke(&mut self, sciezka: &str) -> bool {
        self.znajdz(sciezka).is_some_and(|id| self.usun(id))
    }

    /// Pamiec zajeta przez drzewo (wezly i nazwy), w bajtach.
    pub fn pamiec(&self) -> usize {
        self.wezly.capacity() * std::mem::size_of::<Wezel>() + self.nazwy.capacity()
    }

    /// Sumuje rozmiary w gore drzewa. Dziecko ma zawsze wiekszy numer niz rodzic,
    /// wiec jedno przejscie od konca wystarcza.
    pub(crate) fn zsumuj(&mut self) {
        for i in (1..self.wezly.len()).rev() {
            let w = self.wezly[i];
            if w.rodzic == BRAK || w.duplikat() {
                continue;
            }
            let r = &mut self.wezly[w.rodzic as usize];
            r.rozmiar += w.rozmiar;
            r.na_dysku += w.na_dysku;
            r.pliki = r.pliki.saturating_add(w.pliki);
        }
        self.wezly.shrink_to_fit();
        self.nazwy.shrink_to_fit();
    }
}
