//! Gdzie na tym komputerze leza katalogi, o ktorych mowia reguly.
//!
//! Windows: znane foldery biezacego uzytkownika z SHGetKnownFolderPath (dzialaja tez,
//! gdy Pobrane albo profil sa przeniesione), %TEMP% ze zmiennej srodowiskowej. Z prawami
//! administratora takze pozostali uzytkownicy z katalogu Users, wedlug zwyklego ukladu.
//! Testy i Linux: `sztuczne(korzen)`, gdzie katalog udaje C:\ z tym samym ukladem.

use super::reguly::Baza;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Uzytkownik {
    pub nazwa: String,
    pub profil: PathBuf,
    pub local: PathBuf,
    pub local_low: PathBuf,
    pub roaming: PathBuf,
    pub temp: PathBuf,
    pub pobrane: PathBuf,
    /// Ten, kto uruchomil aplikacje.
    pub biezacy: bool,
}

impl Uzytkownik {
    /// Uzytkownik z samym katalogiem profilu: reszta wedlug domyslnego ukladu Windows.
    pub fn z_profilu(profil: &Path, biezacy: bool) -> Self {
        let local = profil.join("AppData").join("Local");
        Uzytkownik {
            nazwa: profil.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            profil: profil.to_path_buf(),
            local_low: profil.join("AppData").join("LocalLow"),
            roaming: profil.join("AppData").join("Roaming"),
            temp: local.join("Temp"),
            pobrane: profil.join("Downloads"),
            local,
            biezacy,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Srodowisko {
    pub dysk_systemowy: PathBuf,
    pub windows: PathBuf,
    pub program_data: PathBuf,
    pub program_files: PathBuf,
    pub program_files_x86: PathBuf,
    /// Korzenie dyskow, na ktorych szukamy Kosza.
    pub dyski: Vec<PathBuf>,
    /// Biezacy uzytkownik zawsze pierwszy; pozostali tylko z prawami administratora.
    pub uzytkownicy: Vec<Uzytkownik>,
    pub admin: bool,
    /// Sekundy od 1970: od tej chwili liczymy wiek plikow.
    pub teraz: i64,
}

/// Katalogi w Users, ktore nie sa profilami ludzi.
const NIE_PROFILE: &[&str] = &["public", "default", "default user", "all users", "defaultuser0", "defaultapppool"];

pub fn teraz() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Profile w katalogu Users (bez Public, Default i dowiazan), poza pominietym.
fn profile_w(users: &Path, pomin: &Path) -> Vec<PathBuf> {
    let Ok(wpisy) = std::fs::read_dir(users) else { return Vec::new() };
    let mut wynik: Vec<PathBuf> = wpisy
        .flatten()
        .filter(|w| {
            let nazwa = w.file_name().to_string_lossy().to_lowercase();
            let katalog = w.metadata().map(|m| m.is_dir() && !m.file_type().is_symlink()).unwrap_or(false);
            katalog && !NIE_PROFILE.contains(&nazwa.as_str()) && w.path() != pomin
        })
        .map(|w| w.path())
        .collect();
    wynik.sort();
    wynik
}

impl Srodowisko {
    /// Katalog `korzen` udaje C:\ (Users\<nazwa>\AppData\..., Windows, ProgramData, Program Files).
    /// `biezacy`: nazwa uzytkownika uruchamiajacego; None = pierwszy profil w Users.
    pub fn sztuczne(korzen: &Path, biezacy: Option<&str>, admin: bool) -> Self {
        let users = korzen.join("Users");
        let wszystkie = profile_w(&users, Path::new(""));
        let profil_biezacy = match biezacy {
            Some(n) => users.join(n),
            None => wszystkie.first().cloned().unwrap_or_else(|| users.join("Uzytkownik")),
        };
        let mut uzytkownicy = vec![Uzytkownik::z_profilu(&profil_biezacy, true)];
        if admin {
            uzytkownicy.extend(profile_w(&users, &profil_biezacy).iter().map(|p| Uzytkownik::z_profilu(p, false)));
        }
        Srodowisko {
            dysk_systemowy: korzen.to_path_buf(),
            windows: korzen.join("Windows"),
            program_data: korzen.join("ProgramData"),
            program_files: korzen.join("Program Files"),
            program_files_x86: korzen.join("Program Files (x86)"),
            dyski: vec![korzen.to_path_buf()],
            uzytkownicy,
            admin,
            teraz: teraz(),
        }
    }

    /// Prawdziwy komputer z Windows.
    #[cfg(windows)]
    pub fn systemowe() -> Result<Self, String> {
        win::systemowe()
    }

    /// `LUZIK_SZTUCZNE_C=<katalog>` (opcjonalnie `LUZIK_UZYTKOWNIK`, `LUZIK_ADMIN=1`) daje
    /// sztuczne C:\, co pozwala ogladac reguly na Linuksie. Inaczej prawdziwy Windows.
    pub fn z_otoczenia() -> Result<Self, String> {
        if let Some(k) = std::env::var_os("LUZIK_SZTUCZNE_C") {
            let uzytkownik = std::env::var("LUZIK_UZYTKOWNIK").ok();
            let admin = std::env::var("LUZIK_ADMIN").map(|v| v == "1").unwrap_or(false);
            return Ok(Self::sztuczne(Path::new(&k), uzytkownik.as_deref(), admin));
        }
        #[cfg(windows)]
        return Self::systemowe();
        #[cfg(not(windows))]
        Err("Reguły porządków są dla Windows. Na innym systemie ustaw LUZIK_SZTUCZNE_C na katalog udający C:\\.".into())
    }

    /// Katalogi instalacji Steam, ktore istnieja.
    pub fn steam(&self) -> Vec<PathBuf> {
        [self.program_files_x86.join("Steam"), self.program_files.join("Steam")]
            .into_iter()
            .filter(|p| p.join("steamapps").is_dir() || p.join("steam.exe").is_file())
            .collect()
    }

    /// Instalacje Steam i biblioteki gier z ich libraryfolders.vdf, bez powtorek.
    pub fn biblioteki_steam(&self) -> Vec<PathBuf> {
        let mut wynik: Vec<PathBuf> = Vec::new();
        let mut dodaj = |p: PathBuf| {
            let klucz = p.to_string_lossy().to_lowercase();
            if !wynik.iter().any(|q| q.to_string_lossy().to_lowercase() == klucz) {
                wynik.push(p);
            }
        };
        for instalacja in self.steam() {
            dodaj(instalacja.clone());
            if let Ok(vdf) = std::fs::read_to_string(instalacja.join("steamapps").join("libraryfolders.vdf")) {
                for p in sciezki_bibliotek(&vdf) {
                    if p.is_dir() {
                        dodaj(p);
                    }
                }
            }
        }
        wynik
    }

    /// Katalogi, od ktorych liczy sie wzor z dana baza, z nazwa uzytkownika przy profilach.
    pub fn bazy(&self, baza: Baza) -> Vec<(PathBuf, Option<String>)> {
        let systemowa = |p: &PathBuf| vec![(p.clone(), None)];
        let uzytkownikow = |f: fn(&Uzytkownik) -> &PathBuf| {
            self.uzytkownicy.iter().map(|u| (f(u).clone(), Some(u.nazwa.clone()))).collect()
        };
        match baza {
            Baza::DyskSystemowy => systemowa(&self.dysk_systemowy),
            Baza::Windows => systemowa(&self.windows),
            Baza::ProgramData => systemowa(&self.program_data),
            Baza::KazdyDysk => self.dyski.iter().map(|d| (d.clone(), None)).collect(),
            Baza::Steam => self.steam().into_iter().map(|p| (p, None)).collect(),
            Baza::BibliotekaSteam => self.biblioteki_steam().into_iter().map(|p| (p, None)).collect(),
            Baza::Profil => uzytkownikow(|u| &u.profil),
            Baza::Local => uzytkownikow(|u| &u.local),
            Baza::LocalLow => uzytkownikow(|u| &u.local_low),
            Baza::Roaming => uzytkownikow(|u| &u.roaming),
            Baza::Temp => uzytkownikow(|u| &u.temp),
            Baza::Pobrane => uzytkownikow(|u| &u.pobrane),
        }
    }
}

/// Sciezki bibliotek z libraryfolders.vdf: linie `"path"  "D:\\SteamLibrary"`.
pub fn sciezki_bibliotek(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|linia| {
            let linia = linia.trim();
            let reszta = linia.strip_prefix("\"path\"")?.trim();
            let wnetrze = reszta.strip_prefix('"')?.strip_suffix('"')?;
            Some(PathBuf::from(wnetrze.replace("\\\\", "\\")))
        })
        .collect()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::core::GUID;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        IsUserAnAdmin, SHGetKnownFolderPath, FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow,
        FOLDERID_Profile, FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        FOLDERID_RoamingAppData, FOLDERID_Windows,
    };

    fn znany(id: &GUID) -> Option<PathBuf> {
        let mut p: *mut u16 = std::ptr::null_mut();
        // SAFETY: wskaznik wynikowy zwalniamy CoTaskMemFree niezaleznie od wyniku; napis konczy sie zerem.
        unsafe {
            let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p);
            let wynik = if hr >= 0 && !p.is_null() {
                let mut dl = 0usize;
                while *p.add(dl) != 0 {
                    dl += 1;
                }
                Some(PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(p, dl))))
            } else {
                None
            };
            if !p.is_null() {
                CoTaskMemFree(p as *const _);
            }
            wynik
        }
    }

    pub fn systemowe() -> Result<Srodowisko, String> {
        let profil = znany(&FOLDERID_Profile).ok_or("Nie znam katalogu profilu użytkownika.")?;
        let windows = znany(&FOLDERID_Windows).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let dysk_systemowy = std::env::var_os("SystemDrive")
            .map(|d| PathBuf::from(format!("{}\\", d.to_string_lossy())))
            .or_else(|| windows.ancestors().last().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(r"C:\"));
        // SAFETY: funkcja bez argumentow.
        let admin = unsafe { IsUserAnAdmin() } != 0;

        let mut ja = Uzytkownik::z_profilu(&profil, true);
        if let Some(p) = znany(&FOLDERID_LocalAppData) {
            ja.temp = p.join("Temp");
            ja.local = p;
        }
        if let Some(p) = znany(&FOLDERID_LocalAppDataLow) {
            ja.local_low = p;
        }
        if let Some(p) = znany(&FOLDERID_RoamingAppData) {
            ja.roaming = p;
        }
        if let Some(p) = znany(&FOLDERID_Downloads) {
            ja.pobrane = p;
        }
        if let Some(t) = std::env::var_os("TEMP").or_else(|| std::env::var_os("TMP")) {
            ja.temp = PathBuf::from(t);
        }
        if let Ok(n) = std::env::var("USERNAME") {
            ja.nazwa = n;
        }
        let mut uzytkownicy = vec![ja];
        if admin {
            if let Some(users) = profil.parent() {
                uzytkownicy.extend(profile_w(users, &profil).iter().map(|p| Uzytkownik::z_profilu(p, false)));
            }
        }
        let dyski = crate::skan::lista_dyskow()
            .into_iter()
            .filter(|d| d.rodzaj == "staly" || d.rodzaj == "wymienny")
            .map(|d| PathBuf::from(d.sciezka))
            .collect();
        Ok(Srodowisko {
            dysk_systemowy,
            program_data: znany(&FOLDERID_ProgramData).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")),
            program_files: znany(&FOLDERID_ProgramFiles).unwrap_or_else(|| PathBuf::from(r"C:\Program Files")),
            program_files_x86: znany(&FOLDERID_ProgramFilesX86)
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)")),
            windows,
            dyski,
            uzytkownicy,
            admin,
            teraz: teraz(),
        })
    }
}
