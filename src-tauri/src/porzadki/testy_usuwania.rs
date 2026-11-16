//! Testy usuwania: twarda blokada sciezek chronionych (napisy jak na Windows i sztuczne C:\
//! na dysku), usuwanie na stale i do Kosza na katalogu tymczasowym, pomijanie zablokowanych
//! plikow, poprawianie drzewa skanu bez ponownego skanu, historia i narzedzia regul.

use super::narzedzia::{self, Akcja};
use super::ochrona::{klucz, Ochrona};
use super::przyklad::{self, BIEZACY};
use super::usuwanie::{self, Kosz, Tryb, Usuwacz, WpisHistorii};
use super::*;
use crate::skan::{self, Opcje};
use std::fs;
use std::path::{Path, PathBuf};

/// Ochrona prawdziwego ukladu Windows zbudowana z napisow (bez dysku).
fn ochrona_windows() -> Ochrona {
    let c = PathBuf::from(r"C:\");
    let srod = Srodowisko {
        dysk_systemowy: c.clone(),
        windows: PathBuf::from(r"C:\Windows"),
        program_data: PathBuf::from(r"C:\ProgramData"),
        program_files: PathBuf::from(r"C:\Program Files"),
        program_files_x86: PathBuf::from(r"C:\Program Files (x86)"),
        dyski: vec![c, PathBuf::from(r"D:\")],
        uzytkownicy: vec![Uzytkownik::z_profilu(Path::new(r"C:\Users\Jan"), true)],
        admin: true,
        teraz: srodowisko::teraz(),
    };
    let mut o = Ochrona::windows(&srod);
    o.dodaj_miejsce_reguly(Path::new(r"C:\Users\Jan\AppData\Local\Temp"));
    o
}

fn odrzucona(o: &Ochrona, sciezka: &str, katalog: bool) -> String {
    match o.sprawdz(sciezka, katalog) {
        Err(p) => p,
        Ok(()) => panic!("{sciezka} powinna byc zablokowana"),
    }
}

#[test]
fn blokada_odrzuca_system32_i_korzen_dysku() {
    let o = ochrona_windows();
    for s in [r"C:\Windows\System32", r"C:\Windows\System32\drivers\etc\hosts", r"C:\Windows"] {
        assert!(odrzucona(&o, s, true).contains("Windows"), "{s}");
    }
    for s in [r"C:\", "C:", r"D:\", "c:/", r"\\serwer\udzial", r"\\?\C:\"] {
        assert!(odrzucona(&o, s, true).contains("korzeń"), "{s}");
    }
}

#[test]
fn blokada_nie_daje_sie_oszukac_zapisem_sciezki() {
    let o = ochrona_windows();
    for s in [
        r"c:\windows\system32",
        "C:/Windows/System32",
        r"C:\WINDOWS.\System32",
        r"C:\Windows \System32",
        r"\\?\C:\Windows\System32",
        r"C:\\Windows\\\System32",
        r"C:\Windows\.\System32",
        r"C:\Windows\Temp\..\System32",
        r"C:\Users\Jan\AppData\Local\Temp\..\..\..\..\..\Windows",
        r"C:Windows\System32",
        r"Windows\System32",
        r"C:\Windows\System32\config:SAM",
        r"C:\Windows\System3*",
    ] {
        odrzucona(&o, s, false);
    }
}

#[test]
fn blokada_program_files_profile_i_wyjatki_windows() {
    let o = ochrona_windows();
    for (s, katalog) in [
        (r"C:\Program Files", true),
        (r"C:\Program Files\Gra\gra.exe", false),
        (r"C:\Program Files (x86)\Steam\steam.exe", false),
        (r"C:\Users", true),
        (r"C:\Users\Jan", true),
        (r"C:\Users\Ania", true),
        (r"C:\Users\Jan\Documents", true),
        (r"C:\Users\Jan\AppData\Local", true),
        (r"C:\ProgramData", true),
        (r"C:\Windows\Temp", true),
        (r"C:\Windows\SoftwareDistribution", true),
        (r"C:\Windows\SoftwareDistribution\Download", true),
    ] {
        odrzucona(&o, s, katalog);
    }
    for (s, katalog) in [
        (r"C:\Windows\Temp\setup.log", false),
        (r"C:\Windows\Temp\stary", true),
        (r"C:\Windows\SoftwareDistribution\Download\abc123\x.cab", false),
        (r"C:\Users\Jan\Documents\film.mkv", false),
        (r"C:\Users\Jan\Downloads\instalator.exe", false),
        (r"C:\Users\Jan\AppData\Local\Temp\folder", true),
        (r"D:\Filmy\stary.mkv", false),
    ] {
        assert_eq!(o.sprawdz(s, katalog), Ok(()), "{s}");
    }
    assert_eq!(klucz(r"\\?\C:\Windows").unwrap(), klucz(r"c:\windows.").unwrap());
}

#[test]
fn blokada_w_usuwaniu_nic_nie_rusza_na_sztucznym_c() {
    let tmp = tempfile::tempdir().unwrap();
    let c = tmp.path().join("C");
    przyklad::zbuduj(&c).unwrap();
    let srod = Srodowisko::sztuczne(&c, Some(BIEZACY), true);
    let s32 = c.join("Windows").join("System32");
    przyklad::plik(&s32.join("kernel32.dll"), 4096, 10.0).unwrap();
    przyklad::plik(&c.join("Program Files").join("Gra").join("gra.exe"), 4096, 10.0).unwrap();
    let profil = c.join("Users").join(BIEZACY);
    przyklad::plik(&profil.join("Documents").join("praca.docx"), 2048, 10.0).unwrap();
    przyklad::plik(&c.join("Windows").join("Temp").join("smiec.tmp"), 1000, 10.0).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&s32, profil.join("skrot")).unwrap();

    let ochrona = Ochrona::windows(&srod);
    let usuwacz = Usuwacz { ochrona: &ochrona, kosz: Kosz::Katalog(tmp.path().join("kosz")) };
    let chronione = vec![
        c.clone(),
        c.join("Windows"),
        s32.clone(),
        s32.join("kernel32.dll"),
        c.join("Program Files").join("Gra"),
        c.join("Users"),
        profil.clone(),
        profil.join("Documents"),
        profil.join("skrot").join("kernel32.dll"),
    ];
    for tryb in [Tryb::Trwale, Tryb::Kosz] {
        let w = usuwacz.usun(&chronione, tryb);
        assert_eq!(w.pozycje, 0, "{tryb:?}: {:?}", w.pominiete);
        assert_eq!(w.na_dysku, 0);
        assert_eq!(w.pominiete_razem as usize, chronione.len());
        assert!(w.pominiete.iter().all(|p| p.powod.starts_with("Chroniony")), "{:?}", w.pominiete);
    }
    for p in &chronione[..8] {
        assert!(p.exists(), "{} zniknal", p.display());
    }
    assert!(s32.join("kernel32.dll").exists());

    // Pliki poza blokada przechodza.
    let wolne = vec![c.join("Windows").join("Temp").join("smiec.tmp"), profil.join("Documents").join("praca.docx")];
    let w = usuwacz.usun(&wolne, Tryb::Trwale);
    assert_eq!(w.pozycje, 2, "{:?}", w.pominiete);
    assert!(wolne.iter().all(|p| !p.exists()));
}

#[cfg(unix)]
fn jestem_rootem() -> bool {
    // SAFETY: funkcja bez argumentow.
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn usuwanie_trwale_liczy_bajty_i_pomija_zablokowane_z_powodem() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    przyklad::plik(&a.join("jeden.bin"), 10_000, 1.0).unwrap();
    przyklad::plik(&a.join("x").join("dwa.bin"), 20_000, 1.0).unwrap();
    przyklad::plik(&a.join("zamkniety").join("trzy.bin"), 5_000, 1.0).unwrap();
    przyklad::plik(&tmp.path().join("osobny.bin"), 3_000, 1.0).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(a.join("zamkniety"), fs::Permissions::from_mode(0o555)).unwrap();
    }
    let ochrona = Ochrona::default();
    let usuwacz = Usuwacz { ochrona: &ochrona, kosz: Kosz::Katalog(tmp.path().join("kosz")) };
    let w = usuwacz.usun(&[a.clone(), tmp.path().join("osobny.bin"), tmp.path().join("nie-ma")], Tryb::Trwale);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(a.join("zamkniety"), fs::Permissions::from_mode(0o755));
        if !jestem_rootem() {
            // Plik bez prawa usuniecia zostaje, reszta folderu znika.
            assert!(a.join("zamkniety").join("trzy.bin").exists());
            assert!(!a.join("jeden.bin").exists() && !a.join("x").exists());
            assert_eq!(w.rozmiar, 33_000, "{:?}", w.pominiete);
            assert_eq!(w.pliki, 3);
            assert_eq!(w.pozycje, 1, "tylko osobny.bin w calosci");
            assert!(w.pominiete.iter().any(|p| p.sciezka.ends_with("trzy.bin") && p.powod.contains("uprawnie")), "{:?}", w.pominiete);
        }
    }
    assert!(w.pominiete.iter().any(|p| p.sciezka.ends_with("nie-ma") && p.powod == "Już nie istnieje."));
    assert!(!tmp.path().join("osobny.bin").exists());
    assert!(w.na_dysku > 0);
}

