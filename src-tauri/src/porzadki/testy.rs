//! Testy porzadkow: reguly na sztucznym C:\, test negatywny (dokumenty, pulpit i profile
//! przegladarek nietkniete), duplikaty, kategorie i stare duze pliki.

use super::duplikaty::{self, MIN_ROZMIAR, PREFIKS};
use super::kategorie::{self, Kategoria};
use super::pomiar::pasuje;
use super::przyklad::{self, BIEZACY, DRUGI};
use super::srodowisko::sciezki_bibliotek;
use super::*;
use crate::skan::{self, Opcje};
use std::collections::HashMap;
use std::path::Path;

fn sztuczne(admin: bool) -> (tempfile::TempDir, przyklad::Opis, Srodowisko) {
    let tmp = tempfile::tempdir().unwrap();
    let korzen = tmp.path().join("C");
    let opis = przyklad::zbuduj(&korzen).unwrap();
    let srod = Srodowisko::sztuczne(&korzen, Some(BIEZACY), admin);
    (tmp, opis, srod)
}

fn po_id(z: &[Znalezisko]) -> HashMap<&'static str, &Znalezisko> {
    z.iter().map(|z| (z.id, z)).collect()
}

/// (miejsca, pliki, rozmiar) reguly.
fn liczby(z: &HashMap<&str, &Znalezisko>, id: &str) -> (usize, u64, u64) {
    let z = z.get(id).unwrap_or_else(|| panic!("regula {id} nic nie znalazla"));
    (z.miejsca.len(), z.pliki, z.rozmiar)
}

#[test]
fn katalog_regul_jest_kompletny_i_po_polsku() {
    let wymagane = [
        "temp-uzytkownika", "temp-windows", "kosz", "aktualizacje-windows", "optymalizacja-dostarczania",
        "windows-old", "instalator-windows", "zrzuty-awarii", "raporty-bledow", "miniatury", "shadery",
        "przegladarki", "komunikatory", "steam-shadery-logi", "steam-pobierane", "programisci",
        "stare-instalatory", "hibernacja", "plik-wymiany", "winsxs", "onedrive", "punkty-przywracania",
    ];
    for id in wymagane {
        assert!(reguly::regula(id).is_some(), "brak reguly {id}");
    }
    let mut id = std::collections::HashSet::new();
    for r in KATALOG {
        assert!(id.insert(r.id), "powtorzone id {}", r.id);
        assert!(!r.nazwa.is_empty() && !r.miejsca.is_empty(), "{}", r.id);
        // Jedno zdanie: konczy sie kropka i nie ma kropki w srodku.
        assert!(r.wyjasnienie.ends_with('.'), "{}: wyjasnienie bez kropki", r.id);
        assert!(!r.wyjasnienie[..r.wyjasnienie.len() - 1].contains(". "), "{}: wiecej niz jedno zdanie", r.id);
        assert!(!r.wyjasnienie.contains('\u{2014}') && !r.wyjasnienie.contains('\u{2013}'), "{}: dlugi myslnik", r.id);
        assert_eq!(r.sposob == Sposob::Narzedzie, r.polecenie.is_some(), "{}: narzedzie bez polecenia", r.id);
    }
    assert_eq!(reguly::regula("plik-wymiany").unwrap().poziom, Poziom::NieRuszaj);
    assert_eq!(reguly::regula("winsxs").unwrap().poziom, Poziom::Sprawdz);
    assert_eq!(reguly::regula("hibernacja").unwrap().polecenie, Some("powercfg -h off"));
}

#[test]
fn wzory_z_gwiazdka_bez_wielkosci_liter() {
    assert!(pasuje("thumbcache_*.db", "thumbcache_256.db"));
    assert!(pasuje("thumbcache_*.db", "THUMBCACHE_IDX.DB"));
    assert!(!pasuje("thumbcache_*.db", "thumbcache_256.db.bak"));
    assert!(pasuje("*.exe", "ChromeSetup.EXE"));
    assert!(!pasuje("*.exe", "plik.exe.txt"));
    assert!(pasuje("OneDrive*", "OneDrive - Firma"));
    assert!(pasuje("discord*", "discordptb"));
    assert!(pasuje("a?c", "abc") && !pasuje("a?c", "ac"));
    assert!(pasuje("*", "cokolwiek"));
}

