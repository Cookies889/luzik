//! Twarda blokada usuwania. Kazda sciezka, zanim cokolwiek ja ruszy, przechodzi `Ochrona::sprawdz`.
//!
//! Nigdy nie usuwamy:
//! - korzenia dysku (C:\, D:\, \\serwer\udzial, /),
//! - niczego w C:\Windows poza wnetrzem Windows\Temp i Windows\SoftwareDistribution\Download,
//! - niczego w Program Files i Program Files (x86),
//! - katalogu Users, ProgramData ani samego profilu uzytkownika,
//! - katalogu w profilu, jesli nie lezy wewnatrz miejsca z ktorejs reguly (pliki w profilu wolno:
//!   to duplikaty i duze stare pliki wskazane przez uzytkownika).
//!
//! Sciezki porownujemy po czlonach i bez wielkosci liter (tak dziala Windows), po zdjeciu
//! przedrostka `\\?\`, kropek i spacji na koncu czlonu (Windows je ignoruje: `C:\Windows.` to
//! `C:\Windows`). `..`, strumienie NTFS (`plik:strumien`) i sciezki wzgledne odrzucamy od razu.
//! Gdy sciezka istnieje, sprawdzamy tez jej postac kanoniczna: dowiazanie albo junction
//! prowadzace do C:\Windows nie przejdzie.

use super::{miejsca_reguly, Poziom, Sposob, Srodowisko, KATALOG};
use std::path::Path;

/// Sciezka rozbita na czlony: pierwszy to korzen ("c:", "/" albo "\\serwer\udzial").
type Klucz = Vec<String>;

