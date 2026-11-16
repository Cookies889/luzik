//! Usuwanie plikow i folderow: do Kosza albo na stale.
//!
//! - kazda sciezka najpierw przechodzi `Ochrona::sprawdz` (twarda blokada, patrz `ochrona.rs`),
//! - plik otwarty w innym programie albo bez uprawnien jest pomijany z powodem, reszta idzie dalej,
//! - folder na stale: pliki po kolei, potem puste foldery od najglebszych, wiec zablokowany plik
//!   zostawia tylko swoja galaz,
//! - Kosz na Windows: SHFileOperationW z FOF_ALLOWUNDO (paczkami, a gdy paczka sie nie uda, po jednym),
//!   na Linuksie katalog Kosza wedlug freedesktop.org (files/ i info/),
//! - wynik mowi, ile miejsca naprawde zwolniono i co pominieto; `usuniete` sluzy do poprawienia
//!   drzewa skanu bez ponownego skanu.

use super::ochrona::Ochrona;
use crate::skan::platforma;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Tryb {
    Kosz,
    Trwale,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Pominiety {
    pub sciezka: String,
    pub powod: String,
}

/// Ile pominietych sciezek wraca do okna (licznik liczy wszystkie).
pub const POMINIETE_W_WYNIKU: usize = 500;

#[derive(Serialize, Clone, Debug)]
pub struct WynikUsuwania {
    pub tryb: Tryb,
    /// Sciezki z prosby usuniete w calosci.
    pub pozycje: u64,
    pub pliki: u64,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pominiete: Vec<Pominiety>,
    pub pominiete_razem: u64,
    /// Korzen skanu po zmianach (gdy usuniete sciezki lezaly w skanie).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub korzen: Option<crate::skan::Wiersz>,
    /// Wszystko, co zniknelo z dysku (pliki i foldery), do poprawienia drzewa skanu.
    #[serde(skip)]
    pub usuniete: Vec<String>,
}

impl WynikUsuwania {
    pub fn nowy(tryb: Tryb) -> Self {
        WynikUsuwania {
            tryb,
            pozycje: 0,
            pliki: 0,
            rozmiar: 0,
            na_dysku: 0,
            pominiete: Vec::new(),
            pominiete_razem: 0,
            korzen: None,
            usuniete: Vec::new(),
        }
    }

    fn pomin(&mut self, sciezka: &Path, powod: impl Into<String>) {
        self.pominiete_razem += 1;
        if self.pominiete.len() < POMINIETE_W_WYNIKU {
            self.pominiete.push(Pominiety { sciezka: sciezka.to_string_lossy().into_owned(), powod: powod.into() });
        }
    }

    pub fn dolacz(&mut self, inny: WynikUsuwania) {
        self.pozycje += inny.pozycje;
        self.pliki += inny.pliki;
        self.rozmiar += inny.rozmiar;
        self.na_dysku += inny.na_dysku;
        self.pominiete_razem += inny.pominiete_razem;
        for p in inny.pominiete {
            if self.pominiete.len() < POMINIETE_W_WYNIKU {
                self.pominiete.push(p);
            }
        }
        self.usuniete.extend(inny.usuniete);
    }
}

/// Dokad trafia plik w trybie Kosz.
#[derive(Clone, Debug)]
pub enum Kosz {
    /// Kosz systemu (Windows: Kosz Eksploratora, Linux: ~/.local/share/Trash).
    Systemowy,
    /// Katalog w ukladzie freedesktop.org (files/ i info/): testy.
    Katalog(PathBuf),
}

pub struct Usuwacz<'a> {
    pub ochrona: &'a Ochrona,
    pub kosz: Kosz,
}

/// Wpis do usuniecia z miara zrobiona przed usunieciem.
struct Wpis {
    sciezka: PathBuf,
    katalog: bool,
    dowiazanie: bool,
    rozmiar: u64,
    na_dysku: u64,
}

/// Powod po polsku dla bledu systemu plikow.
pub fn powod(e: &io::Error) -> String {
    #[cfg(windows)]
    match e.raw_os_error() {
        Some(32) | Some(33) => return "Plik jest otwarty w innym programie.".into(),
        Some(5) => return "Brak uprawnień. Uruchom Luzika jako administrator.".into(),
        Some(145) => return "Folder nie jest pusty.".into(),
        _ => {}
    }
    #[cfg(unix)]
    match e.raw_os_error() {
        Some(libc::EBUSY) | Some(libc::ETXTBSY) => return "Plik jest używany przez inny program.".into(),
        Some(libc::EXDEV) => return "Kosz jest na innym dysku niż ten plik.".into(),
        _ => {}
    }
    match e.kind() {
        io::ErrorKind::NotFound => "Już nie istnieje.".into(),
        io::ErrorKind::PermissionDenied => "Brak uprawnień. Uruchom Luzika jako administrator.".into(),
        _ => format!("Błąd systemu: {e}"),
    }
}