#[test]
fn biblioteki_steam_z_libraryfolders_vdf() {
    let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
    let s: Vec<String> = sciezki_bibliotek(vdf).iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert_eq!(s, vec!["C:\\Program Files (x86)\\Steam".to_string(), "D:\\SteamLibrary".to_string()]);
}

#[test]
fn reguly_lapia_smieci_na_sztucznym_c() {
    let (_tmp, _opis, srod) = sztuczne(false);
    let wynik = skanuj(&srod, &AtomicBool::new(false));
    let z = po_id(&wynik);
    for r in KATALOG {
        assert!(z.contains_key(r.id), "regula {} nic nie znalazla na sztucznym C:\\", r.id);
    }
    // %TEMP%: dwa pliki starsze niz doba (takze w podkatalogu), swiezy zostaje.
    assert_eq!(liczby(&z, "temp-uzytkownika"), (1, 2, 30_000 + 25_000));
    assert_eq!(liczby(&z, "temp-windows"), (1, 1, 20_000));
    assert_eq!(liczby(&z, "kosz"), (1, 2, 400_000 + 544));
    assert_eq!(liczby(&z, "aktualizacje-windows"), (1, 1, 300_000));
    assert_eq!(liczby(&z, "optymalizacja-dostarczania"), (1, 1, 150_000));
    assert_eq!(liczby(&z, "windows-old"), (1, 1, 700_000));
    assert_eq!(liczby(&z, "instalator-windows"), (1, 1, 250_000));
    assert_eq!(liczby(&z, "zrzuty-awarii"), (4, 4, 40_000 + 500_000 + 80_000 + 90_000));
    assert_eq!(liczby(&z, "raporty-bledow"), (2, 2, 12_000 + 6_000));
    assert_eq!(liczby(&z, "miniatury"), (2, 2, 210_000 + 35_000));
    assert_eq!(liczby(&z, "shadery"), (5, 5, 66_000 + 77_000 + 22_000 + 60_000 + 18_000));
    // Chrome (3 katalogi w Default i Cache w Profile 1), Edge, Brave, Firefox, Opera (Local i Roaming).
    assert_eq!(liczby(&z, "przegladarki"), (9, 9, 120_000 + 45_000 + 15_000 + 60_000 + 70_000 + 33_000 + 55_000 + 44_000 + 11_000));
    // Discord x3, Teams, Spotify Data; junction "Application Data" niczego nie podwaja.
    assert_eq!(liczby(&z, "komunikatory"), (5, 5, 130_000 + 40_000 + 12_000 + 50_000 + 140_000));
    assert_eq!(liczby(&z, "spotify-offline"), (1, 1, 160_000));
    // Shadery z instalacji i z drugiej biblioteki z libraryfolders.vdf, do tego logi.
    assert_eq!(liczby(&z, "steam-shadery-logi"), (3, 3, 90_000 + 70_000 + 8_000));
    assert_eq!(liczby(&z, "steam-pobierane"), (1, 1, 110_000));
    assert_eq!(liczby(&z, "programisci"), (4, 4, 48_000 + 27_000 + 38_000 + 42_000));
    // Instalatory starsze niz 30 dni (takze w podfolderze); swiezy .exe, pdf i jpg zostaja.
    assert_eq!(liczby(&z, "stare-instalatory"), (1, 4, 100_000 + 60_000 + 900_000 + 50_000));
    assert_eq!(liczby(&z, "hibernacja"), (1, 1, 800_000));
    assert_eq!(liczby(&z, "plik-wymiany"), (2, 2, 900_000 + 16_000));
    assert_eq!(liczby(&z, "onedrive"), (2, 2, 25_000 + 35_000));
    // WinSxS i punkty przywracania: tylko wskazanie, rozmiar poda DISM i vssadmin.
    for id in ["winsxs", "punkty-przywracania"] {
        let w = z[id];
        assert!(!w.rozmiar_znany && w.rozmiar == 0 && w.miejsca.len() == 1, "{id}");
    }
    let hiber = z["hibernacja"];
    assert_eq!((hiber.poziom, hiber.sposob, hiber.wymaga_admina), (Poziom::Sprawdz, Sposob::Narzedzie, true));
    assert_eq!(z["plik-wymiany"].sposob, Sposob::Brak);
    assert_eq!(z["stare-instalatory"].sposob, Sposob::Kosz);
    // Na dysku nie mniej niz logicznie (sztuczne pliki nie sa rzadkie).
    for w in &wynik {
        assert!(w.na_dysku >= w.rozmiar, "{}", w.id);
    }
    // Kolejnosc: bezpieczne, potem sprawdz, na koncu nie ruszaj.
    assert!(wynik.windows(2).all(|p| p[0].poziom <= p[1].poziom));
    assert_eq!(wynik.last().unwrap().id, "plik-wymiany");
    // Bez admina tylko biezacy uzytkownik.
    assert!(wynik.iter().flat_map(|w| &w.miejsca).all(|m| m.uzytkownik.as_deref() != Some(DRUGI)));
}

