//! Porzadki: co na Windows 11 da sie bezpiecznie usunac.
//!
//! - `reguly`: katalog regul (smieci systemu, pamieci podreczne programow, pliki specjalne),
//!   kazda z poziomem, sposobem usuniecia i wyjasnieniem dla laika,
//! - `srodowisko`: gdzie na tym komputerze leza katalogi z regul (znane foldery Windows,
//!   wszyscy uzytkownicy, gdy aplikacja ma prawa administratora) albo sztuczne C:\ do testow,
//! - `duplikaty`: te same pliki w kilku miejscach (rozmiar, skrot 64 KB, pelny skrot),
//! - `kategorie`: co zajmuje dysk wedlug rodzaju (wideo, gry, obrazy...) i stare duze pliki,
//! - `przyklad`: buduje sztuczne C:\ dla testow i `luzik-cli sztuczne-c`.
//!
//! - `ochrona`: twarda blokada sciezek (Windows, Program Files, profile, korzenie dyskow),
//! - `usuwanie`: do Kosza albo na stale, z pomijaniem zablokowanych plikow, i historia porzadkow,
//! - `narzedzia`: programy Windows z regul (powercfg, DISM, cleanmgr) i oproznianie Kosza.

pub mod duplikaty;
pub mod kategorie;
pub mod narzedzia;
pub mod ochrona;
mod pomiar;
pub mod usuwanie;
pub mod przyklad;
pub mod reguly;
pub mod srodowisko;
#[cfg(test)]
mod testy;
#[cfg(test)]
mod testy_usuwania;

pub use reguly::{Baza, Cel, Poziom, Regula, Sposob, Wzor, KATALOG};
pub use srodowisko::{Srodowisko, Uzytkownik};

use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

/// Jedno miejsce na dysku, ktore zlapala regula.
#[derive(Serialize, Clone, Debug)]
pub struct Miejsce {
    pub sciezka: String,
    /// Czyj to katalog (dla regul z profilu uzytkownika).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uzytkownik: Option<String>,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pliki: u64,
    /// Wpisy, ktorych nie udalo sie odczytac (brak uprawnien, plik zajety).
    pub bledy: u64,
}

/// Wynik jednej reguly: dane z katalogu regul i wszystkie miejsca, ktore zlapala.
#[derive(Serialize, Clone, Debug)]
pub struct Znalezisko {
    pub id: &'static str,
    pub nazwa: &'static str,
    pub wyjasnienie: &'static str,
    pub poziom: Poziom,
    pub sposob: Sposob,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polecenie: Option<&'static str>,
    pub wymaga_admina: bool,
    /// false: regula tylko wskazuje miejsce (WinSxS, punkty przywracania), rozmiar poda narzedzie systemowe.
    pub rozmiar_znany: bool,
    pub rozmiar: u64,
    pub na_dysku: u64,
    pub pliki: u64,
    pub bledy: u64,
    pub miejsca: Vec<Miejsce>,
}

/// Przechodzi wszystkie reguly katalogu na danym srodowisku.
/// Znaleziska od najbezpieczniejszych, w kazdym poziomie od najwiekszego.
pub fn skanuj(srod: &Srodowisko, anuluj: &AtomicBool) -> Vec<Znalezisko> {
    let mut wynik: Vec<Znalezisko> =
        KATALOG.par_iter().filter_map(|r| sprawdz_regule(srod, r, anuluj)).collect();
    wynik.sort_by(|a, b| a.poziom.cmp(&b.poziom).then(b.na_dysku.cmp(&a.na_dysku)).then(a.id.cmp(b.id)));
    wynik
}

/// Wynik jednej reguly albo None, gdy nic nie znalazla.
pub fn sprawdz_regule(srod: &Srodowisko, regula: &Regula, anuluj: &AtomicBool) -> Option<Znalezisko> {
    let mut miejsca = Vec::new();
    for (sciezka, uzytkownik) in miejsca_reguly(srod, regula) {
        let tekst = sciezka.to_string_lossy().into_owned();
        if regula.cel == Cel::BezRozmiaru {
            miejsca.push(Miejsce { sciezka: tekst, uzytkownik, rozmiar: 0, na_dysku: 0, pliki: 0, bledy: 0 });
            continue;
        }
        let m = pomiar::zmierz(&sciezka, &pomiar::Filtr::z_reguly(regula, srod.teraz), anuluj, None);
        if m.pliki == 0 {
            continue;
        }
        miejsca.push(Miejsce {
            sciezka: tekst,
            uzytkownik,
            rozmiar: m.rozmiar,
            na_dysku: m.na_dysku,
            pliki: m.pliki,
            bledy: m.bledy,
        });
    }
    if miejsca.is_empty() {
        return None;
    }
    miejsca.sort_by(|a, b| b.na_dysku.cmp(&a.na_dysku).then(a.sciezka.cmp(&b.sciezka)));
    Some(Znalezisko {
        id: regula.id,
        nazwa: regula.nazwa,
        wyjasnienie: regula.wyjasnienie,
        poziom: regula.poziom,
        sposob: regula.sposob,
        polecenie: regula.polecenie,
        wymaga_admina: regula.wymaga_admina,
        rozmiar_znany: regula.cel != Cel::BezRozmiaru,
        rozmiar: miejsca.iter().map(|m| m.rozmiar).sum(),
        na_dysku: miejsca.iter().map(|m| m.na_dysku).sum(),
        pliki: miejsca.iter().map(|m| m.pliki).sum(),
        bledy: miejsca.iter().map(|m| m.bledy).sum(),
        miejsca,
    })
}