#[test]
fn usuwanie_do_kosza_przenosi_z_informacja_o_miejscu() {
    let tmp = tempfile::tempdir().unwrap();
    let kosz = tmp.path().join("kosz");
    let plik = tmp.path().join("dane").join("film.mkv");
    let folder = tmp.path().join("dane").join("zdjecia");
    przyklad::plik(&plik, 50_000, 1.0).unwrap();
    przyklad::plik(&folder.join("a.jpg"), 7_000, 1.0).unwrap();
    przyklad::plik(&folder.join("b.jpg"), 8_000, 1.0).unwrap();
    // Drugi plik o tej samej nazwie nie nadpisuje pierwszego w Koszu.
    let drugi = tmp.path().join("inne").join("film.mkv");
    przyklad::plik(&drugi, 1_000, 1.0).unwrap();

    let ochrona = Ochrona::default();
    let usuwacz = Usuwacz { ochrona: &ochrona, kosz: Kosz::Katalog(kosz.clone()) };
    let w = usuwacz.usun(&[plik.clone(), folder.clone(), drugi.clone()], Tryb::Kosz);
    assert_eq!(w.pozycje, 3, "{:?}", w.pominiete);
    assert_eq!(w.pliki, 4);
    assert_eq!(w.rozmiar, 66_000);
    assert!(!plik.exists() && !folder.exists() && !drugi.exists());
    assert_eq!(fs::read(kosz.join("files").join("film.mkv")).unwrap().len(), 50_000);
    assert_eq!(fs::read(kosz.join("files").join("film.mkv.2")).unwrap().len(), 1_000);
    assert!(kosz.join("files").join("zdjecia").join("b.jpg").exists());
    let info = fs::read_to_string(kosz.join("info").join("film.mkv.trashinfo")).unwrap();
    assert!(info.contains(&format!("Path={}", plik.display())), "{info}");
    assert!(info.contains("DeletionDate=20"), "{info}");
}

