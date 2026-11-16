//! luzik-cli: skan sciezki z konsoli, wynik jako JSON na stdout.
//!
//!   luzik-cli skanuj <sciezka> [--najwieksze N] [--watki N] [--przez-systemy] [--postep]
//!   luzik-cli dyski
//!   luzik-cli porzadki [--korzen <katalog udajacy C:\>] [--uzytkownik NAZWA] [--admin] [--json]
//!   luzik-cli sztuczne-c <katalog>
//!   luzik-cli duplikaty <sciezka>
//!   luzik-cli kategorie <sciezka>
//!   luzik-cli stare <sciezka> [--mb N] [--dni N]

use luzik_lib::skan::{self, Opcje, Postep};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const BLEDY_W_WYNIKU: usize = 100;

// Poza src/bin, bo cargo robi z kazdego pliku w src/bin osobny program.
#[path = "../cli/porzadki.rs"]
mod porzadki_cli;

fn pomoc() -> ! {
    eprintln!("Uzycie: luzik-cli skanuj <sciezka> [--najwieksze N] [--watki N] [--przez-systemy] [--postep]");
    eprintln!("        luzik-cli dyski");
    eprintln!("        luzik-cli porzadki [--korzen <katalog udajacy C:\\>] [--uzytkownik NAZWA] [--admin] [--json]");
    eprintln!("        luzik-cli sztuczne-c <katalog>");
    eprintln!("        luzik-cli duplikaty|kategorie <sciezka>");
    eprintln!("        luzik-cli stare <sciezka> [--mb N] [--dni N]");
    std::process::exit(2)
}

fn main() {
    let mut argumenty = std::env::args().skip(1).peekable();
    let mut sciezka: Option<PathBuf> = None;
    let mut opcje = Opcje::default();
    let mut najwieksze = 0usize;
    let mut z_postepem = false;

    match argumenty.peek().map(String::as_str) {
        Some("dyski") => {
            println!("{}", serde_json::to_string_pretty(&skan::lista_dyskow()).unwrap());
            return;
        }
        Some("skanuj") => {
            argumenty.next();
        }
        Some("porzadki" | "sztuczne-c" | "duplikaty" | "kategorie" | "stare") => {
            let polecenie = argumenty.next().unwrap();
            porzadki_cli::uruchom(&polecenie, argumenty.collect());
            return;
        }
        _ => {}
    }
    while let Some(a) = argumenty.next() {
        match a.as_str() {
            "--najwieksze" => najwieksze = argumenty.next().and_then(|x| x.parse().ok()).unwrap_or_else(|| pomoc()),
            "--watki" => opcje.watki = argumenty.next().and_then(|x| x.parse().ok()).unwrap_or_else(|| pomoc()),
            "--przez-systemy" => opcje.przez_systemy = true,
            "--postep" => z_postepem = true,
            "-h" | "--help" => pomoc(),
            _ if sciezka.is_none() => sciezka = Some(PathBuf::from(a)),
            _ => pomoc(),
        }
    }
    let Some(sciezka) = sciezka else { pomoc() };

    let anuluj = AtomicBool::new(false);
    let zglos = |p: Postep| {
        eprintln!("{} plikow, {} MB, {} plikow/s, {}", p.pliki, p.bajty / 1_000_000, p.pliki_na_s, p.biezacy);
    };
    let postep: Option<&(dyn Fn(Postep) + Sync)> = if z_postepem { Some(&zglos) } else { None };

    let wynik = match skan::skanuj(&sciezka, &opcje, &anuluj, postep) {
        Ok(w) => w,
        Err(e) => {
            println!("{}", json!({ "ok": false, "sciezka": sciezka.to_string_lossy(), "blad": e.to_string() }));
            std::process::exit(1);
        }
    };
    let s = &wynik.statystyki;
    let sekundy = (s.czas_ms as f64 / 1000.0).max(0.001);
    let mut wyjscie = json!({
        "ok": true,
        "sciezka": sciezka.to_string_lossy(),
        "suma": wynik.suma(),
        "na_dysku": wynik.na_dysku(),
        "pliki": s.pliki,
        "katalogi": s.katalogi,
        "dowiazania": s.dowiazania,
        "twarde_linki_pominiete": s.twarde_linki_pominiete,
        "inne_systemy": s.inne_systemy,
        "czas_ms": s.czas_ms,
        "pliki_na_s": (s.pliki as f64 / sekundy) as u64,
        "bledy_razem": s.bledy_razem,
        "bledy": s.bledy.iter().take(BLEDY_W_WYNIKU).collect::<Vec<_>>(),
        "wezly": wynik.drzewo.liczba_wezlow(),
        "pamiec_drzewa": wynik.drzewo.pamiec(),
    });
    if najwieksze > 0 {
        wyjscie["najwieksze"] = json!(wynik.drzewo.najwieksze_pliki(najwieksze));
    }
    println!("{}", serde_json::to_string_pretty(&wyjscie).unwrap());
}
