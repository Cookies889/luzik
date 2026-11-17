//! Testy skanera na drzewie tymczasowym. Wzorcem jest `du` z systemu.

use super::drzewo::*;
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

fn skan(p: &Path) -> Wynik {
    skanuj(p, &Opcje::default(), &AtomicBool::new(false), None).expect("skan")
}

/// Liczba z pierwszej kolumny `du`.
fn du(argumenty: &[&str], p: &Path) -> u64 {
    let wyjscie = Command::new("du").args(argumenty).arg(p).output().expect("du");
    String::from_utf8_lossy(&wyjscie.stdout).split_whitespace().next().expect("liczba z du").parse().unwrap()
}

fn plik(p: &Path, bajty: usize) {
    fs::write(p, vec![7u8; bajty]).unwrap();
}

/// Drzewo z kilkoma poziomami, pustym plikiem, pustym katalogiem i nazwa ze spacja.
fn drzewo_probne(k: &Path) {
    fs::create_dir_all(k.join("a/b/c")).unwrap();
    fs::create_dir_all(k.join("pusty")).unwrap();
    fs::create_dir_all(k.join("z spacja")).unwrap();
    plik(&k.join("jeden.bin"), 1);
    plik(&k.join("a/sto.bin"), 100);
    plik(&k.join("a/b/duzy.bin"), 300_000);
    plik(&k.join("a/b/c/gleboko.txt"), 4097);
    plik(&k.join("a/b/c/zero"), 0);
    plik(&k.join("z spacja/zażółć.txt"), 12_345);
    for i in 0..200 {
        plik(&k.join(format!("a/maly-{i}")), i * 13);
    }
}

#[test]
fn suma_zgadza_sie_z_du() {
    let t = tempfile::tempdir().unwrap();
    drzewo_probne(t.path());
    let w = skan(t.path());
    assert_eq!(w.suma(), du(&["-sb", "--apparent-size"], t.path()), "rozmiar logiczny jak du -sb");
    assert_eq!(w.na_dysku(), du(&["-s", "--block-size=1"], t.path()), "miejsce na dysku jak du -s");
    assert_eq!(w.statystyki.pliki, 206);
    assert_eq!(w.statystyki.katalogi, 6);
    assert_eq!(w.statystyki.bledy_razem, 0);
    assert_eq!(w.drzewo.liczba_wezlow(), 212);
}

#[test]
fn plik_rzadki_ma_maly_rozmiar_na_dysku() {
    let t = tempfile::tempdir().unwrap();
    let f = fs::File::create(t.path().join("rzadki")).unwrap();
    f.set_len(50_000_000).unwrap();
    drop(f);
    let w = skan(t.path());
    assert_eq!(w.suma(), 50_000_000);
    assert!(w.na_dysku() < 1_000_000, "na dysku {}", w.na_dysku());
    assert_eq!(w.na_dysku(), du(&["-s", "--block-size=1"], t.path()));
}

#[test]
fn twardy_link_liczony_raz() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("inny")).unwrap();
    plik(&t.path().join("oryginal"), 10_000);
    fs::hard_link(t.path().join("oryginal"), t.path().join("druga-nazwa")).unwrap();
    fs::hard_link(t.path().join("oryginal"), t.path().join("inny/trzecia-nazwa")).unwrap();
    let w = skan(t.path());
    assert_eq!(w.suma(), 10_000, "trzy nazwy, jeden plik");
    assert_eq!(w.suma(), du(&["-sb", "--apparent-size"], t.path()));
    assert_eq!(w.statystyki.twarde_linki_pominiete, 2);
    assert_eq!(w.statystyki.pliki, 3);
    assert_eq!(w.drzewo.najwieksze_pliki(10).len(), 1, "powtorki nie trafiaja na liste najwiekszych");
}