#[test]
fn drzewo_po_usunieciu_bez_ponownego_skanu() {
    let tmp = tempfile::tempdir().unwrap();
    let k = tmp.path().join("skan");
    przyklad::plik(&k.join("duzy.iso"), 300_000, 1.0).unwrap();
    przyklad::plik(&k.join("gry").join("a.pak"), 120_000, 1.0).unwrap();
    przyklad::plik(&k.join("gry").join("b.pak"), 80_000, 1.0).unwrap();
    przyklad::plik(&k.join("gry").join("zapisy").join("s1.sav"), 9_000, 1.0).unwrap();
    przyklad::plik(&k.join("muzyka").join("x.mp3"), 40_000, 1.0).unwrap();
    let anuluj = std::sync::atomic::AtomicBool::new(false);
    let mut d = skan::skanuj(&k, &Opcje::default(), &anuluj, None).unwrap().drzewo;
    let przed = *d.korzen().unwrap();

    let ochrona = Ochrona::default();
    let usuwacz = Usuwacz { ochrona: &ochrona, kosz: Kosz::Katalog(tmp.path().join("kosz")) };
    let w = usuwacz.usun(&[k.join("duzy.iso"), k.join("gry").join("zapisy")], Tryb::Trwale);
    assert_eq!(w.pozycje, 2, "{:?}", w.pominiete);
    for s in &w.usuniete {
        d.usun_sciezke(s);
    }
    let po = *d.korzen().unwrap();
    // Roznica to usuniete pliki plus wlasny rozmiar usunietego folderu (du tez go liczy).
    assert!(przed.rozmiar - po.rozmiar >= w.rozmiar);
    assert_eq!(w.rozmiar, 309_000);
    assert_eq!(po.pliki, przed.pliki - 2);

    // Drzewo po poprawce = drzewo z nowego skanu.
    let nowe = skan::skanuj(&k, &Opcje::default(), &anuluj, None).unwrap().drzewo;
    assert_eq!(po.rozmiar, nowe.korzen().unwrap().rozmiar);
    assert_eq!(po.na_dysku, nowe.korzen().unwrap().na_dysku);
    let gry = d.znajdz(&k.join("gry").to_string_lossy()).unwrap();
    let gry_nowe = nowe.znajdz(&k.join("gry").to_string_lossy()).unwrap();
    assert_eq!(d.wezel(gry).unwrap().rozmiar, nowe.wezel(gry_nowe).unwrap().rozmiar);
    assert_eq!(d.wiersz(gry, false).unwrap().liczba_dzieci, 2);

    let nazwy: Vec<String> = d.dzieci(0).into_iter().map(|w| w.nazwa).collect();
    assert!(!nazwy.contains(&"duzy.iso".to_string()), "{nazwy:?}");
    assert!(d.najwieksze_pliki(10).iter().all(|w| w.nazwa != "duzy.iso" && w.nazwa != "s1.sav"));
    assert!(d.znajdz(&k.join("duzy.iso").to_string_lossy()).is_none());
    let kat: u64 = kategorie::policz(&d).iter().map(|k| k.rozmiar).sum();
    assert_eq!(kat, 240_000);
    assert!(!d.usun(0), "korzenia nie usuwamy");
}

