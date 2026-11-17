//! Luzik: silnik skanu (`skan`) i komendy Tauri dla okna aplikacji.

mod komendy;
pub mod porzadki;
pub mod skan;

pub fn run() {
    tauri::Builder::default()
        // Aktualizacje z GitHub Releases (latest.json, podpis kluczem z tauri.conf.json) i restart po nich.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(komendy::Stan::default())
        .invoke_handler(tauri::generate_handler![
            komendy::lista_dyskow,
            komendy::skanuj,
            komendy::anuluj,
            komendy::dzieci,
            komendy::najwieksze_pliki,
            komendy::porzadki_skanuj,
            komendy::duplikaty,
            komendy::kategorie,
            komendy::stare_pliki,
            komendy::usun,
            komendy::usun_wezly,
            komendy::odswiez_poziom,
            komendy::porzadki_usun,
            komendy::narzedzie,
            komendy::historia,
            komendy::admin,
            komendy::uruchom_jako_admin,
            komendy::autoskan,
            komendy::autoskan_koniec,
            komendy::autoskan_usun,
        ])
        .run(tauri::generate_context!())
        .expect("Luzik nie mogl sie uruchomic");
}
