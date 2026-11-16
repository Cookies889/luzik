//! Sztuczne C:\ z ukladem katalogow Windows 11: dla testow regul i `luzik-cli sztuczne-c`.
//!
//! Dwoch uzytkownikow (Ania biezaca, Bartek drugi), smieci w miejscach z katalogu regul
//! i pliki, ktorych zadna regula nie moze ruszyc (Documents, Desktop, profile przegladarek
//! z haslami i historia). Te drugie zwraca `zbuduj` w polu `chronione`.

use std::fs::{self, File, FileTimes};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const BIEZACY: &str = "Ania";
pub const DRUGI: &str = "Bartek";

pub struct Opis {
    pub korzen: PathBuf,
    /// Pliki uzytkownika, ktorych zadna regula nie moze zlapac.
    pub chronione: Vec<PathBuf>,
}

/// Plik o danym rozmiarze i wieku w dniach (data zmiany i dostepu cofnieta).
pub fn plik(sciezka: &Path, bajty: usize, dni: f64) -> io::Result<()> {
    if let Some(r) = sciezka.parent() {
        fs::create_dir_all(r)?;
    }
    let mut f = File::create(sciezka)?;
    // Tresc zalezna od nazwy, zeby przypadkiem nie powstaly duplikaty.
    let ziarno = sciezka.to_string_lossy().bytes().fold(7u8, |a, b| a.wrapping_mul(31).wrapping_add(b));
    let tresc: Vec<u8> = (0..bajty).map(|i| ziarno.wrapping_add((i % 251) as u8)).collect();
    f.write_all(&tresc)?;
    let czas = SystemTime::now() - Duration::from_secs_f64(dni * 86_400.0);
    let czasy = FileTimes::new().set_modified(czas).set_accessed(czas);
    // Na Windows wiek liczy sie tez od utworzenia, wiec cofamy i te date.
    #[cfg(windows)]
    let czasy = std::os::windows::fs::FileTimesExt::set_created(czasy, czas);
    f.set_times(czasy)?;
    Ok(())
}