/// Wpis z miara (bez wchodzenia w dowiazania).
fn wpis(sciezka: &Path, m: &fs::Metadata) -> Wpis {
    let w = platforma::wolumin(sciezka, Some(m));
    let info = platforma::info(m, &|| sciezka.to_path_buf(), &w);
    let dowiazanie = m.file_type().is_symlink();
    Wpis {
        sciezka: sciezka.to_path_buf(),
        katalog: m.is_dir() && !dowiazanie,
        dowiazanie,
        rozmiar: if m.is_dir() { 0 } else { info.rozmiar },
        na_dysku: if m.is_dir() { 0 } else { info.na_dysku },
    }
}

/// Folder z cala zawartoscia, od najglebszych: najpierw pliki folderu, potem sam folder.
/// Nieczytelne podfoldery trafiaja do `bledy`.
fn rozwin_folder(korzen: &Path, bledy: &mut Vec<(PathBuf, io::Error)>) -> Vec<Wpis> {
    let mut wynik = Vec::new();
    fn krok(k: &Path, wynik: &mut Vec<Wpis>, bledy: &mut Vec<(PathBuf, io::Error)>) {
        match fs::read_dir(k) {
            Ok(wpisy) => {
                for e in wpisy.flatten() {
                    let p = e.path();
                    match fs::symlink_metadata(&p) {
                        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => krok(&p, wynik, bledy),
                        Ok(m) => wynik.push(wpis(&p, &m)),
                        Err(err) => bledy.push((p, err)),
                    }
                }
            }
            Err(err) => bledy.push((k.to_path_buf(), err)),
        }
        if let Ok(m) = fs::symlink_metadata(k) {
            wynik.push(wpis(k, &m));
        }
    }
    krok(korzen, &mut wynik, bledy);
    wynik
}

fn usun_plik(w: &Wpis) -> io::Result<()> {
    let wynik = if w.dowiazanie && cfg!(windows) && w.sciezka.is_dir() {
        // Dowiazanie do folderu i junction na Windows usuwa sie jak folder (sam link, bez celu).
        fs::remove_dir(&w.sciezka)
    } else {
        fs::remove_file(&w.sciezka)
    };
    #[cfg(windows)]
    if let Err(e) = &wynik {
        // Plik tylko do odczytu: zdejmujemy atrybut i probujemy jeszcze raz.
        if e.kind() == io::ErrorKind::PermissionDenied {
            if let Ok(m) = fs::symlink_metadata(&w.sciezka) {
                let mut p = m.permissions();
                if p.readonly() {
                    #[allow(clippy::permissions_set_readonly_false)]
                    p.set_readonly(false);
                    if fs::set_permissions(&w.sciezka, p).is_ok() {
                        return fs::remove_file(&w.sciezka);
                    }
                }
            }
        }
    }
    wynik
}