#[test]
fn inni_uzytkownicy_tylko_z_prawami_administratora() {
    let (_tmp, _opis, srod) = sztuczne(true);
    let nazwy: Vec<&str> = srod.uzytkownicy.iter().map(|u| u.nazwa.as_str()).collect();
    assert_eq!(nazwy, vec![BIEZACY, DRUGI], "Public i Default to nie ludzie");
    let wynik = skanuj(&srod, &AtomicBool::new(false));
    let z = po_id(&wynik);
    assert_eq!(liczby(&z, "temp-uzytkownika"), (2, 4, 2 * (30_000 + 25_000)));
    let przegladarki = z["przegladarki"];
    assert_eq!(przegladarki.miejsca.len(), 10);
    assert!(przegladarki.miejsca.iter().any(|m| m.uzytkownik.as_deref() == Some(DRUGI)));
}

/// Czy sciezka lezy w katalogu pamieci podrecznej przegladarki.
fn w_cache_przegladarki(p: &Path) -> bool {
    p.components().any(|c| {
        let n = c.as_os_str().to_string_lossy();
        ["Cache", "Code Cache", "GPUCache", "cache2"].contains(&n.as_ref())
    })
}

#[test]
fn zadna_regula_nie_lapie_dokumentow_pulpitu_ani_profilu_przegladarki() {
    let (_tmp, opis, srod) = sztuczne(true);
    let korzen = &opis.korzen;
    let anuluj = AtomicBool::new(false);
    let mut wszystkie = Vec::new();
    for r in KATALOG {
        for p in pliki_reguly(&srod, r, &anuluj) {
            assert!(!opis.chronione.contains(&p), "regula {} lapie chroniony plik {}", r.id, p.display());
            let wzgledna = p.strip_prefix(korzen).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            let czlony: Vec<&str> = wzgledna.split('/').collect();
            if czlony.first() == Some(&"Users") && czlony.len() > 2 {
                let w_profilu = czlony[2];
                assert!(
                    !["Documents", "Desktop", "Pictures", "Videos", "Music"].contains(&w_profilu),
                    "regula {} lapie {}",
                    r.id,
                    wzgledna
                );
            }
            let profil_przegladarki = ["User Data", "Firefox/Profiles", "Opera Software"].iter().any(|s| wzgledna.contains(s));
            if profil_przegladarki {
                assert!(w_cache_przegladarki(&p), "regula {} lapie profil przegladarki poza cache: {}", r.id, wzgledna);
            }
            wszystkie.push((r.id, p));
        }
    }
    assert!(wszystkie.len() > 40, "za malo plikow w regulach: {}", wszystkie.len());
    // Kazdy chroniony plik nadal jest na miejscu (modul tylko liczy).
    for p in &opis.chronione {
        assert!(p.exists(), "{}", p.display());
    }
    // Zaden plik nie jest liczony przez dwie reguly naraz.
    let mut widziane = HashMap::new();
    for (id, p) in &wszystkie {
        if let Some(inna) = widziane.insert(p.clone(), *id) {
            panic!("{} liczony przez {} i {}", p.display(), inna, id);
        }
    }
}

// --- Duplikaty ---

fn zapisz(p: &Path, tresc: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, tresc).unwrap();
}

