//! Szybka droga dla calego dysku NTFS: tablica plikow (MFT) czytana ciagiem z woluminu,
//! zamiast otwierania kazdego katalogu osobno. Na zimnym dysku to kilka tysiecy duzych
//! odczytow zamiast kilkuset tysiecy malych. Wymaga uprawnien administratora (otwarcie
//! `\\.\C:`); bez nich albo na innym systemie plikow skan idzie zwykla droga po katalogach.
//!
//! Co liczymy z rekordu pliku:
//! - rozmiar: glowny strumien danych ($DATA bez nazwy),
//! - miejsce na dysku: przydzial wszystkich strumieni $DATA (dla skompresowanych i rzadkich
//!   pole "naprawde zajete"), wiec pliki CompactOS i strumienie dodatkowe wychodza prawdziwie,
//! - plik zmieszczony w rekordzie MFT liczony jako jeden klaster (tak samo jak droga po katalogach),
//! - kazda nazwa pliku ($FILE_NAME poza nazwami DOS 8.3) to osobny wpis; druga i kolejne
//!   nazwy tego samego rekordu sa twardymi linkami i nie wchodza do sum,
//! - punkt reparse zastepujacy nazwe (dowiazanie, junction): liczony, nieprzegladany.
//!
//! Parser nie zna Windows: dostaje funkcje "czytaj bajty od pozycji", wiec test na Linuksie
//! sprawdza go na obrazie zrobionym przez mkfs.ntfs.

use super::drzewo::*;
use super::Statystyki;
use std::io;

/// Czyta dokladnie tyle bajtow, ile ma bufor, od podanej pozycji woluminu.
pub type Czytnik<'a> = dyn FnMut(u64, &mut [u8]) -> io::Result<()> + 'a;

const F_UZYTY: u8 = 1;
const F_KATALOG: u8 = 2;
const F_DOWIAZANIE: u8 = 4;
const F_REZYDENT: u8 = 8;

const ATR_INFORMACJE: u32 = 0x10;
const ATR_LISTA: u32 = 0x20;
const ATR_NAZWA: u32 = 0x30;
const ATR_DANE: u32 = 0x80;
const ATR_REPARSE: u32 = 0xC0;
const MASKA_NUMERU: u64 = 0xFFFF_FFFF_FFFF;
/// Rekordy 0..15 to pliki samego systemu plikow ($MFT, $LogFile, $Extend...): zwykle
/// wyliczanie katalogu tez ich nie pokazuje.
const PIERWSZY_ZWYKLY: u32 = 16;
const KORZEN: u32 = 5;
const PORCJA: u64 = 4 * 1024 * 1024;
const BLOK: u64 = 4096;

#[derive(Clone, Copy, Default)]
struct Rekord {
    mtime: i64,
    rozmiar: u64,
    na_dysku: u64,
    flagi: u8,
}

struct Nazwa {
    rodzic: u32,
    rekord: u32,
    od: u32,
    dl: u16,
}

pub struct Tablica {
    rekordy: Vec<Rekord>,
    nazwy: Vec<Nazwa>,
    tekst: Vec<u8>,
    klaster: u64,
}

fn u16_z(b: &[u8], od: usize) -> u16 {
    b.get(od..od + 2).map_or(0, |x| u16::from_le_bytes(x.try_into().unwrap()))
}
fn u32_z(b: &[u8], od: usize) -> u32 {
    b.get(od..od + 4).map_or(0, |x| u32::from_le_bytes(x.try_into().unwrap()))
}
fn u64_z(b: &[u8], od: usize) -> u64 {
    b.get(od..od + 8).map_or(0, |x| u64::from_le_bytes(x.try_into().unwrap()))
}

fn zly(co: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("MFT: {co}"))
}

/// Rekord konczy kazde 512 bajtow znacznikiem kontrolnym; prawdziwe bajty leza w tablicy
/// na poczatku rekordu. false = rekord zapisany tylko czesciowo albo uszkodzony.
fn napraw(rek: &mut [u8]) -> bool {
    let od = u16_z(rek, 4) as usize;
    let ile = u16_z(rek, 6) as usize;
    if ile < 2 || od + ile * 2 > rek.len() || (ile - 1) * 512 > rek.len() {
        return false;
    }
    let znak = [rek[od], rek[od + 1]];
    for i in 1..ile {
        let koniec = i * 512;
        if rek[koniec - 2..koniec] != znak {
            return false;
        }
        rek[koniec - 2] = rek[od + i * 2];
        rek[koniec - 1] = rek[od + i * 2 + 1];
    }
    true
}