impl Usuwacz<'_> {
    /// Usuwa podane sciezki. Nigdy nie przerywa na bledzie: pomija i jedzie dalej.
    pub fn usun(&self, sciezki: &[PathBuf], tryb: Tryb) -> WynikUsuwania {
        let mut wynik = WynikUsuwania::nowy(tryb);
        let mut do_kosza: Vec<(Wpis, u64)> = Vec::new();
        for sciezka in sciezki {
            let m = match fs::symlink_metadata(sciezka) {
                Ok(m) => m,
                Err(e) => {
                    wynik.pomin(sciezka, powod(&e));
                    continue;
                }
            };
            let glowny = wpis(sciezka, &m);
            if let Err(p) = self.ochrona.sprawdz(&sciezka.to_string_lossy(), glowny.katalog) {
                wynik.pomin(sciezka, p);
                continue;
            }
            match (tryb, glowny.katalog) {
                (Tryb::Trwale, false) => match usun_plik(&glowny) {
                    Ok(()) => {
                        wynik.pozycje += 1;
                        wynik.pliki += 1;
                        wynik.rozmiar += glowny.rozmiar;
                        wynik.na_dysku += glowny.na_dysku;
                        wynik.usuniete.push(sciezka.to_string_lossy().into_owned());
                    }
                    Err(e) => wynik.pomin(sciezka, powod(&e)),
                },
                (Tryb::Trwale, true) => self.usun_folder(sciezka, &mut wynik),
                (Tryb::Kosz, katalog) => {
                    // Miara przed przeniesieniem: po nim pliki leza juz gdzie indziej.
                    let (mut rozmiar, mut na_dysku, mut pliki) = (glowny.rozmiar, glowny.na_dysku, 1u64);
                    if katalog {
                        let mut bledy = Vec::new();
                        let zawartosc = rozwin_folder(sciezka, &mut bledy);
                        rozmiar = zawartosc.iter().map(|w| w.rozmiar).sum();
                        na_dysku = zawartosc.iter().map(|w| w.na_dysku).sum();
                        pliki = zawartosc.iter().filter(|w| !w.katalog).count() as u64;
                    }
                    do_kosza.push((Wpis { rozmiar, na_dysku, ..glowny }, pliki));
                }
            }
        }
        if !do_kosza.is_empty() {
            self.przenies_do_kosza(do_kosza, &mut wynik);
        }
        wynik
    }

    fn usun_folder(&self, sciezka: &Path, wynik: &mut WynikUsuwania) {
        let mut bledy = Vec::new();
        let zawartosc = rozwin_folder(sciezka, &mut bledy);
        for (p, e) in &bledy {
            wynik.pomin(p, powod(e));
        }
        let mut calosc = bledy.is_empty();
        for w in &zawartosc {
            let r = if w.katalog { fs::remove_dir(&w.sciezka) } else { usun_plik(w) };
            match r {
                Ok(()) => {
                    if !w.katalog {
                        wynik.pliki += 1;
                        wynik.rozmiar += w.rozmiar;
                        wynik.na_dysku += w.na_dysku;
                    }
                    wynik.usuniete.push(w.sciezka.to_string_lossy().into_owned());
                }
                // Folder z pominietym plikiem zostaje: powodem jest ten plik, juz zapisany.
                Err(_) if w.katalog && !calosc => {}
                Err(e) => {
                    calosc = false;
                    wynik.pomin(&w.sciezka, powod(&e));
                }
            }
        }
        if calosc {
            wynik.pozycje += 1;
        }
    }

    fn przenies_do_kosza(&self, wpisy: Vec<(Wpis, u64)>, wynik: &mut WynikUsuwania) {
        let zalicz = |(w, pliki): &(Wpis, u64), wynik: &mut WynikUsuwania| {
            wynik.pozycje += 1;
            wynik.pliki += pliki;
            wynik.rozmiar += w.rozmiar;
            wynik.na_dysku += w.na_dysku;
            wynik.usuniete.push(w.sciezka.to_string_lossy().into_owned());
        };
        match &self.kosz {
            Kosz::Katalog(k) => {
                for w in &wpisy {
                    match do_katalogu_kosza(k, &w.0.sciezka) {
                        Ok(()) => zalicz(w, wynik),
                        Err(e) => wynik.pomin(&w.0.sciezka, powod(&e)),
                    }
                }
            }
            Kosz::Systemowy => {
                #[cfg(windows)]
                for paczka in wpisy.chunks(200) {
                    let sciezki: Vec<&Path> = paczka.iter().map(|w| w.0.sciezka.as_path()).collect();
                    if win::do_kosza(&sciezki).is_ok() && sciezki.iter().all(|p| fs::symlink_metadata(p).is_err()) {
                        paczka.iter().for_each(|w| zalicz(w, wynik));
                        continue;
                    }
                    // Paczka sie nie udala: po jednym, zeby wiedziec, ktory plik jest zablokowany.
                    for w in paczka {
                        if fs::symlink_metadata(&w.0.sciezka).is_err() {
                            zalicz(w, wynik);
                            continue;
                        }
                        match win::do_kosza(&[&w.0.sciezka]) {
                            Ok(()) if fs::symlink_metadata(&w.0.sciezka).is_err() => zalicz(w, wynik),
                            Ok(()) => wynik.pomin(&w.0.sciezka, "Część plików jest otwarta w innym programie."),
                            Err(p) => wynik.pomin(&w.0.sciezka, p),
                        }
                    }
                }
                #[cfg(not(windows))]
                {
                    let k = kosz_freedesktop();
                    for w in &wpisy {
                        match do_katalogu_kosza(&k, &w.0.sciezka) {
                            Ok(()) => zalicz(w, wynik),
                            Err(e) => wynik.pomin(&w.0.sciezka, powod(&e)),
                        }
                    }
                }
            }
        }
    }
}

/// Usuwa puste foldery wewnatrz `miejsce` (samego miejsca nie), od najglebszych.
/// Dla regul, ktore czyszcza caly folder: po plikach zostaja puste podfoldery.
pub fn usun_puste_podfoldery(miejsce: &Path, ochrona: &Ochrona, wynik: &mut WynikUsuwania) {
    let Ok(wpisy) = fs::read_dir(miejsce) else { return };
    for e in wpisy.flatten() {
        let p = e.path();
        let Ok(m) = fs::symlink_metadata(&p) else { continue };
        if !m.is_dir() || m.file_type().is_symlink() {
            continue;
        }
        usun_puste_podfoldery(&p, ochrona, wynik);
        if ochrona.sprawdz(&p.to_string_lossy(), true).is_ok() && fs::remove_dir(&p).is_ok() {
            wynik.usuniete.push(p.to_string_lossy().into_owned());
        }
    }
}

