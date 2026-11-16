//! Rownolegly przeglad: jedno zadanie rayon na katalog. Dzieci katalogu trafiaja do areny
//! jednym blokiem (pod krotka blokada), wiec leza obok siebie i wystarcza (pierwsze, liczba).

use super::drzewo::*;
use super::platforma::{self, Info, Wolumin};
use super::{Blad, Opcje, Postep, Statystyki, Wynik, LIMIT_LISTY_BLEDOW, ODSTEP_POSTEPU_MS};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CZESCI_LINKOW: usize = 64;

struct Wspolne<'a> {
    budowa: Mutex<Drzewo>,
    /// Klucze plikow z kilkoma nazwami, w czesciach: na Windows klucz ma kazdy plik,
    /// wiec jedna blokada na 1,2 mln wpisow bylaby waskim gardlem.
    linki: Vec<Mutex<HashSet<(u64, u64)>>>,
    bledy: Mutex<Vec<Blad>>,
    biezacy: Mutex<String>,
    pliki: AtomicU64,
    katalogi: AtomicU64,
    bajty: AtomicU64,
    dowiazania: AtomicU64,
    duplikaty: AtomicU64,
    inne_systemy: AtomicU64,
    bledy_razem: AtomicU64,
    anuluj: &'a AtomicBool,
    wolumin: Wolumin,
    przez_systemy: bool,
}

impl Wspolne<'_> {
    /// true, gdy plik o tym kluczu pojawia sie pierwszy raz.
    fn nowy_plik(&self, klucz: (u64, u64)) -> bool {
        let czesc = (klucz.1 ^ (klucz.1 >> 16) ^ klucz.0) as usize % CZESCI_LINKOW;
        self.linki[czesc].lock().unwrap().insert(klucz)
    }

    fn blad(&self, sciezka: &Path, e: &io::Error) {
        self.bledy_razem.fetch_add(1, Relaxed);
        let mut lista = self.bledy.lock().unwrap();
        if lista.len() < LIMIT_LISTY_BLEDOW {
            lista.push(Blad { sciezka: sciezka.to_string_lossy().into_owned(), powod: e.to_string() });
        }
    }

    fn postep(&self, start: Instant, koniec: bool) -> Postep {
        let czas = start.elapsed();
        let pliki = self.pliki.load(Relaxed);
        Postep {
            pliki,
            katalogi: self.katalogi.load(Relaxed),
            bajty: self.bajty.load(Relaxed),
            bledy: self.bledy_razem.load(Relaxed),
            pliki_na_s: (pliki as f64 / czas.as_secs_f64().max(0.001)) as u64,
            czas_ms: czas.as_millis() as u64,
            biezacy: self.biezacy.lock().unwrap().clone(),
            koniec,
        }
    }
}

#[cfg(unix)]
fn bajty_nazwy(n: &std::ffi::OsStr) -> std::borrow::Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(n.as_bytes())
}

#[cfg(not(unix))]
fn bajty_nazwy(n: &std::ffi::OsStr) -> std::borrow::Cow<'_, [u8]> {
    match n.to_string_lossy() {
        std::borrow::Cow::Borrowed(s) => std::borrow::Cow::Borrowed(s.as_bytes()),
        std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s.into_bytes()),
    }
}

fn nowy_wezel(info: &Info, rodzic: u32, nazwa_od: u32, nazwa_dl: usize, flagi: u8) -> Wezel {
    Wezel {
        rozmiar: info.rozmiar,
        na_dysku: info.na_dysku,
        mtime: info.mtime,
        rodzic,
        pierwsze_dziecko: BRAK,
        liczba_dzieci: 0,
        pliki: u32::from(info.rodzaj != RODZAJ_KATALOG),
        nazwa_od,
        nazwa_dl: nazwa_dl.min(u16::MAX as usize) as u16,
        flagi: info.rodzaj | flagi,
    }
}

