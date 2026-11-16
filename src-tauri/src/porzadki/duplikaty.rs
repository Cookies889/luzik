//! Wyszukiwarka duplikatow: te same tresci w kilku plikach.
//!
//! Trzy sita, kazde tansze od nastepnego: (1) ten sam rozmiar, (2) ten sam skrot BLAKE3
//! pierwszych 64 KB, (3) ten sam skrot BLAKE3 calego pliku. Do drugiego i trzeciego sita
//! trafiaja tylko pliki, ktore przeszly poprzednie, wiec zwykle czyta sie malo.
//! Tylko pliki wieksze niz 1 MB (male duplikaty nie zwolnia miejsca, a jest ich tysiace).
//! Kolejne nazwy twardego linku to ten sam plik na dysku: skan oznacza je `duplikat`
//! i tu nie trafiaja, bo usuniecie ich nic nie zwalnia.

use crate::skan::drzewo::{Drzewo, RODZAJ_PLIK};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

/// Najmniejszy rozmiar pliku branego pod uwage (pliki wieksze niz 1 MB).
pub const MIN_ROZMIAR: u64 = 1024 * 1024;
/// Ile bajtow z poczatku pliku idzie do drugiego sita.
pub const PREFIKS: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Kandydat {
    pub sciezka: PathBuf,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub mtime: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct PlikDuplikatu {
    pub sciezka: String,
    pub na_dysku: u64,
    pub mtime: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Grupa {
    pub rozmiar: u64,
    /// Skrot BLAKE3 calej tresci (szesnastkowo, pierwsze 32 znaki).
    pub skrot: String,
    /// Pliki od najstarszego (najstarszy to zwykle oryginal).
    pub pliki: Vec<PlikDuplikatu>,
    /// Ile miejsca zwolni zostawienie jednej kopii.
    pub do_odzyskania: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct WynikDuplikatow {
    pub grupy: Vec<Grupa>,
    /// Pliki powyzej progu wziete pod uwage.
    pub kandydaci: u64,
    /// Pliki czytane w drugim i trzecim sicie.
    pub czytane_pliki: u64,
    pub czytane_bajty: u64,
    pub bledy: u64,
    pub do_odzyskania: u64,
    pub anulowano: bool,
}

/// Pliki z drzewa skanu wieksze niz 1 MB (bez powtorek twardych linkow).
pub fn kandydaci_z_drzewa(d: &Drzewo) -> Vec<Kandydat> {
    (0..d.liczba_wezlow() as u32)
        .filter_map(|i| {
            let w = d.wezel(i)?;
            (w.rodzaj() == RODZAJ_PLIK && !w.duplikat() && !w.usuniety() && w.rozmiar > MIN_ROZMIAR).then(|| Kandydat {
                sciezka: PathBuf::from(d.sciezka(i)),
                rozmiar: w.rozmiar,
                na_dysku: w.na_dysku,
                mtime: w.mtime,
            })
        })
        .collect()
}

struct Liczniki {
    pliki: AtomicU64,
    bajty: AtomicU64,
    bledy: AtomicU64,
}

fn skrot_poczatku(k: &Kandydat, l: &Liczniki) -> io::Result<[u8; 32]> {
    let mut bufor = vec![0u8; PREFIKS];
    let mut plik = File::open(&k.sciezka)?;
    let mut przeczytane = 0;
    while przeczytane < PREFIKS {
        let n = plik.read(&mut bufor[przeczytane..])?;
        if n == 0 {
            break;
        }
        przeczytane += n;
    }
    l.pliki.fetch_add(1, Relaxed);
    l.bajty.fetch_add(przeczytane as u64, Relaxed);
    Ok(*blake3::hash(&bufor[..przeczytane]).as_bytes())
}

fn skrot_calosci(k: &Kandydat, l: &Liczniki, anuluj: &AtomicBool) -> io::Result<[u8; 32]> {
    let mut plik = File::open(&k.sciezka)?;
    let mut hasher = blake3::Hasher::new();
    let mut bufor = vec![0u8; 1024 * 1024];
    let mut razem = 0u64;
    loop {
        if anuluj.load(Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "anulowano"));
        }
        let n = plik.read(&mut bufor)?;
        if n == 0 {
            break;
        }
        hasher.update(&bufor[..n]);
        razem += n as u64;
    }
    l.bajty.fetch_add(razem, Relaxed);
    // Plik zmienil sie miedzy skanem a czytaniem: nie porownujemy go.
    if razem != k.rozmiar {
        return Err(io::Error::new(io::ErrorKind::Other, "plik zmienil rozmiar"));
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Grupy wedlug klucza, tylko te z co najmniej dwoma plikami.
fn grupuj<K: std::hash::Hash + Eq>(pary: impl IntoIterator<Item = (K, Kandydat)>) -> Vec<Vec<Kandydat>> {
    let mut mapa: HashMap<K, Vec<Kandydat>> = HashMap::new();
    for (k, v) in pary {
        mapa.entry(k).or_default().push(v);
    }
    mapa.into_values().filter(|g| g.len() > 1).collect()
}

/// Szuka duplikatow wsrod kandydatow. Pliki do 1 MB i powtorzone sciezki sa pomijane.
pub fn znajdz(kandydaci: Vec<Kandydat>, anuluj: &AtomicBool) -> WynikDuplikatow {
    let mut widziane = std::collections::HashSet::new();
    let kandydaci: Vec<Kandydat> =
        kandydaci.into_iter().filter(|k| k.rozmiar > MIN_ROZMIAR && widziane.insert(k.sciezka.clone())).collect();
    let mut wynik = WynikDuplikatow { kandydaci: kandydaci.len() as u64, ..Default::default() };
    let l = Liczniki { pliki: AtomicU64::new(0), bajty: AtomicU64::new(0), bledy: AtomicU64::new(0) };

    // Sito 1: rozmiar.
    let po_rozmiarze = grupuj(kandydaci.into_iter().map(|k| (k.rozmiar, k)));

    // Sito 2: poczatek pliku.
    let po_poczatku: Vec<Vec<Kandydat>> = po_rozmiarze
        .into_par_iter()
        .flat_map_iter(|grupa| {
            let pary = grupa.into_iter().filter_map(|k| {
                if anuluj.load(Relaxed) {
                    return None;
                }
                match skrot_poczatku(&k, &l) {
                    Ok(s) => Some(((k.rozmiar, s), k)),
                    Err(_) => {
                        l.bledy.fetch_add(1, Relaxed);
                        None
                    }
                }
            });
            grupuj(pary)
        })
        .collect();

    // Sito 3: cala tresc. Rownolegle po plikach, bo grupy bywaja nierowne.
    let do_czytania: Vec<(usize, Kandydat)> =
        po_poczatku.into_iter().enumerate().flat_map(|(i, g)| g.into_iter().map(move |k| (i, k))).collect();
    let pelne: Vec<((usize, [u8; 32]), Kandydat)> = do_czytania
        .into_par_iter()
        .filter_map(|(i, k)| match skrot_calosci(&k, &l, anuluj) {
            Ok(s) => Some(((i, s), k)),
            Err(_) => {
                l.bledy.fetch_add(1, Relaxed);
                None
            }
        })
        .collect();
    let mut skroty: HashMap<(usize, [u8; 32]), Vec<Kandydat>> = HashMap::new();
    for (klucz, k) in pelne {
        skroty.entry(klucz).or_default().push(k);
    }

    for ((_, skrot), mut pliki) in skroty {
        if pliki.len() < 2 {
            continue;
        }
        pliki.sort_by(|a, b| a.mtime.cmp(&b.mtime).then(a.sciezka.cmp(&b.sciezka)));
        let razem: u64 = pliki.iter().map(|p| p.na_dysku).sum();
        let najwiekszy = pliki.iter().map(|p| p.na_dysku).max().unwrap_or(0);
        let hex: String = skrot[..16].iter().map(|b| format!("{b:02x}")).collect();
        wynik.grupy.push(Grupa {
            rozmiar: pliki[0].rozmiar,
            skrot: hex,
            do_odzyskania: razem - najwiekszy,
            pliki: pliki
                .into_iter()
                .map(|p| PlikDuplikatu { sciezka: p.sciezka.to_string_lossy().into_owned(), na_dysku: p.na_dysku, mtime: p.mtime })
                .collect(),
        });
    }
    wynik.grupy.sort_by(|a, b| b.do_odzyskania.cmp(&a.do_odzyskania).then(a.skrot.cmp(&b.skrot)));
    wynik.do_odzyskania = wynik.grupy.iter().map(|g| g.do_odzyskania).sum();
    wynik.czytane_pliki = l.pliki.load(Relaxed);
    wynik.czytane_bajty = l.bajty.load(Relaxed);
    wynik.bledy = l.bledy.load(Relaxed);
    wynik.anulowano = anuluj.load(Relaxed);
    wynik
}
