//! Katalog regul porzadkow dla Windows 11. Kazda regula mowi, gdzie szukac (wzory sciezek
//! od znanych folderow), co w tym miejscu liczyc, jak bardzo jest to bezpieczne i jak to usunac.
//!
//! Wzor sciezki: segmenty oddzielone `\`, segment moze miec `*` i `?` (wielkosc liter bez
//! znaczenia). Pusty wzor to sama baza. Przegladarki maja wzory konczace sie na katalogu
//! pamieci podrecznej (Cache, Code Cache, GPUCache, cache2), wiec profil z haslami,
//! historia i zakladkami nigdy nie jest celem.

use serde::Serialize;

/// Jak bardzo bezpieczne jest usuniecie. Kolejnosc wariantow = kolejnosc na liscie.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Poziom {
    /// Mozna usunac bez pytania: system albo program odtworzy to sam.
    Bezpieczne,
    /// Zwykle mozna, ale cos sie traci (cofniecie aktualizacji, pobrane piosenki): niech zdecyduje czlowiek.
    Sprawdz,
    /// Tylko wyjasnienie, nie usuwac.
    NieRuszaj,
}

impl Poziom {
    pub fn etykieta(self) -> &'static str {
        match self {
            Poziom::Bezpieczne => "bezpieczne",
            Poziom::Sprawdz => "sprawdź",
            Poziom::NieRuszaj => "nie ruszaj",
        }
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Sposob {
    /// Do Kosza: da sie jeszcze odzyskac.
    Kosz,
    /// Od razu na stale (pamieci podreczne, pliki tymczasowe, sam Kosz).
    Trwale,
    /// Narzedziem Windows z pola `polecenie` (DISM, powercfg, vssadmin, cleanmgr).
    Narzedzie,
    /// Nie usuwac.
    Brak,
}

impl Sposob {
    pub fn etykieta(self) -> &'static str {
        match self {
            Sposob::Kosz => "do Kosza",
            Sposob::Trwale => "trwale",
            Sposob::Narzedzie => "narzędziem Windows",
            Sposob::Brak => "nie usuwać",
        }
    }
}

/// Od czego liczy sie wzor sciezki.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Baza {
    /// Dysk systemowy, zwykle C:\.
    DyskSystemowy,
    /// C:\Windows
    Windows,
    /// C:\ProgramData
    ProgramData,
    /// Korzen kazdego dysku stalego i wymiennego (Kosz).
    KazdyDysk,
    /// Katalog instalacji Steam (Program Files i Program Files (x86)).
    Steam,
    /// Instalacja Steam i kazda biblioteka gier z steamapps\libraryfolders.vdf.
    BibliotekaSteam,
    /// Profil uzytkownika, C:\Users\<nazwa>.
    Profil,
    /// AppData\Local
    Local,
    /// AppData\LocalLow
    LocalLow,
    /// AppData\Roaming
    Roaming,
    /// %TEMP% (dla biezacego uzytkownika ze zmiennej, dla innych AppData\Local\Temp).
    Temp,
    /// Folder Pobrane.
    Pobrane,
}

#[derive(Clone, Copy, Debug)]
pub struct Wzor {
    pub baza: Baza,
    pub sciezka: &'static str,
}

/// Co regula liczy w znalezionym miejscu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cel {
    /// Cale miejsce: katalog ze wszystkim w srodku albo pojedynczy plik.
    Calosc,
    /// Tylko pliki pasujace do masek (w calym poddrzewie) i starsze niz podana liczba godzin.
    Pliki { maski: &'static [&'static str], starsze_niz_h: u64 },
    /// Tylko wskazanie miejsca, bez liczenia (WinSxS, punkty przywracania): rozmiar zna narzedzie systemowe.
    BezRozmiaru,
    /// Pliki OneDrive z kopia na dysku (bez tych tylko w chmurze i bez przypietych na stale).
    OneDriveLokalne,
}

#[derive(Debug)]
pub struct Regula {
    pub id: &'static str,
    pub nazwa: &'static str,
    /// Jedno zdanie dla laika: co to jest i co sie stanie po usunieciu.
    pub wyjasnienie: &'static str,
    pub poziom: Poziom,
    pub sposob: Sposob,
    /// Polecenie albo droga w Ustawieniach, gdy sposob to `Narzedzie`.
    pub polecenie: Option<&'static str>,
    pub wymaga_admina: bool,
    pub miejsca: &'static [Wzor],
    pub cel: Cel,
}

const fn w(baza: Baza, sciezka: &'static str) -> Wzor {
    Wzor { baza, sciezka }
}

