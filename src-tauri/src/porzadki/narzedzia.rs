//! Narzedzia Windows z regul (powercfg, DISM, cleanmgr, vssadmin, PowerShell) i oproznianie Kosza.
//!
//! Okno podaje tylko numer reguly: program i argumenty sa wpisane tutaj, z pelna sciezka do
//! System32, wiec z interfejsu nie da sie uruchomic niczego innego. Gdy regula wymaga praw
//! administratora, a Luzik ich nie ma, program startuje przez ShellExecuteExW z "runas"
//! (Windows pyta uzytkownika).

use serde::Serialize;

/// Co robi narzedzie reguly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Akcja {
    /// Program z System32 i jego argumenty.
    Program { program: &'static str, argumenty: Vec<String> },
    /// SHEmptyRecycleBinW na wszystkich dyskach.
    OproznijKosz,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Narzedzie {
    pub id: &'static str,
    pub akcja: Akcja,
    pub admin: bool,
}

/// Litera dysku systemowego ("C:").
fn dysk_systemowy() -> String {
    std::env::var("SystemDrive").ok().filter(|d| d.len() == 2).unwrap_or_else(|| "C:".into())
}

/// Narzedzie dla reguly albo None, gdy regula nie ma narzedzia (OneDrive robi sie w Eksploratorze).
pub fn dla_reguly(id: &str) -> Option<Narzedzie> {
    let program = |id, program, argumenty: &[&str]| Narzedzie {
        id,
        akcja: Akcja::Program { program, argumenty: argumenty.iter().map(|a| a.to_string()).collect() },
        admin: true,
    };
    Some(match id {
        "hibernacja" => program("hibernacja", "powercfg.exe", &["-h", "off"]),
        "winsxs" => program("winsxs", "Dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup"]),
        "windows-old" => program("windows-old", "cleanmgr.exe", &["/AUTOCLEAN"]),
        "instalator-windows" => program("instalator-windows", "cleanmgr.exe", &["/AUTOCLEAN"]),
        "optymalizacja-dostarczania" => program(
            "optymalizacja-dostarczania",
            r"WindowsPowerShell\v1.0\powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", "Delete-DeliveryOptimizationCache -Force"],
        ),
        "punkty-przywracania" => {
            let mut n = program("punkty-przywracania", "vssadmin.exe", &["delete", "shadows"]);
            if let Akcja::Program { argumenty, .. } = &mut n.akcja {
                argumenty.extend([format!("/for={}", dysk_systemowy()), "/oldest".into(), "/quiet".into()]);
            }
            n
        }
        "kosz" => Narzedzie { id: "kosz", akcja: Akcja::OproznijKosz, admin: false },
        _ => return None,
    })
}

#[derive(Serialize, Clone, Debug)]
pub struct WynikNarzedzia {
    pub id: String,
    pub ok: bool,
    /// Kod wyjscia programu (0 = dobrze).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kod: Option<u32>,
    /// Przyrost wolnego miejsca na dysku systemowym (przed i po).
    pub odzyskane: u64,
    pub opis: String,
}

/// Czy Luzik ma prawa administratora.
pub fn jest_admin() -> bool {
    #[cfg(windows)]
    {
        // SAFETY: funkcja bez argumentow.
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(unix)]
    {
        // SAFETY: funkcja bez argumentow.
        unsafe { libc::geteuid() == 0 }
    }
}

/// Wolne miejsce na dysku systemowym.
fn wolne_systemowy() -> u64 {
    let dyski = crate::skan::lista_dyskow();
    let systemowy = if cfg!(windows) { dysk_systemowy().to_uppercase() } else { "/".into() };
    dyski.iter().find(|d| d.litera.to_uppercase() == systemowy).or(dyski.first()).map_or(0, |d| d.wolne)
}