fn atrybuty(rek: &[u8], f: &mut dyn FnMut(u32, &[u8])) {
    let mut od = u16_z(rek, 0x14) as usize;
    let uzyte = (u32_z(rek, 0x18) as usize).min(rek.len());
    while od + 16 <= uzyte {
        let typ = u32_z(rek, od);
        let dl = u32_z(rek, od + 4) as usize;
        if typ == u32::MAX || dl < 16 || od + dl > uzyte {
            break;
        }
        f(typ, &rek[od..od + dl]);
        od += dl;
    }
}

/// Tresc atrybutu trzymanego w rekordzie.
fn tresc(a: &[u8]) -> &[u8] {
    if a.get(8) != Some(&0) {
        return &[];
    }
    let od = u16_z(a, 20) as usize;
    let dl = u32_z(a, 16) as usize;
    a.get(od..od + dl).unwrap_or(&[])
}

/// Lista przebiegow (pierwszy klaster, liczba klastrow) atrybutu poza rekordem.
fn przebiegi(a: &[u8]) -> io::Result<Vec<(u64, u64)>> {
    let mut wynik = Vec::new();
    let mut p = u16_z(a, 32) as usize;
    let mut lcn = 0i64;
    while let Some(&naglowek) = a.get(p) {
        if naglowek == 0 {
            break;
        }
        let (n_dl, n_od) = ((naglowek & 15) as usize, (naglowek >> 4) as usize);
        let pola = a.get(p + 1..p + 1 + n_dl + n_od).ok_or_else(|| zly("urwana lista przebiegow"))?;
        if n_dl == 0 || n_dl > 8 || n_od == 0 || n_od > 8 {
            return Err(zly("przebieg bez polozenia"));
        }
        let mut dl = [0u8; 8];
        dl[..n_dl].copy_from_slice(&pola[..n_dl]);
        // Przesuniecie jest liczba ze znakiem: dopelniamy starszym bitem.
        let mut skok = if pola[n_dl + n_od - 1] & 0x80 != 0 { [0xFFu8; 8] } else { [0u8; 8] };
        skok[..n_od].copy_from_slice(&pola[n_dl..]);
        lcn += i64::from_le_bytes(skok);
        if lcn < 0 {
            return Err(zly("przebieg przed poczatkiem woluminu"));
        }
        wynik.push((lcn as u64, u64::from_le_bytes(dl)));
        p += 1 + n_dl + n_od;
    }
    Ok(wynik)
}

/// Odczyt dowolnego kawalka: pozycja i dlugosc wyrownane do 4096 (wymog woluminu).
fn kawalek(czytaj: &mut Czytnik, od: u64, dl: usize) -> io::Result<Vec<u8>> {
    let start = od / BLOK * BLOK;
    let koniec = (od + dl as u64).div_ceil(BLOK) * BLOK;
    let mut bufor = vec![0u8; (koniec - start) as usize];
    czytaj(start, &mut bufor)?;
    Ok(bufor[(od - start) as usize..][..dl].to_vec())
}

struct Uklad {
    klaster: u64,
    rekord: usize,
    /// Przebiegi samej tablicy MFT, po kolei.
    przebiegi: Vec<(u64, u64)>,
    /// Ile bajtow tablicy jest zapisanych.
    dlugosc: u64,
}

impl Uklad {
    fn pozycja(&self, numer: u64) -> Option<u64> {
        let mut zostalo = numer * self.rekord as u64;
        for &(lcn, ile) in &self.przebiegi {
            let bajty = ile * self.klaster;
            if zostalo + self.rekord as u64 <= bajty {
                return Some(lcn * self.klaster + zostalo);
            }
            zostalo = zostalo.checked_sub(bajty)?;
        }
        None
    }
}

/// Kawalki listy przebiegow samej tablicy: (pierwszy klaster wirtualny, przebiegi). Mocno
/// pofragmentowana tablica trzyma dalsze kawalki w innych rekordach z listy atrybutow.
#[derive(Default)]
struct Zbior {
    kawalki: Vec<(u64, Vec<(u64, u64)>)>,
    inne_rekordy: Vec<u64>,
    dlugosc: u64,
    blad: Option<io::Error>,
}

