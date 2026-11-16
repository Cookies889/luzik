//! Roznice miedzy systemami: co to za wpis, ile ma bajtow i ile zajmuje na dysku.
//! Linux: lstat (st_size, st_blocks, st_dev, st_ino). Windows: katalog czytany hurtem przez
//! GetFileInformationByHandleEx (FileIdExtdDirectoryInfo): rozmiar, miejsce na dysku, atrybuty
//! i numer pliku bez otwierania plikow. Plik otwierany jest tylko, gdy jest skompresowany,
//! rzadki albo jest punktem reparse (GetCompressedFileSizeW).
//!
//! Twarde linki na Windows: liczby nazw nie ma w wyliczaniu katalogu, wiec kazdy plik dostaje
//! klucz (wolumin, numer pliku), a skan liczy go raz. Kosztuje to wpis w zbiorze na plik
//! (ok. 20 B), za to nie wymaga otwierania 1,2 mln plikow po liczbe linkow.

#[allow(unused_imports)]
use super::drzewo::{RODZAJ_DOWIAZANIE, RODZAJ_INNY, RODZAJ_KATALOG, RODZAJ_PLIK};
use std::ffi::OsString;
use std::fs::Metadata;
#[allow(unused_imports)]
use std::io;
use std::path::{Path, PathBuf};

pub struct Info {
    pub rodzaj: u8,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub mtime: i64,
    /// (wolumin, numer pliku), gdy plik ma wiecej niz jedna nazwe (twardy link).
    pub klucz_linku: Option<(u64, u64)>,
    /// Numer urzadzenia: po nim poznajemy punkt montowania (Linux). Windows: zawsze 0.
    pub urzadzenie: u64,
}

/// Dane woluminu, na ktorym lezy korzen skanu.
pub struct Wolumin {
    pub urzadzenie: u64,
    /// Rozmiar klastra w bajtach (Windows); na Linuksie nieuzywany, bo st_blocks juz to liczy.
    #[cfg_attr(unix, allow(dead_code))]
    pub klaster: u64,
    /// Numer seryjny woluminu (Windows): pierwsza czesc klucza twardego linku.
    #[cfg_attr(unix, allow(dead_code))]
    pub seria: u64,
    /// System plikow nadaje trwale numery plikow (NTFS, ReFS): mozna po nich wykryc twarde linki.
    #[cfg_attr(unix, allow(dead_code))]
    pub id_plikow: bool,
}

/// Jeden wpis katalogu: nazwa i dane albo blad z pelna sciezka.
pub type Wpis = Result<(OsString, Info), (PathBuf, io::Error)>;