fn tresc(n: usize, ziarno: u32) -> Vec<u8> {
    let mut x = ziarno.wrapping_mul(2_654_435_761).max(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

#[test]
fn duplikaty_wykrywane_i_nie_mylone_z_plikami_tego_samego_rozmiaru() {
    let tmp = tempfile::tempdir().unwrap();
    let k = tmp.path();
    let film = tresc(2 * 1024 * 1024 + 123, 1);
    zapisz(&k.join("Filmy/wakacje.mkv"), &film);
    zapisz(&k.join("Pobrane/wakacje (1).mkv"), &film);
    zapisz(&k.join("Pulpit/kopia/wakacje.mkv"), &film);
    // Ten sam rozmiar i ten sam poczatek (pierwsze 64 KB), inny ostatni bajt.
    let mut inny_koniec = film.clone();
    *inny_koniec.last_mut().unwrap() ^= 0xFF;
    zapisz(&k.join("Filmy/inny-koniec.mkv"), &inny_koniec);
    // Ten sam rozmiar, inny pierwszy bajt.
    let mut inny_poczatek = film.clone();
    inny_poczatek[0] ^= 0xFF;
    zapisz(&k.join("Filmy/inny-poczatek.mkv"), &inny_poczatek);
    // Ten sam rozmiar, roznica tuz za granica 64 KB.
    let mut za_prefiksem = film.clone();
    za_prefiksem[PREFIKS] ^= 0x01;
    zapisz(&k.join("Filmy/za-prefiksem.mkv"), &za_prefiksem);
    // Druga grupa.
    let paczka = tresc(3 * 1024 * 1024 + 1, 2);
    zapisz(&k.join("a/paczka.zip"), &paczka);
    zapisz(&k.join("b/paczka.zip"), &paczka);
    // Male duplikaty i dokladnie 1 MiB: ponizej progu.
    let male = tresc(500_000, 3);
    zapisz(&k.join("male1.txt"), &male);
    zapisz(&k.join("male2.txt"), &male);
    let rowno = tresc(MIN_ROZMIAR as usize, 4);
    zapisz(&k.join("rowno1.bin"), &rowno);
    zapisz(&k.join("rowno2.bin"), &rowno);
    // Twardy link to ten sam plik na dysku, nie duplikat.
    #[cfg(unix)]
    std::fs::hard_link(k.join("Filmy/wakacje.mkv"), k.join("Filmy/twardy-link.mkv")).unwrap();

    let anuluj = AtomicBool::new(false);
    let skan = skan::skanuj(k, &Opcje::default(), &anuluj, None).unwrap();
    let kandydaci = duplikaty::kandydaci_z_drzewa(&skan.drzewo);
    assert!(kandydaci.iter().all(|c| c.rozmiar > MIN_ROZMIAR));
    let w = duplikaty::znajdz(kandydaci, &anuluj);

    assert_eq!(w.grupy.len(), 2, "grupy: {:#?}", w.grupy);
    let nazwy = |g: &duplikaty::Grupa| -> Vec<String> {
        let mut v: Vec<String> = g
            .pliki
            .iter()
            .map(|p| Path::new(&p.sciezka).strip_prefix(k).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        v.sort();
        v
    };
    let filmy = w.grupy.iter().find(|g| g.rozmiar == film.len() as u64).expect("grupa filmow");
    let n = nazwy(filmy);
    assert_eq!(n.len(), 3, "{n:?}");
    assert!(n.contains(&"Pobrane/wakacje (1).mkv".to_string()) && n.contains(&"Pulpit/kopia/wakacje.mkv".to_string()));
    assert!(
        n.contains(&"Filmy/wakacje.mkv".to_string()) ^ n.contains(&"Filmy/twardy-link.mkv".to_string()),
        "twardy link nie moze byc osobnym duplikatem: {n:?}"
    );
    for obcy in ["Filmy/inny-koniec.mkv", "Filmy/inny-poczatek.mkv", "Filmy/za-prefiksem.mkv"] {
        assert!(w.grupy.iter().all(|g| !nazwy(g).contains(&obcy.to_string())), "{obcy} to nie duplikat");
    }
    let zip = w.grupy.iter().find(|g| g.rozmiar == paczka.len() as u64).expect("grupa zip");
    assert_eq!(nazwy(zip), vec!["a/paczka.zip".to_string(), "b/paczka.zip".to_string()]);
    // Zostaje jedna kopia: do odzyskania pozostale.
    assert_eq!(filmy.do_odzyskania, 2 * filmy.pliki[0].na_dysku);
    assert_eq!(w.do_odzyskania, filmy.do_odzyskania + zip.do_odzyskania);
    assert!(w.grupy[0].do_odzyskania >= w.grupy[1].do_odzyskania);
    // inny-poczatek odpada na drugim sicie: caly czytany nie jest.
    let pelne = 3 * film.len() as u64 + 2 * film.len() as u64 + 2 * paczka.len() as u64;
    assert!(w.czytane_bajty < pelne + 10 * PREFIKS as u64, "czytane {} B", w.czytane_bajty);
    assert_eq!(w.bledy, 0);
}

// --- Kategorie i stare pliki ---

#[test]
fn kategorie_po_katalogu_i_rozszerzeniu() {
    let tmp = tempfile::tempdir().unwrap();
    let k = tmp.path();
    zapisz(&k.join("Wideo/film.mp4"), &[0u8; 50_000]);
    zapisz(&k.join("Zdjecia/IMG_1.JPG"), &[0u8; 20_000]);
    zapisz(&k.join("Steam/steamapps/common/Gra/dane.bin"), &[0u8; 70_000]);
    zapisz(&k.join("Steam/steamapps/common/Gra/intro.mp4"), &[0u8; 10_000]);
    zapisz(&k.join("projekt/node_modules/lib/index.js"), &[0u8; 5_000]);
    zapisz(&k.join("projekt/src/main.rs"), &[0u8; 3_000]);
    zapisz(&k.join("Windows/System32/kernel32.dll"), &[0u8; 30_000]);
    zapisz(&k.join("Pobrane/ChromeSetup.exe"), &[0u8; 8_000]);
    zapisz(&k.join("Pobrane/Win11.iso"), &[0u8; 9_000]);
    zapisz(&k.join("Pobrane/program.exe"), &[0u8; 4_000]);
    zapisz(&k.join("Pobrane/zdjecia.zip"), &[0u8; 6_000]);
    zapisz(&k.join("Dokumenty/umowa.pdf"), &[0u8; 2_000]);
    zapisz(&k.join("pagefile.sys"), &[0u8; 1_000]);
    let skan = skan::skanuj(k, &Opcje::default(), &AtomicBool::new(false), None).unwrap();
    let w = kategorie::policz(&skan.drzewo);
    let m: HashMap<Kategoria, (u64, u64)> = w.iter().map(|x| (x.kategoria, (x.rozmiar, x.pliki))).collect();
    assert_eq!(m[&Kategoria::Wideo], (50_000, 1), "film w katalogu gry liczy sie jako gra");
    assert_eq!(m[&Kategoria::Gry], (80_000, 2));
    assert_eq!(m[&Kategoria::Obrazy], (20_000, 1));
    assert_eq!(m[&Kategoria::Kod], (8_000, 2));
    assert_eq!(m[&Kategoria::System], (31_000, 2));
    assert_eq!(m[&Kategoria::Instalatory], (17_000, 2));
    assert_eq!(m[&Kategoria::Archiwa], (6_000, 1));
    assert_eq!(m[&Kategoria::Dokumenty], (2_000, 1));
    assert_eq!(m[&Kategoria::Inne], (4_000, 1));
    let suma: u64 = w.iter().map(|x| x.rozmiar).sum();
    assert_eq!(suma, skan.suma(), "kazdy plik w dokladnie jednej kategorii");
    assert!(w.windows(2).all(|p| p[0].na_dysku >= p[1].na_dysku));
    let gry = w.iter().find(|x| x.kategoria == Kategoria::Gry).unwrap();
    assert!(gry.najwieksze[0].sciezka.as_deref().unwrap().ends_with("dane.bin"));
}

#[test]
fn stare_duze_pliki_po_dacie_zmiany_i_dostepu() {
    let tmp = tempfile::tempdir().unwrap();
    let k = tmp.path();
    przyklad::plik(&k.join("archiwum/stary-film.mkv"), 3_000_000, 400.0).unwrap();
    przyklad::plik(&k.join("archiwum/stara-gra.iso"), 2_000_000, 200.0).unwrap();
    przyklad::plik(&k.join("nowy-film.mkv"), 3_000_000, 2.0).unwrap();
    przyklad::plik(&k.join("maly-stary.txt"), 10_000, 400.0).unwrap();
    let skan = skan::skanuj(k, &Opcje::default(), &AtomicBool::new(false), None).unwrap();
    let teraz = srodowisko::teraz();
    let w = kategorie::stare_duze(&skan.drzewo, 1_000_000, 180, teraz, 10);
    let nazwy: Vec<&str> = w.iter().map(|p| Path::new(&p.sciezka).file_name().unwrap().to_str().unwrap()).collect();
    assert_eq!(nazwy, vec!["stary-film.mkv", "stara-gra.iso"]);
    assert!(w[0].dni >= 399 && w[0].otwarty.is_some());
    assert!(w.iter().all(|p| p.rozmiar >= 1_000_000));
    assert_eq!(kategorie::stare_duze(&skan.drzewo, 1_000_000, 180, teraz, 1).len(), 1);
}