#[test]
fn dowiazanie_symboliczne_nie_jest_liczone_podwojnie() {
    let t = tempfile::tempdir().unwrap();
    let k = t.path();
    fs::create_dir(k.join("prawdziwy")).unwrap();
    plik(&k.join("prawdziwy/dane.bin"), 50_000);
    std::os::unix::fs::symlink(k.join("prawdziwy"), k.join("do-katalogu")).unwrap();
    std::os::unix::fs::symlink(k.join("prawdziwy/dane.bin"), k.join("do-pliku")).unwrap();
    std::os::unix::fs::symlink(k, k.join("petla")).unwrap();
    std::os::unix::fs::symlink("/nie/ma/takiego", k.join("zerwane")).unwrap();
    let w = skan(k);
    assert_eq!(w.statystyki.dowiazania, 4);
    assert_eq!(w.suma(), du(&["-sb", "--apparent-size"], k), "jak du: dane raz, plus dlugosc samych dowiazan");
    assert!(w.suma() < 60_000, "dane policzone raz, suma {}", w.suma());
    assert_eq!(w.statystyki.bledy_razem, 0, "zerwane dowiazanie to nie blad");
    let wiersze = w.drzewo.dzieci(0);
    let link = wiersze.iter().find(|x| x.nazwa == "do-katalogu").unwrap();
    assert_eq!(link.rodzaj, "dowiazanie");
    assert_eq!(link.liczba_dzieci, 0, "skan nie wchodzi w dowiazanie");
}

#[test]
fn brak_uprawnien_nie_wywala_skanu() {
    let t = tempfile::tempdir().unwrap();
    let k = t.path();
    fs::create_dir(k.join("zamkniety")).unwrap();
    plik(&k.join("zamkniety/tajne"), 999);
    fs::create_dir(k.join("otwarty")).unwrap();
    plik(&k.join("otwarty/jawne"), 2_000);
    fs::set_permissions(k.join("zamkniety"), fs::Permissions::from_mode(0o000)).unwrap();
    let w = skan(k);
    fs::set_permissions(k.join("zamkniety"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(w.suma(), 2_000, "to, co dostepne, jest policzone");
    assert_eq!(w.statystyki.bledy_razem, 1);
    assert_eq!(w.statystyki.bledy.len(), 1);
    assert!(w.statystyki.bledy[0].sciezka.ends_with("zamkniety"), "{:?}", w.statystyki.bledy);
    let zamkniety = w.drzewo.dzieci(0).into_iter().find(|x| x.nazwa == "zamkniety").unwrap();
    assert!(zamkniety.niedostepny);
}

#[test]
fn nieistniejaca_sciezka_to_blad_a_nie_panika() {
    assert!(skanuj(Path::new("/nie/ma/takiej/sciezki"), &Opcje::default(), &AtomicBool::new(false), None).is_err());
}

#[test]
fn skan_pojedynczego_pliku() {
    let t = tempfile::tempdir().unwrap();
    plik(&t.path().join("sam"), 777);
    let w = skan(&t.path().join("sam"));
    assert_eq!(w.suma(), 777);
    assert_eq!(w.statystyki.pliki, 1);
}

#[test]
fn anulowanie_konczy_skan_wczesniej() {
    let t = tempfile::tempdir().unwrap();
    drzewo_probne(t.path());
    let w = skanuj(t.path(), &Opcje::default(), &AtomicBool::new(true), None).unwrap();
    assert!(w.statystyki.anulowano);
    assert_eq!(w.drzewo.liczba_wezlow(), 1, "po anulowaniu zostaje sam korzen");
}

#[test]
fn postep_przychodzi_i_konczy_sie_stanem_koncowym() {
    let t = tempfile::tempdir().unwrap();
    drzewo_probne(t.path());
    let zebrane = std::sync::Mutex::new(Vec::new());
    let zglos = |p: Postep| zebrane.lock().unwrap().push(p);
    let w = skanuj(t.path(), &Opcje::default(), &AtomicBool::new(false), Some(&zglos)).unwrap();
    let zebrane = zebrane.into_inner().unwrap();
    let ostatni = zebrane.last().expect("co najmniej stan koncowy");
    assert!(ostatni.koniec);
    assert_eq!(ostatni.pliki, w.statystyki.pliki);
    assert_eq!(ostatni.bajty, w.suma());
    assert!(!ostatni.biezacy.is_empty());
}

#[test]
fn drzewo_dzieci_sciezki_i_najwieksze() {
    let t = tempfile::tempdir().unwrap();
    drzewo_probne(t.path());
    let w = skan(t.path());
    let d = &w.drzewo;
    let korzen = d.dzieci(0);
    assert_eq!(korzen.len(), 4);
    assert_eq!(korzen[0].nazwa, "a", "najwiekszy na gorze");
    assert_eq!(korzen[0].rodzaj, "katalog");
    assert_eq!(korzen[0].pliki, 204);
    assert!(korzen.windows(2).all(|p| p[0].na_dysku >= p[1].na_dysku));
    // Suma dzieci rowna sie rodzicowi na kazdym poziomie.
    for id in 0..d.liczba_wezlow() as u32 {
        let wz = d.wezel(id).unwrap();
        if wz.katalog() {
            let suma: u64 = d.id_dzieci(id).map(|i| d.wezel(i).unwrap().rozmiar).sum();
            assert_eq!(suma, wz.rozmiar, "wezel {id}");
        }
    }
    let top = d.najwieksze_pliki(3);
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].rozmiar, 300_000);
    let sciezka = top[0].sciezka.clone().unwrap();
    assert_eq!(Path::new(&sciezka), t.path().join("a/b/duzy.bin"));
    assert_eq!(top[1].nazwa, "zażółć.txt");
    assert!(d.dzieci(9_999_999).is_empty());
}