impl Zbior {
    fn dodaj(&mut self, typ: u32, a: &[u8], glowny: bool) {
        if typ == ATR_DANE && a.get(9) == Some(&0) && a.get(8) == Some(&1) {
            if u64_z(a, 16) == 0 {
                self.dlugosc = u64_z(a, 56);
            }
            match przebiegi(a) {
                Ok(p) => self.kawalki.push((u64_z(a, 16), p)),
                Err(e) => self.blad = Some(e),
            }
        } else if typ == ATR_LISTA && glowny {
            if a.get(8) != Some(&0) {
                self.blad = Some(zly("lista atrybutow poza rekordem"));
                return;
            }
            let t = tresc(a);
            let mut od = 0;
            while od + 26 <= t.len() {
                let dl = u16_z(t, od + 4) as usize;
                if dl < 26 {
                    break;
                }
                let numer = u64_z(t, od + 16) & MASKA_NUMERU;
                if u32_z(t, od) == ATR_DANE && t[od + 6] == 0 && numer != 0 && !self.inne_rekordy.contains(&numer) {
                    self.inne_rekordy.push(numer);
                }
                od += dl;
            }
        }
    }
}

/// Sektor startowy i rekord 0 ($MFT): rozmiary i polozenie calej tablicy.
fn uklad(czytaj: &mut Czytnik) -> io::Result<Uklad> {
    let start = kawalek(czytaj, 0, 512)?;
    if &start[3..11] != b"NTFS    " {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "to nie NTFS"));
    }
    let sektor = u16_z(&start, 11) as u64;
    let klaster = match start[13] {
        0 => return Err(zly("zerowy klaster")),
        n if n <= 0x80 => n as u64 * sektor,
        n => sektor << (256 - n as u32),
    };
    let rekord = match start[0x40] as i8 {
        n if n > 0 => n as u64 * klaster,
        n => 1u64 << (-(n as i32)).min(20),
    } as usize;
    if klaster == 0 || !(512..=65536).contains(&rekord) || rekord % 512 != 0 {
        return Err(zly("dziwny rozmiar klastra albo rekordu"));
    }
    let lcn = u64_z(&start, 0x30);
    let mut u = Uklad { klaster, rekord, przebiegi: vec![(lcn, (rekord as u64).div_ceil(klaster).max(1))], dlugosc: 0 };

    let mut zero = kawalek(czytaj, lcn * klaster, rekord)?;
    if &zero[..4] != b"FILE" || !napraw(&mut zero) {
        return Err(zly("rekord 0 nieczytelny"));
    }
    let mut z = Zbior::default();
    atrybuty(&zero, &mut |typ, a| z.dodaj(typ, a, true));
    let mut i = 0;
    while i < z.inne_rekordy.len() {
        // Rekord z dalszym kawalkiem lezy w czesci tablicy opisanej juz wczesniejszymi kawalkami.
        let numer = z.inne_rekordy[i];
        i += 1;
        z.kawalki.sort_by_key(|k| k.0);
        u.przebiegi = z.kawalki.iter().flat_map(|k| k.1.iter().copied()).collect();
        let poz = u.pozycja(numer).ok_or_else(|| zly("kawalek tablicy poza znanym zakresem"))?;
        let mut rek = kawalek(czytaj, poz, rekord)?;
        if &rek[..4] != b"FILE" || !napraw(&mut rek) {
            return Err(zly("rekord z kawalkiem tablicy nieczytelny"));
        }
        atrybuty(&rek, &mut |typ, a| z.dodaj(typ, a, false));
    }
    u.dlugosc = z.dlugosc;
    let Zbior { mut kawalki, blad, .. } = z;
    if let Some(e) = blad {
        return Err(e);
    }
    kawalki.sort_by_key(|k| k.0);
    if kawalki.first().map(|k| k.0) != Some(0) {
        return Err(zly("brak danych tablicy"));
    }
    // Kawalki musza sie stykac: kazdy zaczyna sie tam, gdzie skonczyl poprzedni.
    let mut vcn = 0u64;
    u.przebiegi.clear();
    for (od, p) in &kawalki {
        if *od != vcn {
            return Err(zly("dziura w tablicy"));
        }
        vcn += p.iter().map(|x| x.1).sum::<u64>();
        u.przebiegi.extend_from_slice(p);
    }
    if u.dlugosc == 0 || u.dlugosc > vcn * klaster {
        return Err(zly("dlugosc tablicy nie zgadza sie z przebiegami"));
    }
    Ok(u)
}