fn przejdz<'s>(w: &'s Wspolne<'s>, zakres: &rayon::Scope<'s>, sciezka: PathBuf, id: u32) {
    if w.anuluj.load(Relaxed) {
        return;
    }
    if let Ok(mut b) = w.biezacy.try_lock() {
        b.clear();
        b.push_str(&sciezka.to_string_lossy());
    }
    // Dzieci zbierane lokalnie, bez blokady: wezly z nazwami w malym buforze.
    let mut wezly: Vec<Wezel> = Vec::new();
    let mut nazwy: Vec<u8> = Vec::new();
    let mut podkatalogi: Vec<(u32, OsString)> = Vec::new();
    let (mut pliki, mut katalogi, mut bajty) = (0u64, 0u64, 0u64);

    let wynik = platforma::wypisz(&sciezka, &w.wolumin, &mut |wpis| {
        let (nazwa, info) = match wpis {
            Ok(x) => x,
            Err((p, e)) => {
                w.blad(&p, &e);
                return;
            }
        };
        let mut flagi = 0u8;
        match info.rodzaj {
            RODZAJ_KATALOG => {
                katalogi += 1;
                if !w.przez_systemy && info.urzadzenie != w.wolumin.urzadzenie {
                    flagi |= FLAGA_INNY_SYSTEM;
                    w.inne_systemy.fetch_add(1, Relaxed);
                } else {
                    podkatalogi.push((wezly.len() as u32, nazwa.clone()));
                }
            }
            RODZAJ_DOWIAZANIE => {
                pliki += 1;
                w.dowiazania.fetch_add(1, Relaxed);
            }
            _ => pliki += 1,
        }
        if let Some(klucz) = info.klucz_linku {
            if !w.nowy_plik(klucz) {
                flagi |= FLAGA_DUPLIKAT;
                w.duplikaty.fetch_add(1, Relaxed);
            }
        }
        if flagi & FLAGA_DUPLIKAT == 0 {
            bajty += info.rozmiar;
        }
        let n = bajty_nazwy(&nazwa);
        wezly.push(nowy_wezel(&info, id, nazwy.len() as u32, n.len(), flagi));
        nazwy.extend_from_slice(&n[..n.len().min(u16::MAX as usize)]);
    });
    if let Err(e) = wynik {
        w.blad(&sciezka, &e);
        // Katalog, ktorego nie dalo sie otworzyc: oznaczony. Blad w polowie: zostaje to, co juz przeczytane.
        if wezly.is_empty() {
            w.budowa.lock().unwrap().wezly[id as usize].flagi |= FLAGA_NIEDOSTEPNY;
            return;
        }
    }

    w.pliki.fetch_add(pliki, Relaxed);
    w.katalogi.fetch_add(katalogi, Relaxed);
    w.bajty.fetch_add(bajty, Relaxed);
    if wezly.is_empty() {
        return;
    }

    let pierwsze = {
        let mut b = w.budowa.lock().unwrap();
        let pierwsze = b.wezly.len() as u32;
        let przesuniecie = b.nazwy.len() as u32;
        for x in &mut wezly {
            x.nazwa_od += przesuniecie;
        }
        b.nazwy.extend_from_slice(&nazwy);
        let liczba = wezly.len() as u32;
        b.wezly.append(&mut wezly);
        let rodzic = &mut b.wezly[id as usize];
        rodzic.pierwsze_dziecko = pierwsze;
        rodzic.liczba_dzieci = liczba;
        pierwsze
    };

    for (i, nazwa) in podkatalogi {
        let dalej = sciezka.join(nazwa);
        zakres.spawn(move |z| przejdz(w, z, dalej, pierwsze + i));
    }
}

fn liczba_watkow(opcje: &Opcje) -> usize {
    if opcje.watki > 0 {
        return opcje.watki;
    }
    // Zmierzone na /usr (1,4 mln plikow, 12 rdzeni): 12 watkow 0,29 s, 24 watki 0,43 s, 6 watkow 0,37 s.
    std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(4, 32)
}