#[derive(Clone, Debug, Default)]
pub struct Ochrona {
    /// Dokladnie te sciezki sa zakazane (korzenie dyskow, Users, ProgramData).
    dokladnie: Vec<(Klucz, &'static str)>,
    /// Cale poddrzewa zakazane.
    poddrzewa: Vec<(Klucz, &'static str)>,
    /// Wnetrza wolne mimo zakazanego poddrzewa (Windows\Temp).
    wyjatki: Vec<Klucz>,
    /// Katalogi z profilami (C:\Users): kazde dziecko to profil.
    profile: Vec<Klucz>,
    /// Miejsca regul: w profilu katalog wolno usunac tylko wewnatrz nich.
    miejsca_regul: Vec<Klucz>,
}

/// Rozbija sciezke na czlony. Err z powodem, gdy sciezki nie wolno nawet rozwazac.
pub fn klucz(sciezka: &str) -> Result<Klucz, String> {
    let mut s = sciezka.replace('/', "\\");
    // \\?\C:\... i \\?\UNC\serwer\udzial\...
    if let Some(r) = s.strip_prefix("\\\\?\\UNC\\").or_else(|| s.strip_prefix("\\??\\UNC\\")) {
        s = format!("\\\\{r}");
    } else if let Some(r) = s.strip_prefix("\\\\?\\").or_else(|| s.strip_prefix("\\??\\")) {
        s = r.to_string();
    }
    let s = s.to_lowercase();
    let (korzen, reszta) = if let Some(r) = s.strip_prefix("\\\\") {
        // UNC: korzeniem jest \\serwer\udzial.
        let mut cz = r.splitn(3, '\\');
        let (serwer, udzial) = (cz.next().unwrap_or(""), cz.next().unwrap_or(""));
        if serwer.is_empty() || udzial.is_empty() {
            return Err("Niepełna ścieżka sieciowa.".into());
        }
        (format!("\\\\{serwer}\\{udzial}"), cz.next().unwrap_or("").to_string())
    } else if s.len() >= 2 && s.as_bytes()[1] == b':' && s.as_bytes()[0].is_ascii_alphabetic() {
        if s.len() > 2 && s.as_bytes()[2] != b'\\' {
            return Err("Ścieżka względna wobec bieżącego folderu dysku.".into());
        }
        (s[..2].to_string(), s[2..].to_string())
    } else if let Some(r) = s.strip_prefix('\\') {
        // Unix (/usr) albo Windows bez litery (\Windows): to drugie odrzucamy, bo zalezy od biezacego dysku.
        if cfg!(windows) {
            return Err("Ścieżka bez litery dysku.".into());
        }
        ("/".to_string(), r.to_string())
    } else {
        return Err("Ścieżka względna.".into());
    };
    let mut wynik = vec![korzen];
    for surowy in reszta.split('\\') {
        // Najpierw `.` i `..`, dopiero potem obcinanie kropek (inaczej `..` znikaloby bez sladu).
        match surowy {
            "" | "." => continue,
            ".." => return Err("Ścieżka z „..”.".into()),
            _ => {}
        }
        let czlon = if cfg!(windows) || wynik[0] != "/" { surowy.trim_end_matches(['.', ' ']) } else { surowy };
        match czlon {
            "" => return Err("Nazwa z samych kropek albo spacji.".into()),
            c if c.contains(':') && wynik[0] != "/" => return Err("Strumień NTFS albo dziwna nazwa z dwukropkiem.".into()),
            c if c.contains(['*', '?', '"', '<', '>', '|']) && wynik[0] != "/" => return Err("Znaki wieloznaczne w ścieżce.".into()),
            c => wynik.push(c.to_string()),
        }
    }
    Ok(wynik)
}

fn wewnatrz(k: &Klucz, baza: &Klucz) -> bool {
    k.len() >= baza.len() && k[..baza.len()] == baza[..]
}

fn gleboko(k: &Klucz, baza: &Klucz) -> bool {
    k.len() > baza.len() && wewnatrz(k, baza)
}

impl Ochrona {
    fn dodaj_dokladnie(&mut self, p: &Path, powod: &'static str) {
        if let Ok(k) = klucz(&p.to_string_lossy()) {
            self.dokladnie.push((k, powod));
        }
    }
    fn dodaj_poddrzewo(&mut self, p: &Path, powod: &'static str) {
        if let Ok(k) = klucz(&p.to_string_lossy()) {
            self.poddrzewa.push((k, powod));
        }
    }
    fn dodaj_wyjatek(&mut self, p: &Path) {
        if let Ok(k) = klucz(&p.to_string_lossy()) {
            self.wyjatki.push(k);
        }
    }
    fn dodaj_profile(&mut self, p: &Path) {
        if let Ok(k) = klucz(&p.to_string_lossy()) {
            self.profile.push(k);
        }
    }
    pub fn dodaj_miejsce_reguly(&mut self, p: &Path) {
        if let Ok(k) = klucz(&p.to_string_lossy()) {
            self.miejsca_regul.push(k);
        }
    }

    /// Ochrona dla Windows z podanych katalogow (testy i `z_srodowiska`).
    pub fn windows(srod: &Srodowisko) -> Self {
        let mut o = Ochrona::default();
        for d in srod.dyski.iter().chain(std::iter::once(&srod.dysk_systemowy)) {
            o.dodaj_dokladnie(d, "korzeń dysku");
        }
        o.dodaj_poddrzewo(&srod.windows, "folder Windows");
        o.dodaj_wyjatek(&srod.windows.join("Temp"));
        o.dodaj_wyjatek(&srod.windows.join("SoftwareDistribution").join("Download"));
        o.dodaj_poddrzewo(&srod.program_files, "Program Files");
        o.dodaj_poddrzewo(&srod.program_files_x86, "Program Files (x86)");
        o.dodaj_dokladnie(&srod.program_data, "ProgramData");
        let users = srod.dysk_systemowy.join("Users");
        o.dodaj_dokladnie(&users, "folder Users");
        o.dodaj_profile(&users);
        for u in &srod.uzytkownicy {
            if let Some(rodzic) = u.profil.parent() {
                o.dodaj_dokladnie(rodzic, "folder Users");
                o.dodaj_profile(rodzic);
            }
        }
        for r in KATALOG.iter().filter(|r| r.poziom != Poziom::NieRuszaj && r.sposob != Sposob::Brak) {
            for (m, _) in miejsca_reguly(srod, r) {
                o.dodaj_miejsce_reguly(&m);
            }
        }
        o
    }

    /// Ochrona tego komputera. Na Windows z prawdziwych folderow, z LUZIK_SZTUCZNE_C ze sztucznego C:\,
    /// a na Linuksie (tylko do prob) chroni katalogi systemu i katalogi domowe.
    pub fn z_otoczenia() -> Self {
        if let Ok(srod) = Srodowisko::z_otoczenia() {
            return Self::windows(&srod);
        }
        let mut o = Ochrona::default();
        o.dodaj_dokladnie(Path::new("/"), "korzeń dysku");
        for p in ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc", "/boot", "/proc", "/sys", "/dev", "/opt", "/var", "/srv", "/run", "/root"] {
            o.dodaj_poddrzewo(Path::new(p), "katalog systemu");
        }
        o.dodaj_dokladnie(Path::new("/home"), "katalog domowy");
        o.dodaj_dokladnie(Path::new("/tmp"), "katalog tymczasowy systemu");
        o.dodaj_profile(Path::new("/home"));
        // Na Linuksie katalogi w domu wolno w pamieciach podrecznych.
        if let Some(dom) = std::env::var_os("HOME") {
            o.dodaj_miejsce_reguly(&Path::new(&dom).join(".cache"));
        }
        o
    }

    /// Ok, gdy sciezke wolno usunac. `katalog`: czy to folder (w profilu folder wymaga reguly).
    pub fn sprawdz(&self, sciezka: &str, katalog: bool) -> Result<(), String> {
        self.sprawdz_klucz(&klucz(sciezka)?, katalog)?;
        // Postac kanoniczna (dowiazania, junctiony, krotkie nazwy 8.3), gdy sciezka istnieje.
        if let Ok(prawdziwa) = std::fs::canonicalize(sciezka) {
            let k = klucz(&prawdziwa.to_string_lossy())?;
            self.sprawdz_klucz(&k, katalog).map_err(|e| format!("{e} (ścieżka prowadzi do {})", prawdziwa.display()))?;
        }
        Ok(())
    }

    fn sprawdz_klucz(&self, k: &Klucz, katalog: bool) -> Result<(), String> {
        if k.len() <= 1 {
            return Err("Chroniony: korzeń dysku.".into());
        }
        if let Some((_, powod)) = self.dokladnie.iter().find(|(d, _)| d == k) {
            return Err(format!("Chroniony: {powod}."));
        }
        for (p, powod) in &self.poddrzewa {
            if wewnatrz(k, p) && !self.wyjatki.iter().any(|w| gleboko(k, w)) {
                return Err(format!("Chroniony: {powod}."));
            }
        }
        for users in &self.profile {
            if !gleboko(k, users) {
                continue;
            }
            if k.len() == users.len() + 1 {
                return Err("Chroniony: profil użytkownika.".into());
            }
            if katalog && !self.miejsca_regul.iter().any(|m| gleboko(k, m)) {
                return Err("Chroniony: folder w profilu użytkownika, którego nie obejmuje żadna reguła.".into());
            }
        }
        Ok(())
    }
}
