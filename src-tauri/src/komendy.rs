//! Komendy Tauri: cienka warstwa nad `skan`. Drzewo ostatniego skanu zostaje w pamieci
//! aplikacji, a okno pyta o kolejne poziomy po numerze wezla (korzen ma numer 0).

use crate::porzadki;
use crate::skan::{self, Blad, Drzewo, Dysk, Opcje, Postep, Wiersz};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

/// Nazwa zdarzenia z postepem skanu (tresc: `skan::Postep`), co 100 ms.
pub const ZDARZENIE_POSTEPU: &str = "skan-postep";
/// Ile sciezek z bledem dostepu wraca w podsumowaniu (licznik podaje wszystkie).
const BLEDY_W_PODSUMOWANIU: usize = 500;

#[derive(Default)]
pub struct Stan {
    drzewo: Mutex<Option<Arc<Drzewo>>>,
    anuluj: Arc<AtomicBool>,
    trwa: Arc<AtomicBool>,
}

impl Stan {
    fn drzewo(&self) -> Result<Arc<Drzewo>, String> {
        self.drzewo.lock().unwrap().clone().ok_or_else(|| "Najpierw uruchom skan.".to_string())
    }
}

#[derive(Serialize)]
pub struct Podsumowanie {
    pub korzen: Wiersz,
    pub pliki: u64,
    pub katalogi: u64,
    pub dowiazania: u64,
    pub twarde_linki_pominiete: u64,
    pub inne_systemy: u64,
    pub bledy_razem: u64,
    pub bledy: Vec<Blad>,
    pub czas_ms: u64,
    pub anulowano: bool,
    pub pamiec_drzewa: usize,
}

#[tauri::command]
pub fn lista_dyskow() -> Vec<Dysk> {
    skan::lista_dyskow()
}

#[tauri::command]
pub async fn skanuj(app: AppHandle, stan: State<'_, Stan>, sciezka: String) -> Result<Podsumowanie, String> {
    if stan.trwa.swap(true, Relaxed) {
        return Err("Skan juz trwa. Anuluj go albo poczekaj na koniec.".into());
    }
    stan.anuluj.store(false, Relaxed);
    let (anuluj, trwa) = (stan.anuluj.clone(), stan.trwa.clone());
    let wynik = tauri::async_runtime::spawn_blocking(move || {
        let zglos = |p: Postep| {
            let _ = app.emit(ZDARZENIE_POSTEPU, p);
        };
        let w = skan::skanuj(&PathBuf::from(&sciezka), &Opcje::default(), &anuluj, Some(&zglos));
        trwa.store(false, Relaxed);
        w
    })
    .await;
    stan.trwa.store(false, Relaxed);
    let wynik = wynik.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    let drzewo = Arc::new(wynik.drzewo);
    let s = wynik.statystyki;
    let mut bledy = s.bledy;
    bledy.truncate(BLEDY_W_PODSUMOWANIU);
    let podsumowanie = Podsumowanie {
        korzen: drzewo.wiersz(0, true).ok_or("Pusty wynik skanu.")?,
        pliki: s.pliki,
        katalogi: s.katalogi,
        dowiazania: s.dowiazania,
        twarde_linki_pominiete: s.twarde_linki_pominiete,
        inne_systemy: s.inne_systemy,
        bledy_razem: s.bledy_razem,
        bledy,
        czas_ms: s.czas_ms,
        anulowano: s.anulowano,
        pamiec_drzewa: drzewo.pamiec(),
    };
    *stan.drzewo.lock().unwrap() = Some(drzewo);
    Ok(podsumowanie)
}

#[tauri::command]
pub fn anuluj(stan: State<'_, Stan>) -> bool {
    stan.anuluj.store(true, Relaxed);
    stan.trwa.load(Relaxed)
}

#[tauri::command]
pub fn dzieci(stan: State<'_, Stan>, id_wezla: u32) -> Result<Vec<Wiersz>, String> {
    let drzewo = stan.drzewo()?;
    if drzewo.wezel(id_wezla).is_none() {
        return Err(format!("Nie ma wezla {id_wezla}."));
    }
    Ok(drzewo.dzieci(id_wezla))
}

#[tauri::command]
pub fn najwieksze_pliki(stan: State<'_, Stan>, n: usize) -> Result<Vec<Wiersz>, String> {
    Ok(stan.drzewo()?.najwieksze_pliki(n.min(10_000)))
}

// --- Porzadki: reguly Windows, duplikaty, kategorie, stare duze pliki ---

/// Zajmuje flage `trwa` na czas dlugiej pracy (skan, duplikaty) i zwalnia ja na koncu.
struct Zajete(Arc<AtomicBool>);

impl Zajete {
    fn wez(stan: &Stan) -> Result<Self, String> {
        if stan.trwa.swap(true, Relaxed) {
            return Err("Trwa inna praca na dysku. Anuluj ją albo poczekaj na koniec.".into());
        }
        stan.anuluj.store(false, Relaxed);
        Ok(Zajete(stan.trwa.clone()))
    }
}