/// Wpisy katalogu przez `std::fs::read_dir` i metadane kazdego wpisu. Linux zawsze,
/// Windows tylko, gdy system plikow nie zna wyliczania hurtem.
#[cfg_attr(windows, allow(dead_code))]
pub fn wypisz_std(katalog: &Path, w: &Wolumin, f: &mut dyn FnMut(Wpis)) -> io::Result<()> {
    for wpis in std::fs::read_dir(katalog)? {
        let wpis = match wpis {
            Ok(x) => x,
            Err(e) => {
                f(Err((katalog.to_path_buf(), e)));
                continue;
            }
        };
        let nazwa = wpis.file_name();
        match wpis.metadata() {
            Ok(m) => f(Ok((nazwa, info(&m, &|| wpis.path(), w)))),
            Err(e) => {
                let p = wpis.path();
                match info_awaryjne(&p, w) {
                    Some(i) => f(Ok((nazwa, i))),
                    None => f(Err((p, e))),
                }
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    pub fn wolumin(_korzen: &Path, m: Option<&Metadata>) -> Wolumin {
        Wolumin {
            urzadzenie: m.map_or(0, |m| m.dev()),
            klaster: m.map_or(4096, |m| m.blksize().max(512)),
            seria: 0,
            id_plikow: false,
        }
    }

    /// Wpisy katalogu: na Linuksie `read_dir` i lstat kazdego wpisu (bez otwierania plikow).
    pub fn wypisz(katalog: &Path, w: &Wolumin, f: &mut dyn FnMut(Wpis)) -> io::Result<()> {
        wypisz_std(katalog, w, f)
    }

    pub fn info(m: &Metadata, _sciezka: &dyn Fn() -> PathBuf, _w: &Wolumin) -> Info {
        let ft = m.file_type();
        let rodzaj = if ft.is_symlink() {
            RODZAJ_DOWIAZANIE
        } else if ft.is_dir() {
            RODZAJ_KATALOG
        } else if ft.is_file() {
            RODZAJ_PLIK
        } else {
            RODZAJ_INNY
        };
        // Rozmiar logiczny jak w `du --apparent-size`: tylko zwykle pliki i dowiazania,
        // katalog i urzadzenie maja 0. Miejsce na dysku: przydzielone bloki po 512 B,
        // wiec plik rzadki albo skompresowany wychodzi tak, jak naprawde lezy.
        let rozmiar = if rodzaj == RODZAJ_PLIK || rodzaj == RODZAJ_DOWIAZANIE { m.size() } else { 0 };
        let klucz_linku = (rodzaj != RODZAJ_KATALOG && m.nlink() > 1).then(|| (m.dev(), m.ino()));
        Info {
            rodzaj,
            rozmiar,
            na_dysku: m.blocks() * 512,
            mtime: m.mtime(),
            klucz_linku,
            urzadzenie: m.dev(),
        }
    }

    pub fn otwarcia() -> u64 {
        0
    }

    pub fn info_awaryjne(_sciezka: &Path, _w: &Wolumin) -> Option<Info> {
        None
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FindClose, FindFirstFileW, GetCompressedFileSizeW, GetDiskFreeSpaceW,
        GetFileInformationByHandle, GetFileInformationByHandleEx, GetVolumeInformationW, GetVolumePathNameW,
        BY_HANDLE_FILE_INFORMATION, FILE_LIST_DIRECTORY, OPEN_EXISTING, WIN32_FIND_DATAW,
    };

    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
    const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x800;
    /// Bit znacznika reparse "zastepuje nazwe": dowiazanie, junction i podobne (jak `is_symlink` w std).
    const ZNACZNIK_ZASTEPUJE_NAZWE: u32 = 0x2000_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_FILE_SIZE: u32 = 0xFFFF_FFFF;
    /// Znaczniki dowiazan nazw: dowiazanie symboliczne i junction (punkt montowania).
    const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

    /// Sciezka dla funkcji W: UTF-16 z zerem, a dluga (ponad MAX_PATH) z przedrostkiem `\\?\`.
    pub fn szeroka(p: &Path) -> Vec<u16> {
        let surowa: Vec<u16> = p.as_os_str().encode_wide().collect();
        let przedrostek: [u16; 4] = [92, 92, 63, 92]; // \\?\
        let mut v = Vec::with_capacity(surowa.len() + 8);
        let ma_przedrostek = surowa.starts_with(&przedrostek);
        if surowa.len() >= 248 && !ma_przedrostek && p.is_absolute() {
            if surowa.starts_with(&[92, 92]) {
                // \\serwer\zasob -> \\?\UNC\serwer\zasob
                v.extend_from_slice(&przedrostek);
                v.extend("UNC\\".encode_utf16());
                v.extend_from_slice(&surowa[2..]);
            } else {
                v.extend_from_slice(&przedrostek);
                v.extend_from_slice(&surowa);
            }
        } else {
            v.extend_from_slice(&surowa);
        }
        v.push(0);
        v
    }

    fn filetime_na_unix(t: u64) -> i64 {
        (t / 10_000_000) as i64 - 11_644_473_600
    }

    fn do_klastra(bajty: u64, klaster: u64) -> u64 {
        if klaster <= 1 {
            bajty
        } else {
            bajty.div_ceil(klaster) * klaster
        }
    }

    pub fn wolumin(korzen: &Path, _m: Option<&Metadata>) -> Wolumin {
        let sciezka = szeroka(korzen);
        let mut bufor = [0u16; 300];
        let mut klaster = 4096u64;
        let (mut seria, mut id_plikow) = (0u64, false);
        // SAFETY: bufory maja podana dlugosc, sciezka konczy sie zerem.
        unsafe {
            if GetVolumePathNameW(sciezka.as_ptr(), bufor.as_mut_ptr(), bufor.len() as u32) != 0 {
                let mut numer = 0u32;
                let mut system = [0u16; 32];
                let ok = GetVolumeInformationW(
                    bufor.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    &mut numer,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    system.as_mut_ptr(),
                    system.len() as u32,
                );
                if ok != 0 {
                    seria = numer as u64;
                    let dl = system.iter().position(|&c| c == 0).unwrap_or(system.len());
                    let nazwa = String::from_utf16_lossy(&system[..dl]);
                    id_plikow = nazwa.eq_ignore_ascii_case("NTFS") || nazwa.eq_ignore_ascii_case("ReFS");
                }
                let (mut sektory, mut bajty, mut wolne, mut razem) = (0u32, 0u32, 0u32, 0u32);
                if GetDiskFreeSpaceW(bufor.as_ptr(), &mut sektory, &mut bajty, &mut wolne, &mut razem) != 0 {
                    let k = sektory as u64 * bajty as u64;
                    if k > 0 {
                        klaster = k;
                    }
                }
            }
        }
        Wolumin { urzadzenie: 0, klaster, seria, id_plikow }
    }

    /// Rozmiar naprawde zajety na dysku (pliki skompresowane NTFS, rzadkie, CompactOS).
    fn skompresowany(sciezka: &[u16]) -> io::Result<u64> {
        let mut wysokie = 0u32;
        // SAFETY: sciezka konczy sie zerem, `wysokie` zyje do konca wywolania.
        let niskie = unsafe { GetCompressedFileSizeW(sciezka.as_ptr(), &mut wysokie) };
        if niskie == INVALID_FILE_SIZE {
            let e = io::Error::last_os_error();
            if e.raw_os_error().unwrap_or(0) != 0 {
                return Err(e);
            }
        }
        Ok(((wysokie as u64) << 32) | niskie as u64)
    }

    /// Dane wpisu z katalogu, bez otwierania pliku: dziala tez dla hiberfil.sys,
    /// pagefile.sys i swapfile.sys, ktorych system nie pozwala otworzyc.
    fn znajdz(sciezka: &[u16]) -> Option<WIN32_FIND_DATAW> {
        // SAFETY: struktura jest zwyklymi danymi, wiec zera sa poprawna wartoscia; uchwyt zamykamy.
        unsafe {
            let mut dane: WIN32_FIND_DATAW = std::mem::zeroed();
            let uchwyt = FindFirstFileW(sciezka.as_ptr(), &mut dane);
            if uchwyt as isize == -1 {
                return None;
            }
            FindClose(uchwyt);
            Some(dane)
        }
    }

    /// (numer seryjny woluminu, numer pliku), gdy plik ma wiecej niz jedna nazwe.
    fn klucz_twardego_linku(p: &Path) -> io::Result<Option<(u64, u64)>> {
        let plik = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(7)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(p)?;
        // SAFETY: uchwyt jest otwarty do konca bloku, struktura to zwykle dane.
        unsafe {
            let mut dane: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
            if GetFileInformationByHandle(plik.as_raw_handle() as _, &mut dane) == 0 {
                return Err(io::Error::last_os_error());
            }
            if dane.nNumberOfLinks > 1 {
                let numer = ((dane.nFileIndexHigh as u64) << 32) | dane.nFileIndexLow as u64;
                Ok(Some((dane.dwVolumeSerialNumber as u64, numer)))
            } else {
                Ok(None)
            }
        }
    }

    pub fn info(m: &Metadata, sciezka: &dyn Fn() -> PathBuf, w: &Wolumin) -> Info {
        let ft = m.file_type();
        let mtime = filetime_na_unix(m.last_write_time());
        // Dowiazanie symboliczne i junction: liczone osobno, skan do nich nie wchodzi.
        // Inne punkty reparse (OneDrive, CompactOS) to zwykle pliki i katalogi z danymi.
        if ft.is_symlink() {
            return Info { rodzaj: RODZAJ_DOWIAZANIE, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 };
        }
        if ft.is_dir() {
            return Info { rodzaj: RODZAJ_KATALOG, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 };
        }
        let p = sciezka();
        let szer = szeroka(&p);
        let mut rozmiar = m.file_size();
        // Otwarcie odmawia (plik systemowy albo zajety): zostaje bez klucza, liczony zwyczajnie.
        let klucz_linku = klucz_twardego_linku(&p).unwrap_or(None);
        let zajete = match skompresowany(&szer) {
            Ok(n) => n,
            Err(_) => {
                if let Some(d) = znajdz(&szer) {
                    rozmiar = ((d.nFileSizeHigh as u64) << 32) | d.nFileSizeLow as u64;
                }
                rozmiar
            }
        };
        Info { rodzaj: RODZAJ_PLIK, rozmiar, na_dysku: do_klastra(zajete, w.klaster), mtime, klucz_linku, urzadzenie: 0 }
    }

    /// Klasy GetFileInformationByHandleEx: 128-bitowy numer pliku i znacznik reparse
    /// (Windows 8+) albo starszy wariant z 64-bitowym numerem (znacznik w polu EaSize).
    const KLASA_EXTD: i32 = 19; // FileIdExtdDirectoryInfo
    const KLASA_BOTH: i32 = 10; // FileIdBothDirectoryInfo
    const ERROR_NO_MORE_FILES: i32 = 18;
    /// Bufor na jedna porcje wpisow; u64, zeby rekordy (wyrownane do 8) lezaly rowno.
    const BUFOR_SLOW: usize = 64 * 1024 / 8;

    /// Ile plikow trzeba bylo otworzyc po prawdziwe miejsce na dysku (do pomiaru w biegu na Windows).
    static OTWARCIA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub fn otwarcia() -> u64 {
        OTWARCIA.load(std::sync::atomic::Ordering::Relaxed)
    }

    struct Uchwyt(HANDLE);
    impl Drop for Uchwyt {
        fn drop(&mut self) {
            // SAFETY: uchwyt pochodzi z udanego CreateFileW i jest zamykany raz.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn u32_z(b: &[u8], od: usize) -> u32 {
        u32::from_le_bytes(b[od..od + 4].try_into().unwrap())
    }
    fn u64_z(b: &[u8], od: usize) -> u64 {
        u64::from_le_bytes(b[od..od + 8].try_into().unwrap())
    }

    /// Wpisy katalogu hurtem, bez otwierania plikow. System plikow bez tych klas
    /// (FAT, niektore udzialy sieciowe) dostaje zwykla droge `read_dir`.
    pub fn wypisz(katalog: &Path, w: &Wolumin, f: &mut dyn FnMut(Wpis)) -> io::Result<()> {
        for klasa in [KLASA_EXTD, KLASA_BOTH] {
            if hurtem(katalog, w, klasa, f)? {
                return Ok(());
            }
        }
        wypisz_std(katalog, w, f)
    }

    /// Ok(false): system plikow nie zna tej klasy (nic jeszcze nie oddano, mozna probowac dalej).
    fn hurtem(katalog: &Path, w: &Wolumin, klasa: i32, f: &mut dyn FnMut(Wpis)) -> io::Result<bool> {
        let szer = szeroka(katalog);
        // SAFETY: sciezka konczy sie zerem; uchwyt zamyka Drop.
        let h = unsafe {
            CreateFileW(
                szer.as_ptr(),
                FILE_LIST_DIRECTORY,
                7, // FILE_SHARE_READ | WRITE | DELETE
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let h = Uchwyt(h);
        let mut bufor = vec![0u64; BUFOR_SLOW];
        let mut pierwsza = true;
        loop {
            // SAFETY: bufor ma podana dlugosc w bajtach i zyje do konca wywolania.
            let ok = unsafe {
                GetFileInformationByHandleEx(h.0, klasa, bufor.as_mut_ptr().cast(), (bufor.len() * 8) as u32)
            };
            if ok == 0 {
                let e = io::Error::last_os_error();
                return match e.raw_os_error() {
                    Some(ERROR_NO_MORE_FILES) => Ok(true),
                    // Nieznana klasa albo funkcja na tym systemie plikow.
                    Some(1 | 50 | 87 | 124) if pierwsza => Ok(false),
                    _ => Err(e),
                };
            }
            pierwsza = false;
            // SAFETY: widok bajtowy tego samego bufora, dlugosc w bajtach sie zgadza.
            let bajty = unsafe { std::slice::from_raw_parts(bufor.as_ptr().cast::<u8>(), bufor.len() * 8) };
            let mut od = 0usize;
            loop {
                let r = &bajty[od..];
                let nastepny = u32_z(r, 0) as usize;
                let (znacznik, id_lo, id_hi, nazwa_od) = if klasa == KLASA_EXTD {
                    (u32_z(r, 68), u64_z(r, 72), u64_z(r, 80), 88)
                } else {
                    (u32_z(r, 64), u64_z(r, 96), 0, 104)
                };
                let dl = u32_z(r, 60) as usize;
                let nazwa16: Vec<u16> =
                    r[nazwa_od..nazwa_od + dl].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                let kropki = nazwa16 == [46] || nazwa16 == [46, 46];
                if !kropki {
                    let nazwa = OsString::from_wide(&nazwa16);
                    let wpis = Rekord {
                        atrybuty: u32_z(r, 56),
                        znacznik,
                        zapis: u64_z(r, 24),
                        koniec: u64_z(r, 40),
                        przydzial: u64_z(r, 48),
                        id: (id_lo, id_hi),
                    };
                    let info = wpis.info(&|| katalog.join(&nazwa), w);
                    f(Ok((nazwa, info)));
                }
                if nastepny == 0 {
                    break;
                }
                od += nastepny;
            }
        }
    }

    /// Pola jednego wpisu z wyliczania katalogu.
    struct Rekord {
        atrybuty: u32,
        znacznik: u32,
        zapis: u64,
        koniec: u64,
        przydzial: u64,
        id: (u64, u64),
    }

    impl Rekord {
        fn info(&self, sciezka: &dyn Fn() -> PathBuf, w: &Wolumin) -> Info {
            let mtime = filetime_na_unix(self.zapis);
            let reparse = self.atrybuty & FILE_ATTRIBUTE_REPARSE_POINT != 0;
            if reparse && self.znacznik & ZNACZNIK_ZASTEPUJE_NAZWE != 0 {
                return Info { rodzaj: RODZAJ_DOWIAZANIE, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 };
            }
            if self.atrybuty & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return Info { rodzaj: RODZAJ_KATALOG, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 };
            }
            let rozmiar = self.koniec;
            // Zwykly plik: przydzial z katalogu, a maly plik w MFT (przydzial nie jest wielokrotnoscia
            // klastra) liczymy jak dawniej, czyli jako klaster. Skompresowany, rzadki, OneDrive oraz
            // kazdy z przydzialem mniejszym niz rozmiar (CompactOS chowa swoj znacznik, a wpis
            // drugiej nazwy twardego linku bywa nieaktualny): dopiero system zna prawdziwe miejsce,
            // wiec tylko te pliki sa otwierane.
            let zwykle = do_klastra(self.przydzial.max(rozmiar), w.klaster);
            let pytaj = self.atrybuty & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0
                || reparse
                || self.przydzial < rozmiar;
            let na_dysku = if pytaj {
                OTWARCIA.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match skompresowany(&szeroka(&sciezka())) {
                    Ok(n) => do_klastra(n, w.klaster),
                    Err(_) if self.przydzial > 0 => self.przydzial,
                    Err(_) => do_klastra(rozmiar, w.klaster),
                }
            } else {
                zwykle
            };
            Info { rodzaj: RODZAJ_PLIK, rozmiar, na_dysku, mtime, klucz_linku: self.klucz(w), urzadzenie: 0 }
        }

        /// Klucz tego samego ksztaltu co z GetFileInformationByHandle: (numer woluminu, numer pliku).
        fn klucz(&self, w: &Wolumin) -> Option<(u64, u64)> {
            let (lo, hi) = self.id;
            if !w.id_plikow || (lo == 0 && hi == 0) || (lo == u64::MAX && hi == u64::MAX) {
                return None;
            }
            Some(if hi == 0 { (w.seria, lo) } else { (hi, lo) })
        }
    }

    /// Gdy systemu nie da sie zapytac o metadane zwykla droga: sam wpis z katalogu.
    pub fn info_awaryjne(sciezka: &Path, w: &Wolumin) -> Option<Info> {
        let d = znajdz(&szeroka(sciezka))?;
        let mtime = filetime_na_unix(((d.ftLastWriteTime.dwHighDateTime as u64) << 32) | d.ftLastWriteTime.dwLowDateTime as u64);
        let dowiazanie = d.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            && (d.dwReserved0 == IO_REPARSE_TAG_SYMLINK || d.dwReserved0 == IO_REPARSE_TAG_MOUNT_POINT);
        if dowiazanie {
            return Some(Info { rodzaj: RODZAJ_DOWIAZANIE, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 });
        }
        if d.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Some(Info { rodzaj: RODZAJ_KATALOG, rozmiar: 0, na_dysku: 0, mtime, klucz_linku: None, urzadzenie: 0 });
        }
        let rozmiar = ((d.nFileSizeHigh as u64) << 32) | d.nFileSizeLow as u64;
        Some(Info { rodzaj: RODZAJ_PLIK, rozmiar, na_dysku: do_klastra(rozmiar, w.klaster), mtime, klucz_linku: None, urzadzenie: 0 })
    }
}

pub use imp::*;