fn rozbierz(t: &mut Tablica, numer: usize, rek: &mut [u8]) {
    if &rek[..4] != b"FILE" {
        return;
    }
    let flagi = u16_z(rek, 0x16);
    if flagi & 1 == 0 || !napraw(rek) {
        return;
    }
    // Rekord dodatkowy (plik z wieloma atrybutami) dopisuje sie do swojego rekordu glownego.
    let baza = (u64_z(rek, 0x20) & MASKA_NUMERU) as usize;
    let cel = if baza == 0 { numer } else { baza };
    if cel >= t.rekordy.len() {
        return;
    }
    let mut r = t.rekordy[cel];
    if baza == 0 {
        r.flagi |= F_UZYTY | if flagi & 2 != 0 { F_KATALOG } else { 0 };
    }
    let (nazwy, tekst) = (&mut t.nazwy, &mut t.tekst);
    atrybuty(rek, &mut |typ, a| match typ {
        ATR_INFORMACJE => {
            let c = tresc(a);
            if c.len() >= 16 {
                r.mtime = (u64_z(c, 8) / 10_000_000) as i64 - 11_644_473_600;
            }
        }
        ATR_NAZWA => {
            let c = tresc(a);
            let znaki = *c.get(64).unwrap_or(&0) as usize;
            // Przestrzen 2 to sama nazwa DOS 8.3: ten sam wpis co nazwa dluga, nie drugi link.
            if c.get(65) == Some(&2) {
                return;
            }
            let Some(surowa) = c.get(66..66 + znaki * 2) else { return };
            let szer: Vec<u16> = surowa.chunks_exact(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect();
            let nazwa = String::from_utf16_lossy(&szer);
            let rodzic = u64_z(c, 0) & MASKA_NUMERU;
            if rodzic > u32::MAX as u64 || tekst.len() + nazwa.len() > u32::MAX as usize {
                return;
            }
            nazwy.push(Nazwa { rodzic: rodzic as u32, rekord: cel as u32, od: tekst.len() as u32, dl: nazwa.len() as u16 });
            tekst.extend_from_slice(nazwa.as_bytes());
        }
        ATR_DANE => {
            let bez_nazwy = a.get(9) == Some(&0);
            if a.get(8) == Some(&0) {
                if bez_nazwy {
                    r.rozmiar = u32_z(a, 16) as u64;
                    r.flagi |= F_REZYDENT;
                }
            } else if u64_z(a, 16) == 0 {
                // Rozmiary sa wazne tylko w pierwszym kawalku atrybutu. Skompresowany albo
                // rzadki ma dluzszy naglowek z polem "naprawde zajete".
                let scisniety = u16_z(a, 12) & 0x8001 != 0 && u16_z(a, 32) >= 72;
                r.na_dysku += if scisniety { u64_z(a, 64) } else { u64_z(a, 40) };
                if bez_nazwy {
                    r.rozmiar = u64_z(a, 48);
                }
            }
        }
        ATR_REPARSE => {
            if u32_z(tresc(a), 0) & 0x2000_0000 != 0 {
                r.flagi |= F_DOWIAZANIE;
            }
        }
        _ => {}
    });
    t.rekordy[cel] = r;
}

/// Czyta cala tablice. `postep(przeczytane rekordy, wszystkie)` zwraca false, zeby przerwac
/// (wynik to wtedy Ok(None)).
pub fn czytaj(czytaj: &mut Czytnik, postep: &mut dyn FnMut(u64, u64) -> bool) -> io::Result<Option<Tablica>> {
    let u = uklad(czytaj)?;
    if u.klaster % 512 != 0 || (u.rekord as u64 > u.klaster && u.rekord as u64 % u.klaster != 0) {
        return Err(zly("rekord nie miesci sie rowno w klastrach"));
    }
    let wszystkie = u.dlugosc / u.rekord as u64;
    if wszystkie > u32::MAX as u64 {
        return Err(zly("za duzo rekordow"));
    }
    let mut t = Tablica {
        rekordy: vec![Rekord::default(); wszystkie as usize],
        nazwy: Vec::with_capacity(wszystkie as usize),
        tekst: Vec::with_capacity(wszystkie as usize * 24),
        klaster: u.klaster,
    };
    let porcja = PORCJA.next_multiple_of(u.rekord as u64);
    let mut bufor = vec![0u8; porcja as usize];
    // Rekord wiekszy niz klaster moze lezec na styku przebiegow: reszta przechodzi dalej.
    let mut reszta: Vec<u8> = Vec::new();
    let mut numer = 0u64;
    let mut zostalo = u.dlugosc;
    'calosc: for &(lcn, ile) in &u.przebiegi {
        let mut poz = lcn * u.klaster;
        let mut w_przebiegu = (ile * u.klaster).min(zostalo);
        while w_przebiegu > 0 {
            let teraz = w_przebiegu.min(porcja);
            // Dlugosc tablicy nie musi byc wielokrotnoscia 4096: czytamy z zapasem do pelnego bloku.
            let czytane = teraz.div_ceil(BLOK).max(1) * BLOK;
            if bufor.len() < czytane as usize {
                bufor.resize(czytane as usize, 0);
            }
            czytaj(poz, &mut bufor[..czytane as usize])?;
            let mut dane = &mut bufor[..teraz as usize];
            if !reszta.is_empty() {
                let brak = (u.rekord - reszta.len()).min(dane.len());
                reszta.extend_from_slice(&dane[..brak]);
                dane = &mut dane[brak..];
                if reszta.len() == u.rekord {
                    rozbierz(&mut t, numer as usize, &mut reszta);
                    numer += 1;
                    reszta.clear();
                }
            }
            let mut kawalki = dane.chunks_exact_mut(u.rekord);
            for rek in &mut kawalki {
                rozbierz(&mut t, numer as usize, rek);
                numer += 1;
            }
            reszta.extend_from_slice(kawalki.into_remainder());
            poz += teraz;
            w_przebiegu -= teraz;
            zostalo -= teraz;
            if !postep(numer, wszystkie) {
                return Ok(None);
            }
            if zostalo == 0 {
                break 'calosc;
            }
        }
    }
    Ok(Some(t))
}

