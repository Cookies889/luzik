//! Roznice miedzy systemami: co to za wpis, ile ma bajtow i ile zajmuje na dysku.
//! Linux: lstat (st_size, st_blocks, st_dev, st_ino). Windows: dane z wyliczania katalogu,
//! GetCompressedFileSizeW, rozmiar klastra i numer pliku z GetFileInformationByHandle.

#[allow(unused_imports)]
use super::drzewo::{RODZAJ_DOWIAZANIE, RODZAJ_INNY, RODZAJ_KATALOG, RODZAJ_PLIK};
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
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    pub fn wolumin(_korzen: &Path, m: Option<&Metadata>) -> Wolumin {
        Wolumin { urzadzenie: m.map_or(0, |m| m.dev()), klaster: m.map_or(4096, |m| m.blksize().max(512)) }
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
    use windows_sys::Win32::Storage::FileSystem::{
        FindClose, FindFirstFileW, GetCompressedFileSizeW, GetDiskFreeSpaceW, GetFileInformationByHandle,
        GetVolumePathNameW, BY_HANDLE_FILE_INFORMATION, WIN32_FIND_DATAW,
    };

    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
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
        // SAFETY: bufory maja podana dlugosc, sciezka konczy sie zerem.
        unsafe {
            if GetVolumePathNameW(sciezka.as_ptr(), bufor.as_mut_ptr(), bufor.len() as u32) != 0 {
                let (mut sektory, mut bajty, mut wolne, mut razem) = (0u32, 0u32, 0u32, 0u32);
                if GetDiskFreeSpaceW(bufor.as_ptr(), &mut sektory, &mut bajty, &mut wolne, &mut razem) != 0 {
                    let k = sektory as u64 * bajty as u64;
                    if k > 0 {
                        klaster = k;
                    }
                }
            }
        }
        Wolumin { urzadzenie: 0, klaster }
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
