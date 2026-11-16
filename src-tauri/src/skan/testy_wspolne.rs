//! Testy skanera, ktore ida na Linuksie i na Windows. Na Windows sprawdzaja wyliczanie
//! katalogu hurtem (GetFileInformationByHandleEx) wobec starej drogi: metadane i pomiar
//! kazdego pliku osobno przez `platforma::info`.

use super::drzewo::*;
use super::platforma;
use super::*;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn skan(p: &Path) -> Wynik {
    skanuj(p, &Opcje::default(), &AtomicBool::new(false), None).expect("skan")
}

fn plik(p: &Path, bajty: usize) {
    fs::write(p, vec![7u8; bajty]).unwrap();
}

/// Suma miejsca na dysku liczona stara droga: kazdy plik osobno, twardy link raz.
fn na_dysku_po_kolei(k: &Path, w: &platforma::Wolumin, linki: &mut HashSet<(u64, u64)>) -> (u64, u64) {
    let (mut rozmiar, mut na_dysku) = (0, 0);
    for wpis in fs::read_dir(k).unwrap() {
        let wpis = wpis.unwrap();
        let m = fs::symlink_metadata(wpis.path()).unwrap();
        let info = platforma::info(&m, &|| wpis.path(), w);
        if info.rodzaj == RODZAJ_KATALOG {
            let (r, d) = na_dysku_po_kolei(&wpis.path(), w, linki);
            rozmiar += r;
            na_dysku += d;
        } else if info.klucz_linku.is_none_or(|kl| linki.insert(kl)) {
            rozmiar += info.rozmiar;
            na_dysku += info.na_dysku;
        }
    }
    (rozmiar, na_dysku)
}

#[test]
fn wyliczanie_hurtem_zgadza_sie_z_pomiarem_kazdego_pliku() {
    let t = tempfile::tempdir().unwrap();
    let k = t.path();
    fs::create_dir_all(k.join("a/b/c")).unwrap();
    fs::create_dir_all(k.join("pusty")).unwrap();
    fs::create_dir_all(k.join("z spacja")).unwrap();
    plik(&k.join("jeden.bin"), 1);
    plik(&k.join("a/b/duzy.bin"), 300_000);
    plik(&k.join("a/b/c/zero"), 0);
    plik(&k.join("z spacja/zażółć.txt"), 12_345);
    // Ponad 64 KB nazw w jednym katalogu: kilka porcji wyliczania.
    for i in 0..1500 {
        plik(&k.join(format!("a/maly-plik-o-dlugiej-nazwie-{i:05}")), i * 7);
    }
    fs::hard_link(k.join("a/b/duzy.bin"), k.join("druga-nazwa.bin")).unwrap();
    fs::hard_link(k.join("a/b/duzy.bin"), k.join("pusty/trzecia-nazwa.bin")).unwrap();

    let w = skan(k);
    let s = &w.statystyki;
    assert_eq!(s.bledy_razem, 0, "{:?}", s.bledy);
    assert_eq!(s.pliki, 1 + 1 + 1 + 1 + 1500 + 2);
    assert_eq!(s.katalogi, 1 + 5);
    assert_eq!(s.twarde_linki_pominiete, 2, "trzy nazwy duzy.bin, liczony raz");
    let logiczny: u64 = 1 + 300_000 + 12_345 + (0..1500u64).map(|i| i * 7).sum::<u64>();
    assert_eq!(w.suma(), logiczny);

    let meta = fs::metadata(k).unwrap();
    let wol = platforma::wolumin(k, Some(&meta));
    let (r, d) = na_dysku_po_kolei(k, &wol, &mut HashSet::new());
    assert_eq!(r, logiczny);
    assert_eq!(w.na_dysku(), d, "miejsce na dysku jak przy pomiarze kazdego pliku osobno");
    assert!(w.na_dysku() >= 300_000);

    // Nazwy z wyliczania: polskie znaki, spacja, sciezka do pliku.
    let d = &w.drzewo;
    let id = d.znajdz(&k.join("z spacja").join("zażółć.txt").to_string_lossy()).expect("plik z polska nazwa");
    assert_eq!(d.wezel(id).unwrap().rozmiar, 12_345);
    let a = d.znajdz(&k.join("a").to_string_lossy()).expect("katalog a");
    assert_eq!(d.id_dzieci(a).len(), 1500 + 1);
    assert!(d.dzieci(0).iter().all(|x| x.nazwa != "." && x.nazwa != ".."));
}