/// Uklada drzewo od katalogu glownego woluminu. Dzieci katalogu leza w arenie obok siebie,
/// tak jak po zwyklym skanie.
pub fn zbuduj(t: &Tablica, nazwa_korzenia: &[u8]) -> (Drzewo, Statystyki) {
    let n = t.rekordy.len();
    let uzyty = |r: u32| (r as usize) < n && t.rekordy[r as usize].flagi & F_UZYTY != 0;
    // Nazwy pogrupowane po rodzicu sortowaniem przez zliczanie: `od[r]..od[r + 1]` to dzieci r.
    let mut od = vec![0u32; n + 2];
    let wazna = |x: &Nazwa| x.rekord >= PIERWSZY_ZWYKLY && x.rekord != x.rodzic && uzyty(x.rekord) && uzyty(x.rodzic);
    for x in t.nazwy.iter().filter(|x| wazna(x)) {
        od[x.rodzic as usize + 2] += 1;
    }
    for i in 2..n + 2 {
        od[i] += od[i - 1];
    }
    let mut po_rodzicu = vec![0u32; od[n + 1] as usize];
    for (i, x) in t.nazwy.iter().enumerate().filter(|(_, x)| wazna(x)) {
        let miejsce = &mut od[x.rodzic as usize + 1];
        po_rodzicu[*miejsce as usize] = i as u32;
        *miejsce += 1;
    }

    let mut d = Drzewo::default();
    let mut s = Statystyki { katalogi: 1, ..Default::default() };
    let korzen = t.rekordy.get(KORZEN as usize).copied().unwrap_or_default();
    let dl = nazwa_korzenia.len().min(u16::MAX as usize);
    d.nazwy.extend_from_slice(&nazwa_korzenia[..dl]);
    d.wezly.push(Wezel {
        rozmiar: 0,
        na_dysku: 0,
        mtime: korzen.mtime,
        rodzic: BRAK,
        pierwsze_dziecko: BRAK,
        liczba_dzieci: 0,
        pliki: 0,
        nazwa_od: 0,
        nazwa_dl: dl as u16,
        flagi: RODZAJ_KATALOG,
    });
    let mut widziany = vec![false; n];
    if (KORZEN as usize) < n {
        widziany[KORZEN as usize] = true;
    }
    // Kolejka katalogow do rozwiniecia: (rekord, wezel). Wezly dopisywane sa na koniec areny,
    // wiec sama arena jest kolejka: idziemy po niej raz od poczatku.
    let mut rekord_wezla: Vec<u32> = vec![KORZEN];
    let mut i = 0usize;
    while i < d.wezly.len() {
        let rek = rekord_wezla[i];
        if rek == u32::MAX || rek as usize >= n {
            i += 1;
            continue;
        }
        let dzieci = &po_rodzicu[od[rek as usize] as usize..od[rek as usize + 1] as usize];
        if dzieci.is_empty() {
            i += 1;
            continue;
        }
        let pierwsze = d.wezly.len();
        // Arena ma numery 32-bitowe: wolumin z ponad 4 mld wpisow konczy sie tutaj.
        if pierwsze + dzieci.len() >= BRAK as usize || d.nazwy.len() > (u32::MAX / 2) as usize {
            break;
        }
        for &nr in dzieci {
            let x = &t.nazwy[nr as usize];
            let r = t.rekordy[x.rekord as usize];
            let pierwszy_raz = !std::mem::replace(&mut widziany[x.rekord as usize], true);
            let mut flagi = 0u8;
            let (rodzaj, rozmiar, na_dysku) = if r.flagi & F_DOWIAZANIE != 0 {
                s.pliki += 1;
                s.dowiazania += 1;
                (RODZAJ_DOWIAZANIE, 0, 0)
            } else if r.flagi & F_KATALOG != 0 {
                s.katalogi += 1;
                (RODZAJ_KATALOG, 0, 0)
            } else {
                s.pliki += 1;
                if !pierwszy_raz {
                    flagi |= FLAGA_DUPLIKAT;
                    s.twarde_linki_pominiete += 1;
                }
                let na_dysku = if r.na_dysku == 0 && r.rozmiar > 0 && r.flagi & F_REZYDENT != 0 {
                    t.klaster
                } else {
                    r.na_dysku
                };
                (RODZAJ_PLIK, r.rozmiar, na_dysku)
            };
            // Katalog rozwijamy raz; uszkodzony wolumin z petla nie zawiesi skanu.
            rekord_wezla.push(if rodzaj == RODZAJ_KATALOG && pierwszy_raz { x.rekord } else { u32::MAX });
            d.wezly.push(Wezel {
                rozmiar,
                na_dysku,
                mtime: r.mtime,
                rodzic: i as u32,
                pierwsze_dziecko: BRAK,
                liczba_dzieci: 0,
                pliki: u32::from(rodzaj != RODZAJ_KATALOG),
                nazwa_od: d.nazwy.len() as u32,
                nazwa_dl: x.dl,
                flagi: rodzaj | flagi,
            });
            d.nazwy.extend_from_slice(&t.tekst[x.od as usize..x.od as usize + x.dl as usize]);
        }
        d.wezly[i].pierwsze_dziecko = pierwsze as u32;
        d.wezly[i].liczba_dzieci = dzieci.len() as u32;
        i += 1;
    }
    d.zsumuj();
    (d, s)
}