use Baza::*;

pub const KATALOG: &[Regula] = &[
    Regula {
        id: "temp-uzytkownika",
        nazwa: "Pliki tymczasowe (%TEMP%)",
        wyjasnienie: "Pliki, które programy odkładają na chwilę i zapominają posprzątać; usuwamy tylko te starsze niż doba, więc nic, co teraz działa, nie ucierpi.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(Temp, "")],
        cel: Cel::Pliki { maski: &["*"], starsze_niz_h: 24 },
    },
    Regula {
        id: "temp-windows",
        nazwa: "Pliki tymczasowe Windows (C:\\Windows\\Temp)",
        wyjasnienie: "Tymczasowe pliki samego systemu i instalatorów; te starsze niż doba nie są już do niczego potrzebne.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: true,
        miejsca: &[w(Windows, "Temp")],
        cel: Cel::Pliki { maski: &["*"], starsze_niz_h: 24 },
    },
    Regula {
        id: "kosz",
        nazwa: "Kosz",
        wyjasnienie: "Pliki, które już wyrzuciłeś; po opróżnieniu Kosza nie da się ich przywrócić, więc najpierw tam zajrzyj.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(KazdyDysk, "$Recycle.Bin")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "aktualizacje-windows",
        nazwa: "Pobrane aktualizacje Windows",
        wyjasnienie: "Paczki aktualizacji, które Windows już zainstalował; gdyby były jeszcze potrzebne, system pobierze je ponownie.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: true,
        miejsca: &[w(Windows, "SoftwareDistribution\\Download")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "optymalizacja-dostarczania",
        nazwa: "Pamięć optymalizacji dostarczania",
        wyjasnienie: "Kawałki aktualizacji, którymi Windows dzieli się z innymi komputerami; po usunięciu po prostu pobierze je z internetu.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Narzedzie,
        polecenie: Some("Delete-DeliveryOptimizationCache -Force"),
        wymaga_admina: true,
        miejsca: &[w(Windows, "ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "windows-old",
        nazwa: "Poprzednia wersja Windows (Windows.old)",
        wyjasnienie: "Kopia starego systemu zostawiona po dużej aktualizacji; po usunięciu nie da się już wrócić do poprzedniej wersji Windows.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("cleanmgr.exe /AUTOCLEAN"),
        wymaga_admina: true,
        miejsca: &[w(DyskSystemowy, "Windows.old")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "instalator-windows",
        nazwa: "Pliki instalatora nowej wersji Windows ($WINDOWS.~BT)",
        wyjasnienie: "Resztki po instalowaniu nowej wersji Windows; usuń je dopiero wtedy, gdy aktualizacja jest skończona i wszystko działa.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("cleanmgr.exe /AUTOCLEAN"),
        wymaga_admina: true,
        miejsca: &[w(DyskSystemowy, "$WINDOWS.~BT"), w(DyskSystemowy, "$WINDOWS.~WS")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "zrzuty-awarii",
        nazwa: "Zrzuty pamięci po awariach",
        wyjasnienie: "Zapis stanu komputera z chwili, gdy coś się wysypało; przydaje się tylko informatykowi, który szuka przyczyny awarii.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: true,
        miejsca: &[
            w(Windows, "Minidump"),
            w(Windows, "MEMORY.DMP"),
            w(Windows, "LiveKernelReports"),
            w(Local, "CrashDumps"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "raporty-bledow",
        nazwa: "Raporty błędów Windows",
        wyjasnienie: "Opisy awarii programów przygotowane do wysłania do Microsoftu; po usunięciu nic nie przestanie działać.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: true,
        miejsca: &[
            w(ProgramData, "Microsoft\\Windows\\WER\\ReportArchive"),
            w(ProgramData, "Microsoft\\Windows\\WER\\ReportQueue"),
            w(ProgramData, "Microsoft\\Windows\\WER\\Temp"),
            w(Local, "Microsoft\\Windows\\WER\\ReportArchive"),
            w(Local, "Microsoft\\Windows\\WER\\ReportQueue"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "miniatury",
        nazwa: "Pamięć miniatur i ikon",
        wyjasnienie: "Zapisane podglądy zdjęć i ikon w Eksploratorze; Windows zrobi je od nowa, a foldery ze zdjęciami przez chwilę otworzą się wolniej.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[
            w(Local, "Microsoft\\Windows\\Explorer\\thumbcache_*.db"),
            w(Local, "Microsoft\\Windows\\Explorer\\iconcache_*.db"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "shadery",
        nazwa: "Pamięć shaderów DirectX i karty graficznej",
        wyjasnienie: "Gotowe kawałki grafiki zapisane przez gry i sterowniki NVIDIA, AMD i Intel; po usunięciu gry przy pierwszym uruchomieniu mogą chwilę przycinać, potem wszystko wraca do normy.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[
            w(Local, "D3DSCache"),
            w(Local, "NVIDIA\\DXCache"),
            w(Local, "NVIDIA\\GLCache"),
            w(LocalLow, "NVIDIA\\PerDriverVersion\\DXCache"),
            w(LocalLow, "NVIDIA\\PerDriverVersion\\GLCache"),
            w(ProgramData, "NVIDIA Corporation\\NV_Cache"),
            w(Local, "AMD\\DxCache"),
            w(Local, "AMD\\DxcCache"),
            w(Local, "AMD\\GLCache"),
            w(Local, "AMD\\VkCache"),
            w(LocalLow, "Intel\\ShaderCache"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "przegladarki",
        nazwa: "Pamięć podręczna przeglądarek",
        wyjasnienie: "Zapisane kopie stron w Chrome, Edge, Firefoxie, Brave i Operze; hasła, zakładki, historia i logowania zostają, a strony najwyżej raz wczytają się wolniej.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[
            w(Local, "Google\\Chrome\\User Data\\*\\Cache"),
            w(Local, "Google\\Chrome\\User Data\\*\\Code Cache"),
            w(Local, "Google\\Chrome\\User Data\\*\\GPUCache"),
            w(Local, "Microsoft\\Edge\\User Data\\*\\Cache"),
            w(Local, "Microsoft\\Edge\\User Data\\*\\Code Cache"),
            w(Local, "Microsoft\\Edge\\User Data\\*\\GPUCache"),
            w(Local, "BraveSoftware\\Brave-Browser\\User Data\\*\\Cache"),
            w(Local, "BraveSoftware\\Brave-Browser\\User Data\\*\\Code Cache"),
            w(Local, "BraveSoftware\\Brave-Browser\\User Data\\*\\GPUCache"),
            w(Local, "Mozilla\\Firefox\\Profiles\\*\\cache2"),
            w(Local, "Opera Software\\*\\Cache"),
            w(Local, "Opera Software\\*\\*\\Cache"),
            w(Roaming, "Opera Software\\*\\Cache"),
            w(Roaming, "Opera Software\\*\\Code Cache"),
            w(Roaming, "Opera Software\\*\\GPUCache"),
            w(Roaming, "Opera Software\\*\\*\\Code Cache"),
            w(Roaming, "Opera Software\\*\\*\\GPUCache"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "komunikatory",
        nazwa: "Pamięć podręczna Discorda, Teams i Spotify",
        wyjasnienie: "Zapisane obrazki, filmiki i kawałki piosenek; czaty, ustawienia i konta zostają, a reszta dociągnie się z internetu.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[
            w(Roaming, "discord*\\Cache"),
            w(Roaming, "discord*\\Code Cache"),
            w(Roaming, "discord*\\GPUCache"),
            w(Roaming, "Microsoft\\Teams\\Cache"),
            w(Roaming, "Microsoft\\Teams\\Code Cache"),
            w(Roaming, "Microsoft\\Teams\\GPUCache"),
            w(Local, "Packages\\MSTeams_8wekyb3d8bbwe\\LocalCache\\Microsoft\\MSTeams\\EBWebView\\*\\Cache"),
            w(Local, "Packages\\MSTeams_8wekyb3d8bbwe\\LocalCache\\Microsoft\\MSTeams\\EBWebView\\*\\Code Cache"),
            w(Local, "Packages\\MSTeams_8wekyb3d8bbwe\\LocalCache\\Microsoft\\MSTeams\\EBWebView\\*\\GPUCache"),
            w(Local, "Spotify\\Data"),
            w(Local, "Packages\\SpotifyAB.SpotifyMusic_zpdnekdrzrea0\\LocalCache\\Spotify\\Data"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "spotify-offline",
        nazwa: "Utwory Spotify zapisane na dysku",
        wyjasnienie: "Pamięć piosenek Spotify razem z pobranymi do słuchania bez internetu; po usunięciu pobrane playlisty trzeba będzie ściągnąć jeszcze raz.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(Local, "Spotify\\Storage")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "steam-shadery-logi",
        nazwa: "Shadery i dzienniki Steama",
        wyjasnienie: "Grafika przygotowana przez gry ze Steama i stare dzienniki programu; gry odbudują shadery przy następnym uruchomieniu, przez chwilę mogą przycinać.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(BibliotekaSteam, "steamapps\\shadercache"), w(Steam, "logs"), w(Steam, "dumps")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "steam-pobierane",
        nazwa: "Niedokończone pobrania Steama",
        wyjasnienie: "Częściowo pobrane gry i aktualizacje; usuń je tylko wtedy, gdy Steam nic teraz nie pobiera, bo inaczej zacznie od początku.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(BibliotekaSteam, "steamapps\\downloading")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "programisci",
        nazwa: "Pamięć podręczna narzędzi programisty (npm, pip, NuGet, Gradle)",
        wyjasnienie: "Paczki pobrane przez narzędzia programisty i trzymane na zapas; po usunięciu narzędzia pobiorą je ponownie, gdy będą potrzebne.",
        poziom: Poziom::Bezpieczne,
        sposob: Sposob::Trwale,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[
            w(Local, "npm-cache"),
            w(Local, "Yarn\\Cache"),
            w(Local, "pip\\Cache"),
            w(Profil, ".nuget\\packages"),
            w(Local, "NuGet\\v3-cache"),
            w(Local, "NuGet\\Cache"),
            w(Profil, ".gradle\\caches"),
        ],
        cel: Cel::Calosc,
    },
    Regula {
        id: "stare-instalatory",
        nazwa: "Stare instalatory w Pobranych",
        wyjasnienie: "Pliki .exe, .msi i .iso pobrane ponad miesiąc temu; program zwykle jest już zainstalowany, a instalator można w razie czego pobrać jeszcze raz.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Kosz,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(Pobrane, "")],
        cel: Cel::Pliki { maski: &["*.exe", "*.msi", "*.iso"], starsze_niz_h: 30 * 24 },
    },
    Regula {
        id: "hibernacja",
        nazwa: "Plik hibernacji (hiberfil.sys)",
        wyjasnienie: "Miejsce na zapis pamięci, gdy komputer hibernuje; wyłączenie hibernacji je zwalnia, ale znika też szybkie uruchamianie Windows.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("powercfg -h off"),
        wymaga_admina: true,
        miejsca: &[w(DyskSystemowy, "hiberfil.sys")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "plik-wymiany",
        nazwa: "Plik wymiany (pagefile.sys, swapfile.sys)",
        wyjasnienie: "Zapasowa pamięć operacyjna na dysku, której Windows potrzebuje; bez niej programy zaczną się wysypywać, więc zostaw ją w spokoju.",
        poziom: Poziom::NieRuszaj,
        sposob: Sposob::Brak,
        polecenie: None,
        wymaga_admina: false,
        miejsca: &[w(DyskSystemowy, "pagefile.sys"), w(DyskSystemowy, "swapfile.sys")],
        cel: Cel::Calosc,
    },
    Regula {
        id: "winsxs",
        nazwa: "Magazyn składników Windows (WinSxS)",
        wyjasnienie: "Części systemu potrzebne do aktualizacji i naprawy Windows; ręczne usuwanie psuje system, bezpiecznie odchudza go tylko narzędzie DISM.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("Dism.exe /Online /Cleanup-Image /StartComponentCleanup"),
        wymaga_admina: true,
        miejsca: &[w(Windows, "WinSxS")],
        cel: Cel::BezRozmiaru,
    },
    Regula {
        id: "onedrive",
        nazwa: "Pliki OneDrive trzymane na dysku",
        wyjasnienie: "Pliki z chmury, które mają też kopię na tym komputerze; „Zwolnij miejsce” zostawia je w OneDrive, ale bez internetu ich nie otworzysz.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("attrib +U -P /S /D (w folderze OneDrive) albo „Zwolnij miejsce” w menu pliku"),
        wymaga_admina: false,
        miejsca: &[w(Profil, "OneDrive*")],
        cel: Cel::OneDriveLokalne,
    },
    Regula {
        id: "punkty-przywracania",
        nazwa: "Punkty przywracania systemu",
        wyjasnienie: "Zapisane stany systemu, do których można wrócić po awarii; usunięcie starych zwalnia miejsce, ale do nich już nie wrócisz.",
        poziom: Poziom::Sprawdz,
        sposob: Sposob::Narzedzie,
        polecenie: Some("vssadmin delete shadows /for=C: /oldest"),
        wymaga_admina: true,
        miejsca: &[w(DyskSystemowy, "System Volume Information")],
        cel: Cel::BezRozmiaru,
    },
];

/// Regula po identyfikatorze.
pub fn regula(id: &str) -> Option<&'static Regula> {
    KATALOG.iter().find(|r| r.id == id)
}