/// Skanuje `korzen` (katalog albo pojedynczy plik). `postep` dostaje stan co 100 ms
/// i raz na koniec; `anuluj` ustawione na true przerywa skan, wynik jest wtedy czesciowy.
pub fn skanuj(
    korzen: &Path,
    opcje: &Opcje,
    anuluj: &AtomicBool,
    postep: Option<&(dyn Fn(Postep) + Sync)>,
) -> io::Result<Wynik> {
    let start = Instant::now();
    let meta = std::fs::metadata(korzen);
    let wolumin = platforma::wolumin(korzen, meta.as_ref().ok());
    let info = match &meta {
        Ok(m) => platforma::info(m, &|| korzen.to_path_buf(), &wolumin),
        Err(e) => platforma::info_awaryjne(korzen, &wolumin)
            .ok_or_else(|| io::Error::new(e.kind(), format!("{}: {e}", korzen.display())))?,
    };

    let mut drzewo = Drzewo::default();
    let nazwa = korzen.to_string_lossy().into_owned().into_bytes();
    let mut pierwszy = nowy_wezel(&info, BRAK, 0, nazwa.len(), 0);
    // Korzen bedacy plikiem z kilkoma nazwami liczymy normalnie: nie ma z czym go zdublowac.
    pierwszy.flagi &= !FLAGA_DUPLIKAT;
    drzewo.nazwy.extend_from_slice(&nazwa[..nazwa.len().min(u16::MAX as usize)]);
    drzewo.wezly.push(pierwszy);

    let katalog = info.rodzaj == RODZAJ_KATALOG;
    let w = Wspolne {
        budowa: Mutex::new(drzewo),
        linki: (0..CZESCI_LINKOW).map(|_| Mutex::new(HashSet::new())).collect(),
        bledy: Mutex::new(Vec::new()),
        biezacy: Mutex::new(korzen.to_string_lossy().into_owned()),
        pliki: AtomicU64::new(u64::from(!katalog)),
        katalogi: AtomicU64::new(u64::from(katalog)),
        bajty: AtomicU64::new(info.rozmiar),
        dowiazania: AtomicU64::new(0),
        duplikaty: AtomicU64::new(0),
        inne_systemy: AtomicU64::new(0),
        bledy_razem: AtomicU64::new(0),
        anuluj,
        wolumin,
        przez_systemy: opcje.przez_systemy,
    };

    if katalog {
        let pula = rayon::ThreadPoolBuilder::new()
            .num_threads(liczba_watkow(opcje))
            .thread_name(|i| format!("luzik-skan-{i}"))
            .build()
            .map_err(io::Error::other)?;
        let koniec = AtomicBool::new(false);
        std::thread::scope(|watki| {
            let zegar = postep.map(|zglos| {
                let (w, koniec) = (&w, &koniec);
                watki.spawn(move || {
                    while !koniec.load(Relaxed) {
                        std::thread::park_timeout(Duration::from_millis(ODSTEP_POSTEPU_MS));
                        if !koniec.load(Relaxed) {
                            zglos(w.postep(start, false));
                        }
                    }
                })
            });
            let w = &w;
            pula.scope(|z| przejdz(w, z, korzen.to_path_buf(), 0));
            koniec.store(true, Relaxed);
            if let Some(z) = zegar {
                z.thread().unpark();
            }
        });
    }

    if let Some(zglos) = postep {
        zglos(w.postep(start, true));
    }

    if std::env::var_os("LUZIK_DIAG").is_some() {
        eprintln!(
            "LUZIK_DIAG watki {} otwarte pliki {} przeglad {} ms",
            liczba_watkow(opcje),
            platforma::otwarcia(),
            start.elapsed().as_millis()
        );
    }
    let mut drzewo = w.budowa.into_inner().unwrap();
    drzewo.zsumuj();
    let statystyki = Statystyki {
        pliki: w.pliki.load(Relaxed),
        katalogi: w.katalogi.load(Relaxed),
        dowiazania: w.dowiazania.load(Relaxed),
        twarde_linki_pominiete: w.duplikaty.load(Relaxed),
        inne_systemy: w.inne_systemy.load(Relaxed),
        bledy_razem: w.bledy_razem.load(Relaxed),
        bledy: w.bledy.into_inner().unwrap(),
        czas_ms: start.elapsed().as_millis() as u64,
        anulowano: anuluj.load(Relaxed),
    };
    Ok(Wynik { drzewo, statystyki })
}