#[cfg(windows)]
pub use okna::skanuj;

#[cfg(windows)]
mod okna {
    use super::*;
    use crate::skan::{Postep, Wynik, ODSTEP_POSTEPU_MS};
    use std::os::windows::fs::{FileExt, OpenOptionsExt};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use std::time::Instant;

    /// Litera dysku, gdy sciezka to sam katalog glowny (`C:\`, `C:`, `\\?\C:\`).
    fn litera(korzen: &Path) -> Option<char> {
        let s = korzen.to_str()?;
        let s = s.strip_prefix(r"\\?\").unwrap_or(s);
        let mut znaki = s.chars();
        let litera = znaki.next().filter(|c| c.is_ascii_alphabetic())?;
        (znaki.next() == Some(':') && matches!(znaki.as_str(), "" | "\\" | "/")).then_some(litera)
    }

    /// Skan calego dysku z tablicy plikow. None = ta droga nie jest dostepna (nie katalog glowny,
    /// brak uprawnien administratora, nie NTFS, nieczytelna tablica): skan idzie po katalogach.
    pub fn skanuj(
        korzen: &Path,
        anuluj: &AtomicBool,
        postep: Option<&(dyn Fn(Postep) + Sync)>,
        start: Instant,
    ) -> Option<Wynik> {
        let diag = std::env::var_os("LUZIK_DIAG").is_some();
        if std::env::var_os("LUZIK_BEZ_MFT").is_some() {
            return None;
        }
        let litera = litera(korzen)?;
        let wolumin = match std::fs::OpenOptions::new().read(true).share_mode(7).open(format!(r"\\.\{litera}:")) {
            Ok(w) => w,
            Err(e) => {
                if diag {
                    eprintln!("LUZIK_DIAG mft niedostepne: {e}");
                }
                return None;
            }
        };
        let mut czytnik = |mut od: u64, mut bufor: &mut [u8]| -> io::Result<()> {
            while !bufor.is_empty() {
                let n = wolumin.seek_read(bufor, od)?;
                if n == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                od += n as u64;
                let reszta = bufor;
                bufor = &mut reszta[n..];
            }
            Ok(())
        };
        let nazwa = korzen.to_string_lossy().into_owned();
        let mut ostatni = Instant::now();
        let mut krok = |gotowe: u64, wszystkie: u64| {
            if let Some(zglos) = postep {
                if ostatni.elapsed().as_millis() as u64 >= ODSTEP_POSTEPU_MS {
                    ostatni = Instant::now();
                    let czas = start.elapsed();
                    zglos(Postep {
                        pliki: gotowe,
                        pliki_na_s: (gotowe as f64 / czas.as_secs_f64().max(0.001)) as u64,
                        czas_ms: czas.as_millis() as u64,
                        biezacy: format!("{nazwa} (spis plików {} z {})", gotowe, wszystkie),
                        ..Default::default()
                    });
                }
            }
            !anuluj.load(Relaxed)
        };
        let tablica = match czytaj(&mut czytnik, &mut krok) {
            Ok(Some(t)) => t,
            Ok(None) => {
                // Przerwane w polowie tablicy: nie ma z czego zlozyc drzewa, zostaje sam korzen.
                let pusta = Tablica { rekordy: Vec::new(), nazwy: Vec::new(), tekst: Vec::new(), klaster: 0 };
                let (drzewo, mut statystyki) = zbuduj(&pusta, nazwa.as_bytes());
                statystyki.anulowano = true;
                statystyki.czas_ms = start.elapsed().as_millis() as u64;
                return Some(Wynik { drzewo, statystyki });
            }
            Err(e) => {
                if diag {
                    eprintln!("LUZIK_DIAG mft nieczytelne: {e}");
                }
                return None;
            }
        };
        let odczyt_ms = start.elapsed().as_millis();
        let (drzewo, mut statystyki) = zbuduj(&tablica, nazwa.as_bytes());
        statystyki.czas_ms = start.elapsed().as_millis() as u64;
        statystyki.anulowano = anuluj.load(Relaxed);
        if diag {
            eprintln!(
                "LUZIK_DIAG mft rekordy {} odczyt {} ms calosc {} ms",
                tablica.rekordy.len(),
                odczyt_ms,
                statystyki.czas_ms
            );
        }
        if let Some(zglos) = postep {
            let k = drzewo.korzen();
            zglos(Postep {
                pliki: statystyki.pliki,
                katalogi: statystyki.katalogi,
                bajty: k.map_or(0, |w| w.rozmiar),
                pliki_na_s: (statystyki.pliki as f64 / start.elapsed().as_secs_f64().max(0.001)) as u64,
                czas_ms: statystyki.czas_ms,
                biezacy: nazwa,
                koniec: true,
                ..Default::default()
            });
        }
        Some(Wynik { drzewo, statystyki })
    }
}

/// Test na prawdziwym NTFS: obraz z mkfs.ntfs wypelniony przez ntfs-3g (bez roota).
/// Gdy tych narzedzi nie ma, test sie pomija.
#[cfg(all(test, unix))]
mod testy {
    use super::*;
    use std::os::unix::fs::FileExt;
    use std::process::Command;