#[test]
fn usuwanie_regul_na_sztucznym_c() {
    let tmp = tempfile::tempdir().unwrap();
    let c = tmp.path().join("C");
    przyklad::zbuduj(&c).unwrap();
    let srod = Srodowisko::sztuczne(&c, Some(BIEZACY), false);
    let anuluj = std::sync::atomic::AtomicBool::new(false);
    let przed = skanuj(&srod, &anuluj);
    let temp = przed.iter().find(|z| z.id == "temp-uzytkownika").expect("temp-uzytkownika");
    let dokumenty: Vec<PathBuf> = fs::read_dir(c.join("Users").join(BIEZACY).join("Documents"))
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();

    let ochrona = Ochrona::windows(&srod);
    let usuwacz = Usuwacz { ochrona: &ochrona, kosz: Kosz::Katalog(tmp.path().join("kosz")) };
    let ids: Vec<String> = ["temp-uzytkownika", "plik-wymiany", "hibernacja"].map(String::from).to_vec();
    let w = usun_reguly(&srod, &ids, Tryb::Trwale, &usuwacz, &anuluj);
    assert_eq!(w.rozmiar, temp.rozmiar, "{:?}", w.pominiete);
    assert_eq!(w.pliki, temp.pliki);
    assert_eq!(w.pozycje, 1);
    assert_eq!(w.pominiete_razem, 2, "{:?}", w.pominiete);

    let po = skanuj(&srod, &anuluj);
    assert!(po.iter().all(|z| z.id != "temp-uzytkownika"), "temp powinien byc pusty");
    // Sam folder Temp zostaje, dokumenty nietkniete.
    assert!(c.join("Users").join(BIEZACY).join("AppData").join("Local").join("Temp").is_dir());
    assert!(dokumenty.iter().all(|p| p.exists()));
}

#[test]
fn historia_dopisuje_na_koniec_i_obcina() {
    let tmp = tempfile::tempdir().unwrap();
    let plik = tmp.path().join("AppData").join("historia.json");
    let wpis = |n: u64| WpisHistorii {
        czas: n as i64,
        rodzaj: "usuwanie".into(),
        opis: format!("wpis {n}"),
        tryb: Some(Tryb::Kosz),
        pozycje: n,
        pliki: n,
        na_dysku: n * 1000,
        pominiete: 0,
    };
    assert!(usuwanie::wczytaj_historie(&plik).is_empty());
    for n in 0..(usuwanie::LIMIT_HISTORII as u64 + 3) {
        usuwanie::dopisz_historie(&plik, wpis(n)).unwrap();
    }
    let h = usuwanie::wczytaj_historie(&plik);
    assert_eq!(h.len(), usuwanie::LIMIT_HISTORII);
    assert_eq!(h.last().unwrap(), &wpis(usuwanie::LIMIT_HISTORII as u64 + 2));
    assert_eq!(h[0].czas, 3);
    let json = fs::read_to_string(&plik).unwrap();
    assert!(json.contains("\"tryb\": \"kosz\""), "{}", &json[..200]);
}

#[test]
fn narzedzia_regul_sa_na_sztywno_i_kompletne() {
    for r in KATALOG.iter().filter(|r| r.sposob == Sposob::Narzedzie && r.id != "onedrive") {
        let n = narzedzia::dla_reguly(r.id).unwrap_or_else(|| panic!("regula {} bez narzedzia", r.id));
        assert_eq!(n.admin, r.wymaga_admina, "{}", r.id);
        if let Akcja::Program { program, argumenty } = &n.akcja {
            let nazwa = program.rsplit('\\').next().unwrap().trim_end_matches(".exe").to_lowercase();
            let polecenie = r.polecenie.unwrap_or("").to_lowercase();
            assert!(polecenie.contains(&nazwa) || nazwa == "powershell", "{}: {program} a {polecenie}", r.id);
            assert!(!argumenty.is_empty());
        }
    }
    assert_eq!(narzedzia::dla_reguly("kosz").unwrap().akcja, Akcja::OproznijKosz);
    assert!(narzedzia::dla_reguly("onedrive").is_none());
    assert!(narzedzia::dla_reguly("calc").is_none());
    assert_eq!(
        narzedzia::polacz_argumenty(&["-Command".into(), "Delete-DeliveryOptimizationCache -Force".into()]),
        "-Command \"Delete-DeliveryOptimizationCache -Force\""
    );
}