/// Istniejace sciezki, ktore pasuja do wzorow reguly (katalogi albo pliki), bez powtorek.
pub fn miejsca_reguly(srod: &Srodowisko, regula: &Regula) -> Vec<(PathBuf, Option<String>)> {
    let mut widziane = HashSet::new();
    let mut wynik = Vec::new();
    for wzor in regula.miejsca {
        for (baza, uzytkownik) in srod.bazy(wzor.baza) {
            for sciezka in pomiar::rozwin(&baza, wzor.sciezka) {
                if regula.cel == Cel::OneDriveLokalne && !sciezka.is_dir() {
                    continue;
                }
                if widziane.insert(klucz_sciezki(&sciezka)) {
                    wynik.push((sciezka, uzytkownik.clone()));
                }
            }
        }
    }
    wynik
}

/// Wszystkie pliki, ktore regula policzyla (to samo, co by usunela). Regula bez rozmiaru: pusto.
pub fn pliki_reguly(srod: &Srodowisko, regula: &Regula, anuluj: &AtomicBool) -> Vec<PathBuf> {
    if regula.cel == Cel::BezRozmiaru {
        return Vec::new();
    }
    let filtr = pomiar::Filtr::z_reguly(regula, srod.teraz);
    let mut pliki = Vec::new();
    for (sciezka, _) in miejsca_reguly(srod, regula) {
        pomiar::zmierz(&sciezka, &filtr, anuluj, Some(&mut pliki));
    }
    pliki
}

/// Usuwa pliki wybranych regul (te same, ktore regula policzyla). Reguly „nie ruszaj”, bez sposobu
/// i z narzedziem sa pomijane z powodem: narzedzia uruchamia `narzedzia::uruchom`.
pub fn usun_reguly(
    srod: &Srodowisko,
    ids: &[String],
    tryb: usuwanie::Tryb,
    usuwacz: &usuwanie::Usuwacz,
    anuluj: &AtomicBool,
) -> usuwanie::WynikUsuwania {
    let mut wynik = usuwanie::WynikUsuwania::nowy(tryb);
    for id in ids {
        let Some(regula) = KATALOG.iter().find(|r| r.id == id) else {
            wynik.dolacz(pominiete(tryb, id, "Nieznana reguła."));
            continue;
        };
        if regula.poziom == Poziom::NieRuszaj || regula.sposob == Sposob::Brak {
            wynik.dolacz(pominiete(tryb, regula.nazwa, "Tego Luzik nie usuwa."));
            continue;
        }
        if regula.sposob == Sposob::Narzedzie || regula.id == "kosz" {
            wynik.dolacz(pominiete(tryb, regula.nazwa, "To czyści narzędzie Windows, uruchom je osobno."));
            continue;
        }
        let pliki = pliki_reguly(srod, regula, anuluj);
        let mut w = usuwacz.usun(&pliki, tryb);
        if regula.cel == Cel::Calosc {
            for (miejsce, _) in miejsca_reguly(srod, regula) {
                if miejsce.is_dir() {
                    usuwanie::usun_puste_podfoldery(&miejsce, usuwacz.ochrona, &mut w);
                }
            }
        }
        // Pozycja to regula: liczy sie, gdy nic nie zostalo pominiete.
        w.pozycje = u64::from(w.pominiete_razem == 0);
        wynik.dolacz(w);
    }
    wynik
}

fn pominiete(tryb: usuwanie::Tryb, sciezka: &str, powod: &str) -> usuwanie::WynikUsuwania {
    let mut w = usuwanie::WynikUsuwania::nowy(tryb);
    w.pominiete_razem = 1;
    w.pominiete.push(usuwanie::Pominiety { sciezka: sciezka.into(), powod: powod.into() });
    w
}

/// Klucz do porownania sciezek: Windows nie rozroznia wielkosci liter.
fn klucz_sciezki(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}