    fn obraz(katalog: &std::path::Path) -> Option<std::fs::File> {
        let skrypt = r#"
            set -e
            truncate -s 96M o.img
            mkfs.ntfs -F -Q -q o.img >/dev/null 2>&1
            mkdir m
            ntfs-3g -o no_def_opts o.img m >/dev/null 2>&1
            mkdir m/kat
            mkdir m/kat/pod
            head -c 300000 /dev/urandom > m/kat/duzy.bin
            echo maly > m/kat/pod/maly.txt
            ln m/kat/duzy.bin m/kat/pod/link.bin
            truncate -s 5M m/rzadki.bin
            head -c 70000 /dev/zero > "m/zażółć gęślą jaźń.dat"
            i=0; while [ $i -lt 400 ]; do echo $i > m/kat/p$i.txt; i=$((i+1)); done
            : > m/pusty.txt
            fusermount -u m
        "#;
        let ok = Command::new("sh").arg("-c").arg(skrypt).current_dir(katalog).status().ok()?.success();
        if !ok {
            let _ = Command::new("fusermount").arg("-u").arg(katalog.join("m")).status();
            return None;
        }
        std::fs::File::open(katalog.join("o.img")).ok()
    }

    #[test]
    fn drzewo_z_tablicy_plikow_na_prawdziwym_ntfs() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(plik) = obraz(tmp.path()) else {
            eprintln!("pominiete: brak mkfs.ntfs albo ntfs-3g");
            return;
        };
        let mut czytnik = |od: u64, b: &mut [u8]| plik.read_exact_at(b, od);
        let t = czytaj(&mut czytnik, &mut |_, _| true).unwrap().unwrap();
        let (d, s) = zbuduj(&t, b"X:\\");