/// Uruchamia narzedzie reguly i czeka na koniec.
pub fn uruchom(id: &str) -> Result<WynikNarzedzia, String> {
    let n = dla_reguly(id).ok_or_else(|| format!("Reguła „{id}” nie ma narzędzia do uruchomienia."))?;
    if !cfg!(windows) {
        return Err("Narzędzia Windows działają tylko na Windows.".into());
    }
    let przed = wolne_systemowy();
    let kod = wykonaj(&n)?;
    let odzyskane = wolne_systemowy().saturating_sub(przed);
    let ok = kod == 0;
    Ok(WynikNarzedzia {
        id: id.into(),
        ok,
        kod: matches!(n.akcja, Akcja::Program { .. }).then_some(kod),
        odzyskane,
        opis: if ok { "Gotowe.".into() } else { format!("Narzędzie skończyło z kodem {kod}.") },
    })
}

#[cfg(not(windows))]
fn wykonaj(_n: &Narzedzie) -> Result<u32, String> {
    Err("Narzędzia Windows działają tylko na Windows.".into())
}

#[cfg(windows)]
fn wykonaj(n: &Narzedzie) -> Result<u32, String> {
    match &n.akcja {
        Akcja::OproznijKosz => win::oproznij_kosz().map(|_| 0),
        Akcja::Program { program, argumenty } => {
            let system32 = std::env::var_os("SystemRoot")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
                .join("System32");
            let sciezka = system32.join(program);
            if n.admin && !jest_admin() {
                win::uruchom_podniesiony(&sciezka, &polacz_argumenty(argumenty), true)
            } else {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                let w = std::process::Command::new(&sciezka)
                    .args(argumenty)
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .map_err(|e| format!("Nie udało się uruchomić {program}: {e}"))?;
                Ok(w.status.code().map_or(1, |k| k as u32))
            }
        }
    }
}

/// Argumenty w jednej linii dla ShellExecute: te ze spacja w cudzyslowie.
pub fn polacz_argumenty(argumenty: &[String]) -> String {
    argumenty
        .iter()
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Uruchamia Luzika jeszcze raz z prawami administratora. Ok = nowy proces ruszyl,
/// wtedy biezacy ma sie zamknac.
pub fn uruchom_jako_admin() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        win::uruchom_podniesiony(&exe, "", false).map(|_| ())
    }
    #[cfg(not(windows))]
    Err("Uruchamianie jako administrator działa tylko na Windows.".into())
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    fn szeroki(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn oproznij_kosz() -> Result<(), String> {
        // SAFETY: null = wszystkie dyski, bez okna.
        let hr = unsafe {
            SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
        };
        // E_UNEXPECTED przychodzi, gdy Kosz byl juz pusty.
        if hr >= 0 || hr as u32 == 0x8000_FFFF {
            Ok(())
        } else {
            Err(format!("Windows nie opróżnił Kosza (kod {:#x}).", hr as u32))
        }
    }

    /// ShellExecuteExW z "runas". `czekaj`: czeka na koniec i oddaje kod wyjscia.
    pub fn uruchom_podniesiony(program: &Path, argumenty: &str, czekaj: bool) -> Result<u32, String> {
        let plik = szeroki(program.as_os_str());
        let arg = szeroki(std::ffi::OsStr::new(argumenty));
        let czasownik = szeroki(std::ffi::OsStr::new("runas"));
        // SAFETY: struktura z liczb i wskaznikow, zera to poprawny stan poczatkowy.
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = czasownik.as_ptr();
        info.lpFile = plik.as_ptr();
        info.lpParameters = arg.as_ptr();
        // Narzedzia bez okna konsoli; sam Luzik normalnie.
        info.nShow = if czekaj { 0 } else { 1 };
        // SAFETY: bufory zyja do konca wywolania.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            // SAFETY: funkcja bez argumentow.
            let blad = unsafe { GetLastError() };
            return Err(if blad == 1223 {
                "Nie zgodzono się na prawa administratora.".into()
            } else {
                format!("Windows nie uruchomił programu (kod {blad}).")
            });
        }
        let mut kod = 0u32;
        if !info.hProcess.is_null() {
            // SAFETY: uchwyt procesu z ShellExecuteExW (SEE_MASK_NOCLOSEPROCESS), zamykany tutaj.
            unsafe {
                if czekaj {
                    WaitForSingleObject(info.hProcess, INFINITE);
                    GetExitCodeProcess(info.hProcess, &mut kod);
                }
                CloseHandle(info.hProcess);
            }
        }
        Ok(kod)
    }
}