impl Drop for Zajete {
    fn drop(&mut self) {
        self.0.store(false, Relaxed);
    }
}

/// Reguly porzadkow na tym komputerze: lista znalezisk z rozmiarem na dysku, poziomem,
/// sposobem usuniecia i wyjasnieniem. Nie wymaga wczesniejszego skanu.
#[tauri::command]
pub async fn porzadki_skanuj(stan: State<'_, Stan>) -> Result<Vec<porzadki::Znalezisko>, String> {
    let zajete = Zajete::wez(&stan)?;
    let anuluj = stan.anuluj.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _zajete = zajete;
        let srod = porzadki::Srodowisko::z_otoczenia()?;
        Ok(porzadki::skanuj(&srod, &anuluj))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Duplikaty wsrod plikow ostatniego skanu wiekszych niz 1 MB. Anulowanie: komenda `anuluj`.
#[tauri::command]
pub async fn duplikaty(stan: State<'_, Stan>) -> Result<porzadki::duplikaty::WynikDuplikatow, String> {
    let drzewo = stan.drzewo()?;
    let zajete = Zajete::wez(&stan)?;
    let anuluj = stan.anuluj.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _zajete = zajete;
        porzadki::duplikaty::znajdz(porzadki::duplikaty::kandydaci_z_drzewa(&drzewo), &anuluj)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Podzial ostatniego skanu na rodzaje plikow (wideo, gry, obrazy, archiwa, instalatory, kod, system).
#[tauri::command]
pub async fn kategorie(stan: State<'_, Stan>) -> Result<Vec<porzadki::kategorie::WynikKategorii>, String> {
    let drzewo = stan.drzewo()?;
    tauri::async_runtime::spawn_blocking(move || porzadki::kategorie::policz(&drzewo))
        .await
        .map_err(|e| e.to_string())
}

/// Duze pliki ostatniego skanu nieruszane od `dni` dni (domyslnie 100 MB i 180 dni, najwyzej 200).
#[tauri::command]
pub async fn stare_pliki(
    stan: State<'_, Stan>,
    min_mb: Option<u64>,
    dni: Option<u64>,
    n: Option<usize>,
) -> Result<Vec<porzadki::kategorie::StaryPlik>, String> {
    let drzewo = stan.drzewo()?;
    let (min, dni, n) = (min_mb.unwrap_or(100) * 1024 * 1024, dni.unwrap_or(180), n.unwrap_or(200).min(10_000));
    tauri::async_runtime::spawn_blocking(move || {
        porzadki::kategorie::stare_duze(&drzewo, min, dni, porzadki::srodowisko::teraz(), n)
    })
    .await
    .map_err(|e| e.to_string())
}

// --- Usuwanie, narzedzia Windows, historia, prawa administratora ---

use porzadki::usuwanie::{self, Tryb, WpisHistorii, WynikUsuwania};
use tauri::Manager;

/// Plik historii porzadkow w AppData aplikacji (Windows: %APPDATA%\pl.luzik.app).
fn plik_historii(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("historia-porzadkow.json"))
}

/// Poprawia drzewo ostatniego skanu o usuniete sciezki (bez ponownego skanu)
/// i wpisuje do wyniku nowy korzen.
fn popraw_drzewo(stan: &Stan, wynik: &mut WynikUsuwania) {
    let mut g = stan.drzewo.lock().unwrap();
    let Some(arc) = g.as_mut() else { return };
    if !wynik.usuniete.iter().any(|s| arc.znajdz(s).is_some()) {
        return;
    }
    // Kopia tylko wtedy, gdy drzewo trzyma jeszcze ktos inny (np. szukanie duplikatow).
    let d = Arc::make_mut(arc);
    let mut zmiana = false;
    for s in &wynik.usuniete {
        zmiana |= d.usun_sciezke(s);
    }
    if zmiana {
        wynik.korzen = d.wiersz(0, true);
    }
}

fn zapisz_historie(app: &AppHandle, opis: String, w: &WynikUsuwania) {
    if let Ok(plik) = plik_historii(app) {
        let _ = usuwanie::dopisz_historie(
            &plik,
            WpisHistorii {
                czas: porzadki::srodowisko::teraz(),
                rodzaj: "usuwanie".into(),
                opis,
                tryb: Some(w.tryb),
                pozycje: w.pozycje,
                pliki: w.pliki,
                na_dysku: w.na_dysku,
                pominiete: w.pominiete_razem,
            },
        );
    }
}

/// Usuwa sciezki (duplikaty, duze stare pliki) do Kosza albo na stale. Kazda przechodzi blokade
/// sciezek chronionych; zablokowane pliki wracaja w `pominiete` z powodem.
#[tauri::command]
pub async fn usun(
    app: AppHandle,
    stan: State<'_, Stan>,
    sciezki: Vec<String>,
    tryb: Tryb,
    opis: Option<String>,
) -> Result<WynikUsuwania, String> {
    let zajete = Zajete::wez(&stan)?;
    let mut wynik = tauri::async_runtime::spawn_blocking(move || {
        let _zajete = zajete;
        let ochrona = porzadki::ochrona::Ochrona::z_otoczenia();
        let usuwacz = usuwanie::Usuwacz { ochrona: &ochrona, kosz: usuwanie::Kosz::Systemowy };
        let sciezki: Vec<PathBuf> = sciezki.into_iter().map(PathBuf::from).collect();
        usuwacz.usun(&sciezki, tryb)
    })
    .await
    .map_err(|e| e.to_string())?;
    popraw_drzewo(&stan, &mut wynik);
    zapisz_historie(&app, opis.unwrap_or_else(|| "Wybrane pliki".into()), &wynik);
    Ok(wynik)
}

/// Usuwa pliki wybranych regul porzadkow (po numerach regul, nie po sciezkach z okna).
#[tauri::command]
pub async fn porzadki_usun(
    app: AppHandle,
    stan: State<'_, Stan>,
    ids: Vec<String>,
    tryb: Tryb,
) -> Result<WynikUsuwania, String> {
    let zajete = Zajete::wez(&stan)?;
    let anuluj = stan.anuluj.clone();
    let opis = format!("Do sprzątnięcia: {}", ids.len());
    let mut wynik = tauri::async_runtime::spawn_blocking(move || {
        let _zajete = zajete;
        let srod = porzadki::Srodowisko::z_otoczenia()?;
        let ochrona = porzadki::ochrona::Ochrona::windows(&srod);
        let usuwacz = usuwanie::Usuwacz { ochrona: &ochrona, kosz: usuwanie::Kosz::Systemowy };
        Ok::<_, String>(porzadki::usun_reguly(&srod, &ids, tryb, &usuwacz, &anuluj))
    })
    .await
    .map_err(|e| e.to_string())??;
    popraw_drzewo(&stan, &mut wynik);
    zapisz_historie(&app, opis, &wynik);
    Ok(wynik)
}

/// Uruchamia narzedzie Windows reguly (po potwierdzeniu w oknie). Gdy regula wymaga praw
/// administratora, Windows zapyta o nie. Wynik trafia do historii.
#[tauri::command]
pub async fn narzedzie(app: AppHandle, stan: State<'_, Stan>, id: String) -> Result<porzadki::narzedzia::WynikNarzedzia, String> {
    let zajete = Zajete::wez(&stan)?;
    let w = tauri::async_runtime::spawn_blocking(move || {
        let _zajete = zajete;
        porzadki::narzedzia::uruchom(&id)
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Ok(plik) = plik_historii(&app) {
        let nazwa = porzadki::KATALOG.iter().find(|r| r.id == w.id).map_or(w.id.as_str(), |r| r.nazwa);
        let _ = usuwanie::dopisz_historie(
            &plik,
            WpisHistorii {
                czas: porzadki::srodowisko::teraz(),
                rodzaj: "narzedzie".into(),
                opis: format!("{nazwa}: {}", w.opis),
                tryb: None,
                pozycje: u64::from(w.ok),
                pliki: 0,
                na_dysku: w.odzyskane,
                pominiete: u64::from(!w.ok),
            },
        );
    }
    Ok(w)
}

/// Historia porzadkow, od najnowszego wpisu.
#[tauri::command]
pub fn historia(app: AppHandle) -> Result<Vec<WpisHistorii>, String> {
    let mut h = usuwanie::wczytaj_historie(&plik_historii(&app)?);
    h.reverse();
    Ok(h)
}

#[derive(Serialize)]
pub struct StanAdmina {
    pub admin: bool,
    /// Czy przycisk „Uruchom jako administrator” ma sens (Windows bez praw).
    pub mozna_podniesc: bool,
}

#[tauri::command]
pub fn admin() -> StanAdmina {
    let admin = porzadki::narzedzia::jest_admin();
    StanAdmina { admin, mozna_podniesc: cfg!(windows) && !admin }
}

/// Uruchamia Luzika ponownie z prawami administratora i zamyka biezace okno.
#[tauri::command]
pub fn uruchom_jako_admin(app: AppHandle) -> Result<(), String> {
    porzadki::narzedzia::uruchom_jako_admin()?;
    app.exit(0);
    Ok(())
}

// --- LUZIK_AUTOSKAN: samosprawdzenie prawdziwego okna ---

/// Sciezka z LUZIK_AUTOSKAN albo None. Gdy jest, okno samo skanuje, wypisuje wynik i konczy.
#[tauri::command]
pub fn autoskan() -> Option<String> {
    std::env::var("LUZIK_AUTOSKAN").ok().filter(|s| !s.is_empty())
}

/// Wypisuje na stdout jedna linie `LUZIK_AUTOSKAN {json}` od interfejsu i zamyka aplikacje.
#[tauri::command]
pub fn autoskan_koniec(app: AppHandle, wynik: serde_json::Value) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "LUZIK_AUTOSKAN {wynik}");
    let _ = out.flush();
    app.exit(0);
}