        let znajdz = |sciezka: &str| -> Wezel {
            let mut id = 0u32;
            for czesc in sciezka.split('/') {
                let w = d.wezly[id as usize];
                id = (w.pierwsze_dziecko..w.pierwsze_dziecko + w.liczba_dzieci)
                    .find(|&i| {
                        let x = d.wezly[i as usize];
                        &d.nazwy[x.nazwa_od as usize..][..x.nazwa_dl as usize] == czesc.as_bytes()
                    })
                    .unwrap_or_else(|| panic!("brak {czesc} w {sciezka}"));
            }
            d.wezly[id as usize]
        };
        let duzy = znajdz("kat/duzy.bin");
        assert_eq!(duzy.rozmiar, 300_000);
        assert_eq!(duzy.na_dysku, 300_000u64.div_ceil(t.klaster) * t.klaster);
        let link = znajdz("kat/pod/link.bin");
        assert_eq!(link.rozmiar, 300_000);
        assert!(duzy.duplikat() != link.duplikat(), "dokladnie jedna z dwoch nazw jest duplikatem");
        // Maly plik siedzi w rekordzie MFT: liczony jako klaster.
        let maly = znajdz("kat/pod/maly.txt");
        assert_eq!((maly.rozmiar, maly.na_dysku), (5, t.klaster));
        let rzadki = znajdz("rzadki.bin");
        assert_eq!(rzadki.rozmiar, 5 * 1024 * 1024);
        assert!(rzadki.na_dysku < 64 * 1024, "rzadki plik nie zajmuje 5 MB: {}", rzadki.na_dysku);
        assert_eq!(znajdz("zażółć gęślą jaźń.dat").rozmiar, 70_000);
        let pusty = znajdz("pusty.txt");
        assert_eq!((pusty.rozmiar, pusty.na_dysku), (0, 0));
        let kat = znajdz("kat");
        assert!(kat.katalog());
        assert_eq!(kat.liczba_dzieci, 402);
        // 400 malych, duzy, maly, link, rzadki, zazolc, pusty. Dowiazan ntfs-3g nie robi jako
        // punktow reparse, wiec te droge sprawdza dopiero porownanie obu skanow na Windows.
        assert_eq!(s.pliki, 406);
        assert_eq!(s.katalogi, 3);
        assert_eq!(s.dowiazania, 0);
        assert_eq!(s.twarde_linki_pominiete, 1);
        let k = d.korzen().unwrap();
        assert_eq!(k.pliki, 405, "druga nazwa twardego linku nie wchodzi do sumy");
        let male: u64 = (0..400u32).map(|i| i.to_string().len() as u64 + 1).sum();
        assert_eq!(k.rozmiar, 300_000 + 5 + 5 * 1024 * 1024 + 70_000 + male);
    }

    #[test]
    fn zwykly_plik_to_nie_ntfs() {
        let mut czytnik = |_: u64, b: &mut [u8]| {
            b.fill(0);
            Ok(())
        };
        assert!(czytaj(&mut czytnik, &mut |_, _| true).is_err());
    }
}
