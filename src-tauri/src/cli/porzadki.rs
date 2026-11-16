//! Polecenia porzadkow w luzik-cli. `porzadki` wypisuje znaleziska po polsku (albo JSON
//! z `--json`), reszta zawsze JSON.

use luzik_lib::porzadki::{self, duplikaty, kategorie, przyklad, Poziom, Srodowisko};
use luzik_lib::skan::{self, Opcje};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn blad(tekst: &str) -> ! {
    eprintln!("{tekst}");
    std::process::exit(2)
}

/// 1536 -> "1,5 KB" (jednostki po 1024, jak w Eksploratorze Windows).
pub fn ludzki(bajty: u64) -> String {
    const J: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut x = bajty as f64;
    let mut i = 0;
    while x >= 1024.0 && i < J.len() - 1 {
        x /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bajty} B")
    } else {
        format!("{x:.1} {}", J[i]).replace('.', ",")
    }
}

fn pliki(n: u64) -> String {
    let slowo = if n == 1 {
        "plik"
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "pliki"
    } else {
        "plików"
    };
    format!("{n} {slowo}")
}

fn drzewo(sciezka: &Path) -> skan::Drzewo {
    match skan::skanuj(sciezka, &Opcje::default(), &AtomicBool::new(false), None) {
        Ok(w) => w.drzewo,
        Err(e) => blad(&format!("Nie mogę przeskanować {}: {e}", sciezka.display())),
    }
}

fn wypisz(w: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&w).unwrap());
}

pub fn uruchom(polecenie: &str, argumenty: Vec<String>) {
    let mut it = argumenty.into_iter();
    let mut korzen: Option<PathBuf> = None;
    let mut uzytkownik: Option<String> = None;
    let (mut admin, mut jako_json) = (false, false);
    let (mut mb, mut dni) = (100u64, 180u64);
    let mut sciezka: Option<PathBuf> = None;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--korzen" => korzen = it.next().map(PathBuf::from),
            "--uzytkownik" => uzytkownik = it.next(),
            "--admin" => admin = true,
            "--json" => jako_json = true,
            "--mb" => mb = it.next().and_then(|x| x.parse().ok()).unwrap_or_else(|| blad("--mb wymaga liczby")),
            "--dni" => dni = it.next().and_then(|x| x.parse().ok()).unwrap_or_else(|| blad("--dni wymaga liczby")),
            _ if sciezka.is_none() && !a.starts_with("--") => sciezka = Some(PathBuf::from(a)),
            _ => blad(&format!("Nieznany argument: {a}")),
        }
    }
    let wymagana = |s: Option<PathBuf>| s.unwrap_or_else(|| blad(&format!("luzik-cli {polecenie} wymaga ścieżki")));

    match polecenie {
        "sztuczne-c" => {
            let k = wymagana(sciezka);
            match przyklad::zbuduj(&k) {
                Ok(o) => wypisz(json!({ "ok": true, "korzen": k.to_string_lossy(), "chronione": o.chronione.len() })),
                Err(e) => blad(&format!("Nie mogę zbudować sztucznego C:\\ w {}: {e}", k.display())),
            }
        }
        "porzadki" => {
            let srod = match korzen {
                Some(k) => Srodowisko::sztuczne(&k, uzytkownik.as_deref(), admin),
                None => Srodowisko::z_otoczenia().unwrap_or_else(|e| blad(&e)),
            };
            let wynik = porzadki::skanuj(&srod, &AtomicBool::new(false));
            if jako_json {
                wypisz(json!({
                    "ok": true,
                    "dysk_systemowy": srod.dysk_systemowy.to_string_lossy(),
                    "admin": srod.admin,
                    "uzytkownicy": srod.uzytkownicy.iter().map(|u| &u.nazwa).collect::<Vec<_>>(),
                    "znaleziska": wynik,
                }));
            } else {
                wypisz_porzadki(&srod, &wynik);
            }
        }
        "duplikaty" => {
            let d = drzewo(&wymagana(sciezka));
            wypisz(json!(duplikaty::znajdz(duplikaty::kandydaci_z_drzewa(&d), &AtomicBool::new(false))));
        }
        "kategorie" => wypisz(json!(kategorie::policz(&drzewo(&wymagana(sciezka))))),
        "stare" => {
            let d = drzewo(&wymagana(sciezka));
            let w = kategorie::stare_duze(&d, mb * 1024 * 1024, dni, porzadki::srodowisko::teraz(), 200);
            wypisz(json!(w));
        }
        _ => blad(&format!("Nieznane polecenie: {polecenie}")),
    }
}

fn wypisz_porzadki(srod: &Srodowisko, wynik: &[porzadki::Znalezisko]) {
    let nazwy: Vec<&str> = srod.uzytkownicy.iter().map(|u| u.nazwa.as_str()).collect();
    println!(
        "Porządki na {} (użytkownicy: {}; administrator: {})",
        srod.dysk_systemowy.display(),
        nazwy.join(", "),
        if srod.admin { "tak" } else { "nie" }
    );
    for z in wynik {
        println!();
        let ile = if z.rozmiar_znany {
            format!("{} na dysku, {}", ludzki(z.na_dysku), pliki(z.pliki))
        } else {
            "rozmiar poda narzędzie Windows".to_string()
        };
        println!("[{}] {}: {}", z.poziom.etykieta(), z.nazwa, ile);
        println!("    {}", z.wyjasnienie);
        let mut jak = format!("    Jak usunąć: {}", z.sposob.etykieta());
        if let Some(p) = z.polecenie {
            jak.push_str(&format!(" ({p})"));
        }
        if z.wymaga_admina {
            jak.push_str(", potrzebne prawa administratora");
        }
        println!("{jak}");
        for m in &z.miejsca {
            let kto = m.uzytkownik.as_deref().map(|u| format!(" [{u}]")).unwrap_or_default();
            if z.rozmiar_znany {
                println!("      {}{}: {}", m.sciezka, kto, ludzki(m.na_dysku));
            } else {
                println!("      {}{}", m.sciezka, kto);
            }
        }
    }
    let suma = |p: Poziom| wynik.iter().filter(|z| z.poziom == p).map(|z| z.na_dysku).sum::<u64>();
    println!();
    println!(
        "Razem: bezpieczne {}, do sprawdzenia {}, nie ruszać {}",
        ludzki(suma(Poziom::Bezpieczne)),
        ludzki(suma(Poziom::Sprawdz)),
        ludzki(suma(Poziom::NieRuszaj))
    );
}
