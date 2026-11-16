//! Rozwijanie wzorow sciezek i liczenie tego, co regula lapie w danym miejscu.
//! Do dowiazan symbolicznych i junctionow nie wchodzimy nigdy (profil ma np. junction
//! "Application Data" -> Roaming, wiec inaczej liczylibysmy dwa razy albo wyszli poza cel).

use super::reguly::{Cel, Regula};
use crate::skan::platforma::{self, Info, Wolumin};
use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::UNIX_EPOCH;

/// Dopasowanie nazwy do wzoru z `*` i `?`, bez wielkosci liter (jak w Windows).
pub fn pasuje(wzor: &str, nazwa: &str) -> bool {
    let w: Vec<char> = wzor.chars().flat_map(char::to_lowercase).collect();
    let n: Vec<char> = nazwa.chars().flat_map(char::to_lowercase).collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut gwiazdka, mut powrot) = (None::<usize>, 0usize);
    while j < n.len() {
        if i < w.len() && (w[i] == '?' || w[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < w.len() && w[i] == '*' {
            gwiazdka = Some(i);
            powrot = j;
            i += 1;
        } else if let Some(g) = gwiazdka {
            i = g + 1;
            powrot += 1;
            j = powrot;
        } else {
            return false;
        }
    }
    w[i..].iter().all(|&c| c == '*')
}

fn ma_wieloznacznik(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}

/// Istniejace sciezki pasujace do wzoru `a\b*\c` od bazy. Posrednie segmenty musza byc
/// katalogami, ostatni moze byc plikiem. Dowiazania i junctiony pomijamy.
pub fn rozwin(baza: &Path, wzor: &str) -> Vec<PathBuf> {
    let segmenty: Vec<&str> = wzor.split('\\').filter(|s| !s.is_empty()).collect();
    match fs::symlink_metadata(baza) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        _ => return Vec::new(),
    }
    let mut biezace = vec![baza.to_path_buf()];
    for (i, seg) in segmenty.iter().enumerate() {
        let ostatni = i + 1 == segmenty.len();
        let mut nastepne = Vec::new();
        for katalog in &biezace {
            if ma_wieloznacznik(seg) {
                let Ok(wpisy) = fs::read_dir(katalog) else { continue };
                for wpis in wpisy.flatten() {
                    if !pasuje(seg, &wpis.file_name().to_string_lossy()) {
                        continue;
                    }
                    let Ok(m) = wpis.metadata() else { continue };
                    if m.file_type().is_symlink() || (!ostatni && !m.is_dir()) {
                        continue;
                    }
                    nastepne.push(wpis.path());
                }
                continue;
            }
            let kandydat = katalog.join(seg);
            match fs::symlink_metadata(&kandydat) {
                Ok(m) => {
                    if !m.file_type().is_symlink() && (ostatni || m.is_dir()) {
                        nastepne.push(kandydat);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if let Some(p) = bez_wielkosci_liter(katalog, seg, ostatni) {
                        nastepne.push(p);
                    }
                }
                // Plik jest, ale system nie daje metadanych (pagefile.sys zajety): zostaje,
                // pomiar sprobuje wpisu z katalogu.
                Err(_) if ostatni => nastepne.push(kandydat),
                Err(_) => {}
            }
        }
        biezace = nastepne;
    }
    biezace
}

/// Windows nie rozroznia wielkosci liter, Linux tak: na sztucznym C:\ szukamy po nazwie.
#[cfg(unix)]
fn bez_wielkosci_liter(katalog: &Path, nazwa: &str, ostatni: bool) -> Option<PathBuf> {
    let szukana = nazwa.to_lowercase();
    fs::read_dir(katalog).ok()?.flatten().find_map(|wpis| {
        if wpis.file_name().to_string_lossy().to_lowercase() != szukana {
            return None;
        }
        let m = wpis.metadata().ok()?;
        (!m.file_type().is_symlink() && (ostatni || m.is_dir())).then(|| wpis.path())
    })
}

#[cfg(windows)]
fn bez_wielkosci_liter(_katalog: &Path, _nazwa: &str, _ostatni: bool) -> Option<PathBuf> {
    None
}

/// Co liczyc w znalezionym miejscu.
pub struct Filtr<'a> {
    pub maski: &'a [&'a str],
    /// Plik musi byc starszy niz tyle sekund (licza sie zmiana i, na Windows, utworzenie).
    pub min_wiek_s: Option<i64>,
    pub onedrive: bool,
    pub teraz: i64,
}

impl Filtr<'static> {
    pub fn z_reguly(r: &Regula, teraz: i64) -> Self {
        match r.cel {
            Cel::Pliki { maski, starsze_niz_h } => {
                Filtr { maski, min_wiek_s: Some(starsze_niz_h as i64 * 3600), onedrive: false, teraz }
            }
            Cel::OneDriveLokalne => Filtr { maski: &[], min_wiek_s: None, onedrive: true, teraz },
            Cel::Calosc | Cel::BezRozmiaru => Filtr { maski: &[], min_wiek_s: None, onedrive: false, teraz },
        }
    }
}

#[derive(Default, Debug)]
pub struct Miara {
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pliki: u64,
    pub bledy: u64,
}

fn sekundy(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    })
}