#[test]
fn wezel_jest_zwarty() {
    assert_eq!(std::mem::size_of::<Wezel>(), 48);
    // 2 mln plikow z nazwami po 40 bajtow: 176 MB, czyli z zapasem ponizej 300 MB.
    assert!(2_000_000 * (std::mem::size_of::<Wezel>() + 40) < 300 * 1024 * 1024);
}

#[test]
fn lista_dyskow_ma_dysk_z_korzeniem() {
    let dyski = lista_dyskow();
    assert!(!dyski.is_empty());
    assert!(dyski.iter().all(|d| d.rozmiar > 0 && d.wolne <= d.rozmiar && !d.system_plikow.is_empty()));
}

#[test]
fn znikniete_poza_luzikiem_wypadaja_z_wyniku() {
    let t = tempfile::tempdir().unwrap();
    drzewo_probne(t.path());
    let mut w = skan(t.path());
    let d = &mut w.drzewo;
    assert!(d.znikniete(0).is_empty(), "zaraz po skanie wszystko jest na dysku");
    let przed = d.korzen().unwrap().na_dysku;
    let a = d.znajdz(&t.path().join("a").to_string_lossy()).unwrap();
    let rozmiar_a = d.wezel(a).unwrap().na_dysku;
    // Znajomy kasuje folder i plik w Eksploratorze, Luzik dalej ma stary wynik.
    fs::remove_dir_all(t.path().join("a")).unwrap();
    fs::remove_file(t.path().join("jeden.bin")).unwrap();
    let mut z = d.znikniete(0);
    z.sort_unstable();
    assert_eq!(z.len(), 2, "folder a i plik jeden.bin");
    for id in z {
        assert!(d.usun(id));
    }
    let po = d.korzen().unwrap().na_dysku;
    assert!(przed - po >= rozmiar_a, "przed {przed}, po {po}, folder a {rozmiar_a}");
    assert!(d.znajdz(&t.path().join("a").to_string_lossy()).is_none());
    assert!(d.znikniete(0).is_empty(), "po odswiezeniu nic wiecej nie brakuje");
    // Wezel, ktory zniknal razem z rodzicem, zglasza tylko siebie.
    let c = d.znajdz(&t.path().join("z spacja").to_string_lossy()).unwrap();
    fs::remove_dir_all(t.path().join("z spacja")).unwrap();
    assert_eq!(d.znikniete(c), vec![c]);
}