#[cfg(not(windows))]
fn kosz_freedesktop() -> PathBuf {
    if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("Trash");
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/Trash")
}

/// Data UTC w formacie RRRR-MM-DDTgg:mm:ss (do .trashinfo).
fn data_iso(sekundy: i64) -> String {
    let dni = sekundy.div_euclid(86_400);
    let s = sekundy.rem_euclid(86_400);
    // Algorytm "civil from days" (Howard Hinnant).
    let z = dni + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let r = yoe + era * 400 + i64::from(m <= 2);
    format!("{r:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

/// Przenosi wpis do Kosza w ukladzie freedesktop.org: files/<nazwa> i info/<nazwa>.trashinfo.
fn do_katalogu_kosza(kosz: &Path, sciezka: &Path) -> io::Result<()> {
    let (pliki, info) = (kosz.join("files"), kosz.join("info"));
    fs::create_dir_all(&pliki)?;
    fs::create_dir_all(&info)?;
    let nazwa = sciezka.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "bez-nazwy".into());
    let mut cel = nazwa.clone();
    let mut n = 1;
    while fs::symlink_metadata(pliki.join(&cel)).is_ok() || info.join(format!("{cel}.trashinfo")).exists() {
        n += 1;
        cel = format!("{nazwa}.{n}");
    }
    let plik_info = info.join(format!("{cel}.trashinfo"));
    let bezwzgledna = std::path::absolute(sciezka).unwrap_or_else(|_| sciezka.to_path_buf());
    fs::write(
        &plik_info,
        format!("[Trash Info]\nPath={}\nDeletionDate={}\n", bezwzgledna.display(), data_iso(super::srodowisko::teraz())),
    )?;
    fs::rename(sciezka, pliki.join(&cel)).inspect_err(|_| {
        let _ = fs::remove_file(&plik_info);
    })
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };

    /// Przenosi sciezki do Kosza jedna operacja powloki (z mozliwoscia cofniecia).
    pub fn do_kosza(sciezki: &[&Path]) -> Result<(), String> {
        // pFrom: sciezki rozdzielone zerem, na koncu dwa zera.
        let mut z: Vec<u16> = Vec::new();
        for s in sciezki {
            z.extend(s.as_os_str().encode_wide());
            z.push(0);
        }
        z.push(0);
        // SAFETY: struktura z samych liczb i wskaznikow, zera to poprawny stan poczatkowy.
        let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        op.wFunc = FO_DELETE;
        op.pFrom = z.as_ptr();
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16;
        // SAFETY: `op` i bufor `z` zyja do konca wywolania.
        let r = unsafe { SHFileOperationW(&mut op) };
        if op.fAnyOperationsAborted != 0 {
            return Err("Przerwane.".into());
        }
        match r {
            0 => Ok(()),
            32 | 33 => Err("Plik jest otwarty w innym programie.".into()),
            5 | 0x78 => Err("Brak uprawnień. Uruchom Luzika jako administrator.".into()),
            kod => Err(format!("Windows nie przeniósł tego do Kosza (kod {kod:#x}).")),
        }
    }
}

// --- historia porzadkow ---

/// Jeden wpis historii: usuwanie albo narzedzie Windows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WpisHistorii {
    /// Sekundy od 1970.
    pub czas: i64,
    /// "usuwanie" albo "narzedzie".
    pub rodzaj: String,
    pub opis: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tryb: Option<Tryb>,
    pub pozycje: u64,
    pub pliki: u64,
    pub na_dysku: u64,
    pub pominiete: u64,
}

/// Najwyzej tyle wpisow w pliku (najstarsze odpadaja).
pub const LIMIT_HISTORII: usize = 500;

pub fn wczytaj_historie(plik: &Path) -> Vec<WpisHistorii> {
    fs::read(plik).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Dopisuje wpis na koniec. Zapis przez plik tymczasowy i zmiane nazwy, wiec przerwany zapis
/// nie psuje historii.
pub fn dopisz_historie(plik: &Path, wpis: WpisHistorii) -> io::Result<()> {
    let mut h = wczytaj_historie(plik);
    h.push(wpis);
    if h.len() > LIMIT_HISTORII {
        h.drain(..h.len() - LIMIT_HISTORII);
    }
    if let Some(k) = plik.parent() {
        fs::create_dir_all(k)?;
    }
    let tymczasowy = plik.with_extension("json.tmp");
    fs::write(&tymczasowy, serde_json::to_vec_pretty(&h).map_err(io::Error::other)?)?;
    fs::rename(&tymczasowy, plik)
}