pub fn zbuduj(korzen: &Path) -> io::Result<Opis> {
    let mut chronione = Vec::new();
    let k = korzen;
    let p = |s: &str| -> PathBuf { s.split('\\').fold(k.to_path_buf(), |a, c| a.join(c)) };
    let smiec = |s: &str, bajty: usize, dni: f64| plik(&p(s), bajty, dni);

    // --- System ---
    smiec("Windows\\Temp\\stary-instalator.log", 20_000, 5.0)?;
    smiec("Windows\\Temp\\swiezy.log", 10_000, 0.1)?;
    smiec("Windows\\SoftwareDistribution\\Download\\a1b2\\windows11-kb5040442.cab", 300_000, 10.0)?;
    smiec("Windows\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache\\c1\\kawalek.dat", 150_000, 3.0)?;
    smiec("Windows\\Minidump\\111625-01.dmp", 40_000, 7.0)?;
    smiec("Windows\\MEMORY.DMP", 500_000, 7.0)?;
    smiec("Windows\\LiveKernelReports\\WATCHDOG\\WATCHDOG-20261101.dmp", 80_000, 15.0)?;
    smiec("Windows\\WinSxS\\amd64_microsoft-windows-x\\plik.dll", 50_000, 100.0)?;
    smiec("ProgramData\\Microsoft\\Windows\\WER\\ReportQueue\\AppCrash_x\\Report.wer", 12_000, 2.0)?;
    smiec("ProgramData\\NVIDIA Corporation\\NV_Cache\\shader.bin", 60_000, 4.0)?;
    smiec("Windows.old\\Windows\\System32\\stary.dll", 700_000, 90.0)?;
    smiec("$WINDOWS.~BT\\Sources\\install.esd", 250_000, 90.0)?;
    smiec("hiberfil.sys", 800_000, 1.0)?;
    smiec("pagefile.sys", 900_000, 0.0)?;
    smiec("swapfile.sys", 16_000, 0.0)?;
    smiec("System Volume Information\\{3808876b-c176-4e48-b7ae-04046e6cc752}", 30_000, 20.0)?;
    smiec("$Recycle.Bin\\S-1-5-21-1001\\$RX1Y2Z3.mp4", 400_000, 6.0)?;
    smiec("$Recycle.Bin\\S-1-5-21-1001\\$IX1Y2Z3.mp4", 544, 6.0)?;
    // Pliki systemu, ktorych reguly nie dotykaja.
    for s in ["Windows\\System32\\kernel32.dll", "Windows\\SoftwareDistribution\\DataStore\\DataStore.edb", "Program Files\\Gra\\gra.exe"] {
        plik(&p(s), 5_000, 200.0)?;
        chronione.push(p(s));
    }

    // --- Steam z druga biblioteka gier ---
    smiec("Program Files (x86)\\Steam\\steam.exe", 4_000, 30.0)?;
    smiec("Program Files (x86)\\Steam\\steamapps\\shadercache\\730\\fozpipelinesv6\\a.foz", 90_000, 3.0)?;
    smiec("Program Files (x86)\\Steam\\steamapps\\downloading\\440\\czesc.bin", 110_000, 1.0)?;
    smiec("Program Files (x86)\\Steam\\logs\\content_log.txt", 8_000, 2.0)?;
    let biblioteka = k.join("D_SteamLibrary");
    plik(&biblioteka.join("steamapps").join("shadercache").join("570").join("b.foz"), 70_000, 3.0)?;
    let vdf = format!(
        "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
        p("Program Files (x86)\\Steam").to_string_lossy().replace('\\', "\\\\"),
        biblioteka.to_string_lossy().replace('\\', "\\\\"),
    );
    fs::write(p("Program Files (x86)\\Steam\\steamapps\\libraryfolders.vdf"), vdf)?;
    for s in [
        "Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive\\game\\pak01_000.vpk",
        "Program Files (x86)\\Steam\\userdata\\123\\config\\localconfig.vdf",
    ] {
        plik(&p(s), 30_000, 40.0)?;
        chronione.push(p(s));
    }
    let gra = biblioteka.join("steamapps").join("common").join("dota 2 beta").join("game.vpk");
    plik(&gra, 30_000, 40.0)?;
    chronione.push(gra);

    // --- Uzytkownicy ---
    for (nazwa, pelny) in [(BIEZACY, true), (DRUGI, false)] {
        let u = format!("Users\\{nazwa}");
        let smiec = |s: &str, bajty: usize, dni: f64| plik(&p(&format!("{u}\\{s}")), bajty, dni);
        smiec("AppData\\Local\\Temp\\stary.tmp", 30_000, 3.0)?;
        smiec("AppData\\Local\\Temp\\{guid}\\rozpakowane.dll", 25_000, 2.0)?;
        smiec("AppData\\Local\\Temp\\otwarty-teraz.tmp", 9_000, 0.2)?;
        smiec("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data\\data_1", 120_000, 1.0)?;
        if pelny {
            smiec("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Code Cache\\js\\index", 45_000, 1.0)?;
            smiec("AppData\\Local\\Google\\Chrome\\User Data\\Default\\GPUCache\\data_0", 15_000, 1.0)?;
            smiec("AppData\\Local\\Google\\Chrome\\User Data\\Profile 1\\Cache\\Cache_Data\\f_000001", 60_000, 1.0)?;
            smiec("AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache\\Cache_Data\\data_2", 70_000, 1.0)?;
            smiec("AppData\\Local\\BraveSoftware\\Brave-Browser\\User Data\\Default\\Code Cache\\wasm\\x", 33_000, 1.0)?;
            smiec("AppData\\Local\\Mozilla\\Firefox\\Profiles\\ab12cd.default-release\\cache2\\entries\\E1F2", 55_000, 1.0)?;
            smiec("AppData\\Local\\Opera Software\\Opera Stable\\Default\\Cache\\Cache_Data\\data_3", 44_000, 1.0)?;
            smiec("AppData\\Roaming\\Opera Software\\Opera Stable\\Default\\GPUCache\\data_4", 11_000, 1.0)?;
            smiec("AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db", 210_000, 1.0)?;
            smiec("AppData\\Local\\Microsoft\\Windows\\Explorer\\iconcache_32.db", 35_000, 1.0)?;
            smiec("AppData\\Local\\D3DSCache\\5f1a\\gra.idx", 66_000, 5.0)?;
            smiec("AppData\\Local\\NVIDIA\\DXCache\\a.nvph", 77_000, 5.0)?;
            smiec("AppData\\LocalLow\\NVIDIA\\PerDriverVersion\\DXCache\\b.nvph", 22_000, 5.0)?;
            smiec("AppData\\Local\\AMD\\DxCache\\c.bin", 18_000, 5.0)?;
            smiec("AppData\\Local\\CrashDumps\\Discord.exe.4412.dmp", 90_000, 8.0)?;
            smiec("AppData\\Local\\Microsoft\\Windows\\WER\\ReportArchive\\AppCrash_y\\Report.wer", 6_000, 8.0)?;
            smiec("AppData\\Roaming\\discord\\Cache\\Cache_Data\\f_00a1", 130_000, 1.0)?;
            smiec("AppData\\Roaming\\discord\\Code Cache\\js\\x", 40_000, 1.0)?;
            smiec("AppData\\Roaming\\discord\\GPUCache\\data_0", 12_000, 1.0)?;
            smiec("AppData\\Roaming\\Microsoft\\Teams\\Cache\\f_0001", 50_000, 1.0)?;
            smiec("AppData\\Local\\Spotify\\Data\\ab\\cd.file", 140_000, 1.0)?;
            smiec("AppData\\Local\\Spotify\\Storage\\ef\\gh.file", 160_000, 1.0)?;
            smiec("AppData\\Local\\npm-cache\\_cacache\\content-v2\\sha512\\aa\\x", 48_000, 9.0)?;
            smiec("AppData\\Local\\pip\\Cache\\http-v2\\a\\b", 27_000, 9.0)?;
            smiec(".nuget\\packages\\newtonsoft.json\\13.0.3\\newtonsoft.json.13.0.3.nupkg", 38_000, 9.0)?;
            smiec(".gradle\\caches\\modules-2\\files-2.1\\x.jar", 42_000, 9.0)?;
            smiec("Downloads\\ChromeSetup.exe", 100_000, 45.0)?;
            smiec("Downloads\\stary\\sterownik-wifi.exe", 60_000, 200.0)?;
            smiec("Downloads\\Win11_PL.iso", 900_000, 60.0)?;
            smiec("Downloads\\vc_redist.msi", 50_000, 31.0)?;
            smiec("Downloads\\nowy-program.exe", 70_000, 3.0)?;
            smiec("OneDrive\\Dokumenty\\plan.xlsx", 25_000, 10.0)?;
            smiec("OneDrive - Firma\\Raporty\\raport.docx", 35_000, 10.0)?;
        }
        // Chronione: dokumenty, pulpit, zawartosc profili przegladarek poza cache, ustawienia programow.
        let wazne: &[(&str, f64)] = &[
            ("Documents\\CV.docx", 400.0),
            ("Documents\\Cache\\nie-cache.bin", 400.0),
            ("Documents\\Temp\\praca-domowa.tmp", 400.0),
            ("Documents\\thumbcache_99.db", 400.0),
            ("Documents\\instalki\\stary-setup.exe", 400.0),
            ("Desktop\\setup.exe", 400.0),
            ("Desktop\\notatki.txt", 400.0),
            ("Desktop\\GPUCache\\zdjecie.png", 400.0),
            ("Downloads\\umowa.pdf", 400.0),
            ("Downloads\\wakacje.jpg", 400.0),
            ("Pictures\\wakacje\\IMG_0001.jpg", 400.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Login Data", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Network\\Cookies", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\History", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Bookmarks", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Default\\Local Storage\\leveldb\\000003.log", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Profile 1\\Login Data", 30.0),
            ("AppData\\Local\\Google\\Chrome\\User Data\\Local State", 30.0),
            ("AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Login Data", 30.0),
            ("AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Web Data", 30.0),
            ("AppData\\Local\\BraveSoftware\\Brave-Browser\\User Data\\Default\\Login Data", 30.0),
            ("AppData\\Roaming\\Mozilla\\Firefox\\Profiles\\ab12cd.default-release\\logins.json", 30.0),
            ("AppData\\Roaming\\Mozilla\\Firefox\\Profiles\\ab12cd.default-release\\key4.db", 30.0),
            ("AppData\\Roaming\\Mozilla\\Firefox\\Profiles\\ab12cd.default-release\\places.sqlite", 30.0),
            ("AppData\\Local\\Mozilla\\Firefox\\Profiles\\ab12cd.default-release\\startupCache\\x.lz4", 30.0),
            ("AppData\\Roaming\\Opera Software\\Opera Stable\\Default\\Login Data", 30.0),
            ("AppData\\Roaming\\Opera Software\\Opera Stable\\Default\\Bookmarks", 30.0),
            ("AppData\\Roaming\\discord\\Local Storage\\leveldb\\000005.ldb", 30.0),
            ("AppData\\Roaming\\discord\\settings.json", 30.0),
            ("AppData\\Roaming\\Microsoft\\Teams\\desktop-config.json", 30.0),
            ("AppData\\Local\\Microsoft\\Windows\\Explorer\\ExplorerStartupLog.etl", 30.0),
        ];
        for (s, dni) in wazne {
            let sciezka = p(&format!("{u}\\{s}"));
            plik(&sciezka, 7_000, *dni)?;
            chronione.push(sciezka);
        }
    }
    // Junction "Application Data" -> Roaming jak w prawdziwym profilu: nie moze podwoic wynikow.
    #[cfg(unix)]
    {
        let profil = p(&format!("Users\\{BIEZACY}"));
        let _ = std::os::unix::fs::symlink(profil.join("AppData").join("Roaming"), profil.join("Application Data"));
        let _ = std::os::unix::fs::symlink(profil.join("AppData").join("Local"), profil.join("AppData").join("Local").join("Application Data"));
    }
    // Profile, ktore nie sa ludzmi.
    plik(&p("Users\\Public\\Desktop\\skrot.lnk"), 1_000, 100.0)?;
    plik(&p("Users\\Default\\NTUSER.DAT"), 1_000, 100.0)?;
    Ok(Opis { korzen: korzen.to_path_buf(), chronione })
}