/// Najmlodszy z czasow, po ktorych poznajemy, ze plik jest jeszcze w uzyciu. Na Windows
/// liczy sie tez utworzenie (plik skopiowany wczoraj ma stara date zmiany). Na Linuksie
/// nie: tam czas utworzenia to chwila zapisu na dysk, takze w testach.
fn najmlodszy_czas(m: &Metadata) -> Option<i64> {
    let zmiana = sekundy(m.modified());
    #[cfg(windows)]
    {
        let utworzenie = sekundy(m.created());
        return zmiana.max(utworzenie);
    }
    #[cfg(not(windows))]
    zmiana
}

/// Plik OneDrive tylko w chmurze (atrybuty "odwolanie przy dostepie", offline) albo przypiety
/// na stale ("Zawsze zachowuj na tym urzadzeniu"): nie do zwolnienia.
#[cfg(windows)]
fn onedrive_pomin(m: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x0004_0000;
    const PINNED: u32 = 0x0008_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    m.file_attributes() & (OFFLINE | RECALL_ON_OPEN | PINNED | RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(windows))]
fn onedrive_pomin(_m: &Metadata) -> bool {
    false
}

struct Licznik<'a, 'b> {
    filtr: &'a Filtr<'a>,
    wolumin: Wolumin,
    linki: HashSet<(u64, u64)>,
    miara: Miara,
    zbierz: Option<&'b mut Vec<PathBuf>>,
}

impl Licznik<'_, '_> {
    fn pasuje_nazwa(&self, sciezka: &Path) -> bool {
        if self.filtr.maski.is_empty() {
            return true;
        }
        let nazwa = sciezka.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        self.filtr.maski.iter().any(|m| pasuje(m, &nazwa))
    }

    fn za_mlody(&self, czas: Option<i64>) -> bool {
        match (self.filtr.min_wiek_s, czas) {
            (Some(min), Some(t)) => self.filtr.teraz - t < min,
            // Bez czasu nie wiemy, ile ma lat: zostawiamy.
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn dodaj(&mut self, sciezka: &Path, info: Info) {
        if let Some(klucz) = info.klucz_linku {
            if !self.linki.insert(klucz) {
                return;
            }
        }
        self.miara.rozmiar += info.rozmiar;
        self.miara.na_dysku += info.na_dysku;
        self.miara.pliki += 1;
        if let Some(z) = self.zbierz.as_deref_mut() {
            z.push(sciezka.to_path_buf());
        }
    }

    fn plik(&mut self, sciezka: &Path, m: &Metadata) {
        if !m.file_type().is_file() || !self.pasuje_nazwa(sciezka) || self.za_mlody(najmlodszy_czas(m)) {
            return;
        }
        if self.filtr.onedrive && onedrive_pomin(m) {
            return;
        }
        let info = platforma::info(m, &|| sciezka.to_path_buf(), &self.wolumin);
        self.dodaj(sciezka, info);
    }

    /// Wpis bez metadanych zwykla droga (Windows: pagefile.sys, hiberfil.sys): dane z katalogu.
    fn awaryjnie(&mut self, sciezka: &Path) {
        match platforma::info_awaryjne(sciezka, &self.wolumin) {
            Some(info) if info.rodzaj == crate::skan::drzewo::RODZAJ_PLIK => {
                if self.pasuje_nazwa(sciezka) && !self.za_mlody(Some(info.mtime)) && !self.filtr.onedrive {
                    self.dodaj(sciezka, info);
                }
            }
            Some(_) => {}
            None => self.miara.bledy += 1,
        }
    }
}

/// Liczy pliki w miejscu (katalog z poddrzewem albo jeden plik), ktore przechodza filtr.
/// `zbierz`: dopisuje sciezki policzonych plikow.
pub fn zmierz(miejsce: &Path, filtr: &Filtr, anuluj: &AtomicBool, zbierz: Option<&mut Vec<PathBuf>>) -> Miara {
    let meta = fs::symlink_metadata(miejsce);
    let mut l = Licznik {
        filtr,
        wolumin: platforma::wolumin(miejsce, meta.as_ref().ok()),
        linki: HashSet::new(),
        miara: Miara::default(),
        zbierz,
    };
    let meta = match meta {
        Ok(m) => m,
        Err(_) => {
            l.awaryjnie(miejsce);
            return l.miara;
        }
    };
    if meta.file_type().is_symlink() {
        return l.miara;
    }
    if !meta.is_dir() {
        l.plik(miejsce, &meta);
        return l.miara;
    }
    let mut stos = vec![miejsce.to_path_buf()];
    while let Some(katalog) = stos.pop() {
        if anuluj.load(Relaxed) {
            break;
        }
        let wpisy = match fs::read_dir(&katalog) {
            Ok(w) => w,
            Err(_) => {
                l.miara.bledy += 1;
                continue;
            }
        };
        for wpis in wpisy {
            let Ok(wpis) = wpis else {
                l.miara.bledy += 1;
                continue;
            };
            let sciezka = wpis.path();
            match wpis.metadata() {
                Ok(m) if m.file_type().is_symlink() => {}
                Ok(m) if m.is_dir() => stos.push(sciezka),
                Ok(m) => l.plik(&sciezka, &m),
                Err(_) => l.awaryjnie(&sciezka),
            }
        }
    }
    l.miara
}
