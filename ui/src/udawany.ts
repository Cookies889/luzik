// Udawany backend do ogladania okna w zwyklej przegladarce (bez Tauri).
// Drzewo przypomina dysk C: zwyklego komputera z Windows 11: gry ze Steama, nagrania OBS,
// pamieci podreczne przegladarek, Windows.old. Znaleziska, duplikaty, typy plikow i stare pliki
// sa liczone z tego samego drzewa, wiec liczby na wszystkich zakladkach sie zgadzaja.

import type {
  Backend, Dysk, GrupaDuplikatow, Miejsce, Podsumowanie, StaryPlik, Tryb, Wiersz, WpisHistorii,
  WynikDuplikatow, WynikKategorii, WynikUsuwania, Znalezisko,
} from './api';

const KB = 1024;
const MB = 1024 * KB;
const GB = 1024 * MB;
const DZIEN = 86_400;
const TERAZ = Math.floor(Date.UTC(2026, 10, 16, 12) / 1000);

// --- Opis drzewa ---

type Spec =
  | { t: 'k'; nazwa: string; dzieci: Spec[] }
  | { t: 'p'; nazwa: string; bajty: number; dni: number; dup?: string }
  | { t: 'l'; nazwa: string; bajty: number; n: number; nazwa_pliku: (i: number) => string; dni: number };

const K = (nazwa: string, ...dzieci: Spec[]): Spec => ({ t: 'k', nazwa, dzieci });
const P = (nazwa: string, bajty: number, dni = 40, dup?: string): Spec => ({ t: 'p', nazwa, bajty, dni, dup });
/** Katalog z n plikami o lacznym rozmiarze `bajty` (kilka duzych, reszta coraz mniejsza). */
const L = (nazwa: string, bajty: number, n: number, nazwa_pliku: (i: number) => string, dni = 30): Spec =>
  ({ t: 'l', nazwa, bajty, n, nazwa_pliku, dni });
/** Sciezka zagniezdzonych katalogow zakonczona jednym wezlem. */
const Z = (sciezka: string, koniec: Spec): Spec =>
  sciezka.split('\\').reverse().reduce<Spec>((acc, czesc) => K(czesc, acc), koniec);

const hex = (i: number, s = 0x1a3f) => (s + i * 37).toString(16).padStart(6, '0');
const wzor = (przed: string, roz: string, start = 1) => (i: number) => `${przed}${i + start}.${roz}`;
const cache = (i: number) => `f_${hex(i)}`;
const dll = (i: number) => `${['api-ms-win', 'Windows.UI', 'd3d', 'msvcp', 'ucrt', 'combase', 'nvwgf', 'ntdll'][i % 8]}${i}.dll`;

const NAGRANIA = (i: number) => {
  const d = new Date((TERAZ - (420 - i * 9) * DZIEN) * 1000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(18 + (i % 5))}-${p((i * 13) % 60)}-${p((i * 7) % 60)}.mkv`;
};

const ANIA = K('Ania',
  K('AppData',
    K('Local',
      L('Temp', 6.4 * GB, 3200, (i) => (i % 3 ? `tmp${hex(i, 0x4b2)}.tmp` : `{${hex(i)}-${hex(i * 3)}}.log`), 9),
      Z('Google\\Chrome\\User Data', K('Default',
        Z('Cache', L('Cache_Data', 2.9 * GB, 1840, cache, 2)),
        L('Code Cache', 0.8 * GB, 960, cache, 3),
        L('Service Worker', 1.1 * GB, 720, cache, 5),
        P('History', 61 * MB, 0),
        P('Favicons', 18 * MB, 0),
      )),
      K('Microsoft',
        Z('Edge\\User Data\\Default', L('Cache', 1.4 * GB, 1100, cache, 4)),
        Z('Windows', L('Explorer', 0.9 * GB, 14, (i) => `thumbcache_${[16, 32, 48, 96, 256, 768, 1280, 1920, 2560, 'exif', 'idx', 'sr', 'wide', 'custom_stream'][i]}.db`, 1)),
        L('OneDrive', 0.35 * GB, 210, wzor('OneDrive', 'dll'), 30),
      ),
      Z('NVIDIA', L('DXCache', 1.9 * GB, 420, (i) => `${hex(i, 0xd0c)}_${hex(i * 7)}.nvph`, 6)),
      L('D3DSCache', 0.6 * GB, 140, (i) => `${hex(i, 0xa11)}.idx`, 6),
      L('Packages', 3.2 * GB, 2100, (i) => `settings${i}.dat`, 20),
      K('Programs',
        L('Microsoft VS Code', 0.42 * GB, 2400, (i) => `code${i}.js`, 25),
        L('Python', 0.31 * GB, 4100, (i) => `module${i}.py`, 160),
      ),
      L('Discord', 0.52 * GB, 380, wzor('app-', 'asar'), 3),
      Z('Spotify', L('Storage', 4.2 * GB, 2600, (i) => `${hex(i, 0x5b0)}.file`, 12)),
      L('npm-cache', 1.6 * GB, 9800, (i) => `content-v2-${hex(i)}`, 70),
      Z('pip', L('cache', 0.7 * GB, 1300, (i) => `${hex(i, 0x9e1)}.whl`, 90)),
      L('CrashDumps', 1.2 * GB, 4, (i) => `${['Discord.exe', 'obs64.exe', 'Cyberpunk2077.exe', 'explorer.exe'][i]}.${2840 + i * 113}.dmp`, 50),
    ),
    K('Roaming',
      K('discord',
        L('Cache', 1.8 * GB, 2300, cache, 1),
        L('Code Cache', 0.21 * GB, 300, cache, 1),
      ),
      L('Code', 0.9 * GB, 1600, (i) => `state${i}.vscdb`, 4),
      L('Microsoft', 0.4 * GB, 900, (i) => `Recent${i}.lnk`, 10),
      L('npm', 0.3 * GB, 3200, (i) => `index${i}.js`, 80),
    ),
    L('LocalLow', 0.6 * GB, 420, (i) => `Player${i}.log`, 30),
  ),
  K('Videos',
    L('Nagrania OBS', 31.4 * GB, 46, NAGRANIA, 230),
    L('Captures', 6.2 * GB, 30, (i) => `Cyberpunk 2077 2025-0${(i % 9) + 1}-1${i % 9} 21-0${i % 9}-1${i % 6}.mp4`, 260),
    P('Diuna Część druga (2024).mkv', 4.3 * GB, 380, 'diuna'),
  ),
  K('Pictures',
    K('Telefon 2024',
      L('Wakacje Chorwacja', 4.2 * GB, 1200, wzor('IMG_20240714_', 'jpg', 101000), 480),
      L('Zwykłe', 4.9 * GB, 1600, wzor('IMG_20241', 'jpg', 10000), 400),
      P('IMG_20240714_183022.jpg', 6.1 * MB, 480, 'img1'),
      P('IMG_20240716_101544.jpg', 5.4 * MB, 478, 'img2'),
      P('VID_20240715_200311.mp4', 812 * MB, 479, 'vid1'),
    ),
    L('Zrzuty ekranu', 1.4 * GB, 900, (i) => `Zrzut ekranu 2025-0${(i % 9) + 1}-${10 + (i % 18)} 1${i % 10}${i % 6}${i % 10}.png`, 90),
    L('Tapety', 0.3 * GB, 60, wzor('tapeta-', 'jpg'), 300),
  ),
  K('Downloads',
    P('Win11_24H2_Polish_x64.iso', 5.8 * GB, 410, 'iso'),
    P('Diuna.Czesc.Druga.2024.1080p.mkv', 4.3 * GB, 381, 'diuna'),
    P('Mody_Stardew.zip', 2.4 * GB, 220),
    L('Zdjęcia z wycieczki', 1.9 * GB, 180, wzor('IMG_2023', 'jpg', 9000), 600),
    P('nvidia-driver-566.14-desktop-win10-win11-64bit.exe', 0.7 * GB, 120),
    P('Docker Desktop Installer.exe', 0.6 * GB, 190),
    P('OBS-Studio-30.2.3-Windows-Installer.exe', 143 * MB, 300),
    P('VSCodeUserSetup-x64-1.95.3.exe', 98 * MB, 60),
    P('python-3.12.4-amd64.exe', 26 * MB, 160),
    P('SteamSetup.exe', 2.3 * MB, 500),
    P('Discord Setup.exe', 112 * MB, 8),
  ),
  K('Documents',
    L('Szkoła', 0.8 * GB, 640, (i) => `${['Referat', 'Notatki', 'Projekt', 'Sprawozdanie'][i % 4]} ${i + 1}.docx`, 200),
    K('Projekty',
      K('strona-klubu',
        L('node_modules', 0.62 * GB, 14200, (i) => `index${i}.js`, 140),
        L('src', 4 * MB, 60, (i) => `komponent${i}.ts`, 140),
      ),
      L('gra-w-unity', 1.5 * GB, 2300, (i) => `asset${i}.prefab`, 260),
    ),
    L('My Games', 1.2 * GB, 120, (i) => `save_${i}.sav`, 40),
    K('Instalki', P('Win11_24H2_Polish_x64 (1).iso', 5.8 * GB, 409, 'iso')),
  ),
  K('OneDrive',
    K('Zdjęcia z aparatu',
      L('2023', 8.4 * GB, 2400, wzor('IMG_2023', 'jpg', 1000), 700),
      P('IMG_20240714_183022.jpg', 6.1 * MB, 470, 'img1'),
      P('IMG_20240716_101544.jpg', 5.4 * MB, 470, 'img2'),
      P('VID_20240715_200311.mp4', 812 * MB, 470, 'vid1'),
    ),
    L('Dokumenty', 1.9 * GB, 800, (i) => `Dokument ${i + 1}.pdf`, 150),
    L('Pulpit', 0.4 * GB, 90, (i) => `Notatka ${i + 1}.txt`, 120),
  ),
  L('Desktop', 2.3 * GB, 48, (i) => (i % 2 ? `Skrót ${i}.lnk` : `Nagranie ${i}.mp4`), 30),
  L('Music', 3.1 * GB, 620, (i) => `Utwór ${String(i + 1).padStart(3, '0')}.mp3`, 500),
);

const DRZEWO_C = K('C:\\',
  K('$Recycle.Bin', K('S-1-5-21-3623811015-3361044348-30300820-1001',
    P('$RW8K2LP.mkv', 2.1 * GB, 40), P('$R4Q7ZA1.zip', 0.9 * GB, 60),
    L('$RJ2M0QX', 0.6 * GB, 240, wzor('IMG_', 'jpg'), 60))),
  K('Program Files',
    L('NVIDIA Corporation', 2.1 * GB, 380, dll, 120),
    Z('Microsoft Office', L('root', 4.3 * GB, 2600, (i) => `office${i}.dll`, 30)),
    K('Adobe',
      L('Adobe Photoshop 2025', 4.1 * GB, 3100, (i) => `ps${i}.dll`, 60),
      L('Adobe Premiere Pro 2025', 5.3 * GB, 4200, (i) => `pr${i}.dll`, 60),
    ),
    K('WindowsApps',
      L('Microsoft.MinecraftUWP_1.21.4402.0_x64__8wekyb3d8bbwe', 1.4 * GB, 900, (i) => `data${i}.pak`, 30),
      L('Microsoft.GamingApp_2410.1001.20.0_x64__8wekyb3d8bbwe', 1.1 * GB, 700, dll, 30),
      L('MSTeams_24295.605.3225.8804_x64__8wekyb3d8bbwe', 0.42 * GB, 500, dll, 30),
      L('Microsoft.WindowsTerminal_1.21.3231.0_x64__8wekyb3d8bbwe', 0.1 * GB, 120, dll, 30),
    ),
    L('Docker', 2.4 * GB, 600, (i) => `docker${i}.exe`, 190),
    L('Common Files', 1.2 * GB, 900, dll, 200),
    L('Git', 0.36 * GB, 1800, (i) => `git${i}.exe`, 150),
    L('Mozilla Firefox', 0.27 * GB, 140, dll, 12),
  ),
  K('Program Files (x86)',
    K('Steam',
      K('steamapps',
        K('common',
          L('Cyberpunk 2077', 70.9 * GB, 900, (i) => `basegame_${i}.archive`, 300),
          L('Counter-Strike Global Offensive', 33.6 * GB, 1400, (i) => `pak01_${String(i).padStart(3, '0')}.vpk`, 20),
          L('Rocket League', 21.2 * GB, 800, (i) => `TAGame_${i}.upk`, 400),
          L('Stardew Valley', 0.6 * GB, 300, (i) => `Content${i}.xnb`, 220),
        ),
        L('shadercache', 4.8 * GB, 600, (i) => `fozpipelinesv6_${i}.foz`, 7),
        L('downloading', 6.3 * GB, 40, (i) => `chunk_${hex(i)}.tmp`, 15),
        L('workshop', 2.1 * GB, 120, (i) => `${2900000000 + i * 1777}.bin`, 90),
      ),
      L('logs', 0.18 * GB, 90, (i) => `content_log${i}.txt`, 2),
      L('bin', 1.1 * GB, 400, dll, 5),
    ),
    L('Microsoft', 0.9 * GB, 500, dll, 12),
  ),
  K('ProgramData',
    K('Microsoft',
      L('Windows Defender', 1.1 * GB, 300, (i) => `mpavbase${i}.vdm`, 1),
      K('Windows',
        L('WER', 0.8 * GB, 120, (i) => `Report.wer.${hex(i)}`, 45),
        L('Start Menu', 0.02 * GB, 200, (i) => `Program ${i}.lnk`, 90),
      ),
      L('Search', 1.4 * GB, 30, (i) => `Windows.edb.${i}`, 0),
      L('Diagnosis', 0.4 * GB, 80, (i) => `events${i}.rbs`, 3),
    ),
    L('NVIDIA Corporation', 1.2 * GB, 260, dll, 120),
    L('Package Cache', 3.1 * GB, 340, (i) => `vc_redist_${i}.msi`, 400),
  ),
  K('Users', ANIA, L('Public', 0.4 * GB, 90, wzor('plik', 'dat'), 300), L('Default', 0.1 * GB, 60, wzor('NTUSER', 'DAT'), 600)),
  K('Windows',
    L('WinSxS', 9.8 * GB, 2400, (i) => `amd64_microsoft-windows-${hex(i)}.manifest`, 60),
    K('System32',
      Z('DriverStore', L('FileRepository', 3.0 * GB, 1900, (i) => `nv_dispi.inf_amd64_${hex(i)}.sys`, 120)),
      L('drivers', 0.12 * GB, 420, (i) => `driver${i}.sys`, 60),
      L('config', 0.21 * GB, 40, (i) => `SYSTEM${i}.LOG`, 0),
      L('biblioteki', 4.1 * GB, 1800, dll, 60),
    ),
    L('SysWOW64', 1.6 * GB, 1500, dll, 60),
    L('Installer', 4.2 * GB, 300, (i) => `${hex(i, 0x3c1)}.msi`, 200),
    K('SoftwareDistribution',
      L('Download', 3.1 * GB, 160, (i) => `windows11.0-kb50${44284 + i}-x64.cab`, 25),
      L('DataStore', 0.31 * GB, 10, (i) => `DataStore${i}.edb`, 1),
    ),
    Z('ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization',
      L('Cache', 2.6 * GB, 220, (i) => `${hex(i, 0xde1)}`, 20)),
    L('Temp', 1.1 * GB, 500, (i) => `MpCmdRun${i}.log`, 12),
    L('assembly', 1.4 * GB, 900, dll, 300),
    L('Microsoft.NET', 0.9 * GB, 1200, dll, 200),
    L('Fonts', 0.6 * GB, 400, (i) => `font${i}.ttf`, 400),
    L('servicing', 0.9 * GB, 1000, (i) => `package${i}.mum`, 60),
    L('Logs', 0.3 * GB, 200, (i) => `CBS${i}.log`, 2),
    L('LiveKernelReports', 0.7 * GB, 3, (i) => `WATCHDOG-2025060${i + 1}-1830.dmp`, 160),
  ),
  K('Windows.old',
    L('Windows', 14.9 * GB, 3000, dll, 150),
    L('Program Files', 3.8 * GB, 900, dll, 150),
    L('Users', 2.6 * GB, 700, (i) => `plik${i}.dat`, 150),
  ),
  P('pagefile.sys', 16 * GB, 0),
  P('hiberfil.sys', 12.7 * GB, 0),
  P('swapfile.sys', 256 * MB, 0),
  P('DumpStack.log.tmp', 8 * KB, 0),
);

const DRZEWO_D = K('D:\\',
  K('Gry',
    L('Baldurs Gate 3', 122 * GB, 1100, (i) => `Gustav_${i}.pak`, 180),
    L('Forza Horizon 5', 103 * GB, 900, (i) => `media${i}.zip`, 250),
  ),
  K('Filmy', L('Seriale', 88 * GB, 140, (i) => `Odcinek ${i + 1}.mkv`, 500), L('Rodzinne', 41 * GB, 300, wzor('MOV_', 'mp4'), 900)),
  L('Kopie zapasowe', 96 * GB, 12, (i) => `Kopia telefonu ${i + 1}.zip`, 700),
  L('Muzyka', 12 * GB, 2400, (i) => `Utwór ${i + 1}.flac`, 800),
);

// --- Budowa wezlow ---

interface Wezel {
  id: number;
  nazwa: string;
  rodzaj: 'plik' | 'katalog';
  rozmiar: number;
  na_dysku: number;
  pliki: number;
  mtime: number;
  rodzic: number;
  dzieci: number[];
  sciezka: string;
  dup?: string;
}

const naDysku = (b: number) => Math.ceil(b / 4096) * 4096;

class Drzewo {
  wezly: Wezel[] = [];
  poSciezce = new Map<string, Wezel>();
  katalogi: Wezel[] = [];

  constructor(spec: Spec) {
    this.dodaj(spec, -1, '');
    for (const w of this.wezly) if (w.dzieci.length) w.dzieci.sort((a, b) => this.wezly[b].na_dysku - this.wezly[a].na_dysku);
  }

  private nowy(nazwa: string, rodzaj: 'plik' | 'katalog', rodzic: number, sciezkaRodzica: string): Wezel {
    const sciezka = rodzic < 0 ? nazwa : sciezkaRodzica.endsWith('\\') ? sciezkaRodzica + nazwa : `${sciezkaRodzica}\\${nazwa}`;
    const w: Wezel = { id: this.wezly.length, nazwa, rodzaj, rozmiar: 0, na_dysku: 0, pliki: 0, mtime: 0, rodzic, dzieci: [], sciezka };
    this.wezly.push(w);
    this.poSciezce.set(sciezka.toLowerCase(), w);
    if (rodzic >= 0) this.wezly[rodzic].dzieci.push(w.id);
    if (rodzaj === 'katalog') this.katalogi.push(w);
    return w;
  }

  private plik(nazwa: string, bajty: number, dni: number, rodzic: Wezel, dup?: string): Wezel {
    const w = this.nowy(nazwa, 'plik', rodzic.id, rodzic.sciezka);
    w.rozmiar = Math.round(bajty);
    w.na_dysku = naDysku(w.rozmiar);
    w.pliki = 1;
    w.mtime = TERAZ - dni * DZIEN - (w.id % 3600);
    w.dup = dup;
    return w;
  }

  private dodaj(s: Spec, rodzic: number, sciezkaRodzica: string): Wezel {
    if (s.t === 'p') return this.plik(s.nazwa, s.bajty, s.dni, this.wezly[rodzic], s.dup);
    const k = this.nowy(s.nazwa, 'katalog', rodzic, sciezkaRodzica);
    const dzieci: Wezel[] = [];
    if (s.t === 'k') {
      for (const d of s.dzieci) dzieci.push(this.dodaj(d, k.id, k.sciezka));
    } else {
      // Rozklad potegowy: kilka duzych plikow, dlugi ogon malych.
      const wagi = Array.from({ length: s.n }, (_, i) => 1 / Math.pow(i + 1, 1.15));
      const suma = wagi.reduce((a, b) => a + b, 0);
      let reszta = Math.round(s.bajty);
      for (let i = 0; i < s.n; i++) {
        const b = i === s.n - 1 ? reszta : Math.round((s.bajty * wagi[i]) / suma);
        reszta -= b;
        dzieci.push(this.plik(s.nazwa_pliku(i), Math.max(b, 0), s.dni + ((i * 7) % 40), k));
      }
    }
    for (const d of dzieci) {
      k.rozmiar += d.rozmiar;
      k.na_dysku += d.na_dysku;
      k.pliki += d.pliki;
      k.mtime = Math.max(k.mtime, d.mtime);
    }
    return k;
  }

  wiersz(w: Wezel, zeSciezka = false): Wiersz {
    return {
      id: w.id, nazwa: w.nazwa, rodzaj: w.rodzaj, rozmiar: w.rozmiar, na_dysku: w.na_dysku, pliki: w.pliki,
      mtime: w.mtime, liczba_dzieci: w.dzieci.length, duplikat: false, niedostepny: false, inny_system: false,
      ...(zeSciezka ? { sciezka: w.sciezka } : {}),
    };
  }

  znajdz(sciezka: string): Wezel | undefined {
    return this.poSciezce.get(sciezka.toLowerCase());
  }

  /** Usuwa wezel z drzewa i odejmuje go od przodkow (jak Drzewo::usun_sciezke w Rust). */
  usun(w: Wezel) {
    for (let r = w.rodzic; r >= 0; r = this.wezly[r].rodzic) {
      const p = this.wezly[r];
      p.rozmiar -= w.rozmiar;
      p.na_dysku -= w.na_dysku;
      p.pliki -= w.pliki;
    }
    if (w.rodzic >= 0) {
      const rodzic = this.wezly[w.rodzic];
      rodzic.dzieci = rodzic.dzieci.filter((id) => id !== w.id);
    }
    const stos = [w.id];
    while (stos.length) {
      const x = this.wezly[stos.pop()!];
      this.poSciezce.delete(x.sciezka.toLowerCase());
      stos.push(...x.dzieci);
    }
  }

  *pliki(od = 0): Generator<Wezel> {
    const stos = [od];
    while (stos.length) {
      const w = this.wezly[stos.pop()!];
      if (w.rodzaj === 'plik') yield w;
      else stos.push(...w.dzieci);
    }
  }
}

// --- Reguly porzadkow (teksty jak w src-tauri/src/porzadki/reguly.rs) ---

interface RegulaUdawana {
  id: string;
  nazwa: string;
  wyjasnienie: string;
  poziom: Znalezisko['poziom'];
  sposob: Znalezisko['sposob'];
  polecenie?: string;
  wymaga_admina: boolean;
  sciezki: string[];
  bezRozmiaru?: boolean;
  filtr?: (w: Wezel) => boolean;
}

const U = 'C:\\Users\\Ania';
const REGULY: RegulaUdawana[] = [
  { id: 'temp-uzytkownika', nazwa: 'Pliki tymczasowe (%TEMP%)', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Pliki, które programy odkładają na chwilę i zapominają posprzątać; usuwamy tylko te starsze niż doba, więc nic, co teraz działa, nie ucierpi.',
    sciezki: [`${U}\\AppData\\Local\\Temp`] },
  { id: 'temp-windows', nazwa: 'Pliki tymczasowe Windows (C:\\Windows\\Temp)', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: true,
    wyjasnienie: 'Tymczasowe pliki samego systemu i instalatorów; te starsze niż doba nie są już do niczego potrzebne.',
    sciezki: ['C:\\Windows\\Temp'] },
  { id: 'kosz', nazwa: 'Kosz', poziom: 'sprawdz', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Pliki, które już wyrzuciłeś; po opróżnieniu Kosza nie da się ich przywrócić, więc najpierw tam zajrzyj.',
    sciezki: ['C:\\$Recycle.Bin\\S-1-5-21-3623811015-3361044348-30300820-1001'] },
  { id: 'aktualizacje-windows', nazwa: 'Pobrane aktualizacje Windows', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: true,
    wyjasnienie: 'Paczki aktualizacji, które Windows już zainstalował; gdyby były jeszcze potrzebne, system pobierze je ponownie.',
    sciezki: ['C:\\Windows\\SoftwareDistribution\\Download'] },
  { id: 'optymalizacja-dostarczania', nazwa: 'Pamięć optymalizacji dostarczania', poziom: 'bezpieczne', sposob: 'narzedzie', wymaga_admina: true,
    polecenie: 'Delete-DeliveryOptimizationCache -Force',
    wyjasnienie: 'Kawałki aktualizacji, którymi Windows dzieli się z innymi komputerami; po usunięciu po prostu pobierze je z internetu.',
    sciezki: ['C:\\Windows\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache'] },
  { id: 'windows-old', nazwa: 'Poprzednia wersja Windows (Windows.old)', poziom: 'sprawdz', sposob: 'narzedzie', wymaga_admina: true,
    polecenie: 'cleanmgr.exe /AUTOCLEAN',
    wyjasnienie: 'Kopia starego systemu zostawiona po dużej aktualizacji; po usunięciu nie da się już wrócić do poprzedniej wersji Windows.',
    sciezki: ['C:\\Windows.old'] },
  { id: 'zrzuty-awarii', nazwa: 'Zrzuty pamięci po awariach', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: true,
    wyjasnienie: 'Zapis stanu komputera z chwili, gdy coś się wysypało; przydaje się tylko informatykowi, który szuka przyczyny awarii.',
    sciezki: ['C:\\Windows\\LiveKernelReports', `${U}\\AppData\\Local\\CrashDumps`] },
  { id: 'raporty-bledow', nazwa: 'Raporty błędów Windows', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: true,
    wyjasnienie: 'Opisy awarii programów przygotowane do wysłania do Microsoftu; po usunięciu nic nie przestanie działać.',
    sciezki: ['C:\\ProgramData\\Microsoft\\Windows\\WER'] },
  { id: 'miniatury', nazwa: 'Pamięć miniatur i ikon', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Zapisane podglądy zdjęć i ikon w Eksploratorze; Windows zrobi je od nowa, a foldery ze zdjęciami przez chwilę otworzą się wolniej.',
    sciezki: [`${U}\\AppData\\Local\\Microsoft\\Windows\\Explorer`] },
  { id: 'shadery', nazwa: 'Pamięć shaderów DirectX i karty graficznej', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Gotowe kawałki grafiki zapisane przez gry i sterowniki NVIDIA, AMD i Intel; po usunięciu gry przy pierwszym uruchomieniu mogą chwilę przycinać, potem wszystko wraca do normy.',
    sciezki: [`${U}\\AppData\\Local\\NVIDIA\\DXCache`, `${U}\\AppData\\Local\\D3DSCache`] },
  { id: 'przegladarki', nazwa: 'Pamięć podręczna przeglądarek', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Zapisane kopie stron w Chrome, Edge, Firefoxie, Brave i Operze; hasła, zakładki, historia i logowania zostają, a strony najwyżej raz wczytają się wolniej.',
    sciezki: [
      `${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache`,
      `${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Code Cache`,
      `${U}\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Service Worker`,
      `${U}\\AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache`,
    ] },
  { id: 'komunikatory', nazwa: 'Pamięć podręczna Discorda, Teams i Spotify', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Zapisane obrazki, filmiki i kawałki piosenek; czaty, ustawienia i konta zostają, a reszta dociągnie się z internetu.',
    sciezki: [`${U}\\AppData\\Roaming\\discord\\Cache`, `${U}\\AppData\\Roaming\\discord\\Code Cache`] },
  { id: 'spotify-offline', nazwa: 'Utwory Spotify zapisane na dysku', poziom: 'sprawdz', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Pamięć piosenek Spotify razem z pobranymi do słuchania bez internetu; po usunięciu pobrane playlisty trzeba będzie ściągnąć jeszcze raz.',
    sciezki: [`${U}\\AppData\\Local\\Spotify\\Storage`] },
  { id: 'steam-shadery-logi', nazwa: 'Shadery i dzienniki Steama', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Grafika przygotowana przez gry ze Steama i stare dzienniki programu; gry odbudują shadery przy następnym uruchomieniu, przez chwilę mogą przycinać.',
    sciezki: ['C:\\Program Files (x86)\\Steam\\steamapps\\shadercache', 'C:\\Program Files (x86)\\Steam\\logs'] },
  { id: 'steam-pobierane', nazwa: 'Niedokończone pobrania Steama', poziom: 'sprawdz', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Częściowo pobrane gry i aktualizacje; usuń je tylko wtedy, gdy Steam nic teraz nie pobiera, bo inaczej zacznie od początku.',
    sciezki: ['C:\\Program Files (x86)\\Steam\\steamapps\\downloading'] },
  { id: 'programisci', nazwa: 'Pamięć podręczna narzędzi programisty (npm, pip, NuGet, Gradle)', poziom: 'bezpieczne', sposob: 'trwale', wymaga_admina: false,
    wyjasnienie: 'Paczki pobrane przez narzędzia programisty i trzymane na zapas; po usunięciu narzędzia pobiorą je ponownie, gdy będą potrzebne.',
    sciezki: [`${U}\\AppData\\Local\\npm-cache`, `${U}\\AppData\\Local\\pip\\cache`] },
  { id: 'stare-instalatory', nazwa: 'Stare instalatory w Pobranych', poziom: 'sprawdz', sposob: 'kosz', wymaga_admina: false,
    wyjasnienie: 'Pliki .exe, .msi i .iso pobrane ponad miesiąc temu; program zwykle jest już zainstalowany, a instalator można w razie czego pobrać jeszcze raz.',
    sciezki: [`${U}\\Downloads`],
    filtr: (w) => /\.(exe|msi|iso)$/i.test(w.nazwa) && TERAZ - w.mtime > 30 * DZIEN },
  { id: 'hibernacja', nazwa: 'Plik hibernacji (hiberfil.sys)', poziom: 'sprawdz', sposob: 'narzedzie', wymaga_admina: true,
    polecenie: 'powercfg -h off',
    wyjasnienie: 'Miejsce na zapis pamięci, gdy komputer hibernuje; wyłączenie hibernacji je zwalnia, ale znika też szybkie uruchamianie Windows.',
    sciezki: ['C:\\hiberfil.sys'] },
  { id: 'plik-wymiany', nazwa: 'Plik wymiany (pagefile.sys, swapfile.sys)', poziom: 'nie-ruszaj', sposob: 'brak', wymaga_admina: false,
    wyjasnienie: 'Zapasowa pamięć operacyjna na dysku, której Windows potrzebuje; bez niej programy zaczną się wysypywać, więc zostaw ją w spokoju.',
    sciezki: ['C:\\pagefile.sys', 'C:\\swapfile.sys'] },
  { id: 'winsxs', nazwa: 'Magazyn składników Windows (WinSxS)', poziom: 'sprawdz', sposob: 'narzedzie', wymaga_admina: true,
    polecenie: 'Dism.exe /Online /Cleanup-Image /StartComponentCleanup',
    wyjasnienie: 'Części systemu potrzebne do aktualizacji i naprawy Windows; ręczne usuwanie psuje system, bezpiecznie odchudza go tylko narzędzie DISM.',
    sciezki: ['C:\\Windows\\WinSxS'], bezRozmiaru: true },
  { id: 'onedrive', nazwa: 'Pliki OneDrive trzymane na dysku', poziom: 'sprawdz', sposob: 'narzedzie', wymaga_admina: false,
    polecenie: 'attrib +U -P /S /D (w folderze OneDrive) albo „Zwolnij miejsce” w menu pliku',
    wyjasnienie: 'Pliki z chmury, które mają też kopię na tym komputerze; „Zwolnij miejsce” zostawia je w OneDrive, ale bez internetu ich nie otworzysz.',
    sciezki: [`${U}\\OneDrive`] },
];

function znaleziska(d: Drzewo): Znalezisko[] {
  const wynik: Znalezisko[] = [];
  for (const r of REGULY) {
    const miejsca: Miejsce[] = [];
    for (const s of r.sciezki) {
      const w = d.znajdz(s);
      if (!w) continue;
      if (r.filtr) {
        for (const p of d.pliki(w.id)) {
          if (!r.filtr(p)) continue;
          miejsca.push({ sciezka: p.sciezka, rozmiar: p.rozmiar, na_dysku: p.na_dysku, pliki: 1, bledy: 0 });
        }
        continue;
      }
      const uzytkownik = s.startsWith(U) ? 'Ania' : undefined;
      miejsca.push({
        sciezka: w.sciezka, ...(uzytkownik ? { uzytkownik } : {}),
        rozmiar: r.bezRozmiaru ? 0 : w.rozmiar, na_dysku: r.bezRozmiaru ? 0 : w.na_dysku,
        pliki: r.bezRozmiaru ? 0 : w.pliki, bledy: 0,
      });
    }
    if (!miejsca.length) continue;
    const suma = (k: 'rozmiar' | 'na_dysku' | 'pliki') => miejsca.reduce((a, m) => a + m[k], 0);
    wynik.push({
      id: r.id, nazwa: r.nazwa, wyjasnienie: r.wyjasnienie, poziom: r.poziom, sposob: r.sposob,
      ...(r.polecenie ? { polecenie: r.polecenie } : {}),
      wymaga_admina: r.wymaga_admina, rozmiar_znany: !r.bezRozmiaru,
      rozmiar: suma('rozmiar'), na_dysku: suma('na_dysku'), pliki: suma('pliki'), bledy: 0, miejsca,
    });
  }
  const kolejnosc = { bezpieczne: 0, sprawdz: 1, 'nie-ruszaj': 2 } as const;
  return wynik.sort((a, b) => kolejnosc[a.poziom] - kolejnosc[b.poziom] || b.na_dysku - a.na_dysku);
}

function duplikaty(d: Drzewo): WynikDuplikatow {
  const grupy = new Map<string, Wezel[]>();
  for (const w of d.pliki()) if (w.dup) grupy.set(w.dup, [...(grupy.get(w.dup) ?? []), w]);
  const lista: GrupaDuplikatow[] = [...grupy.entries()].map(([klucz, pliki]) => ({
    rozmiar: pliki[0].rozmiar,
    skrot: Array.from(klucz).map((c) => c.charCodeAt(0).toString(16)).join('').padEnd(16, '7a').slice(0, 16),
    pliki: pliki.map((p) => ({ sciezka: p.sciezka, na_dysku: p.na_dysku, mtime: p.mtime })),
    do_odzyskania: pliki[0].na_dysku * (pliki.length - 1),
  })).sort((a, b) => b.do_odzyskania - a.do_odzyskania);
  const kandydaci = [...d.pliki()].filter((w) => w.rozmiar > MB).length;
  return {
    grupy: lista, kandydaci, czytane_pliki: lista.reduce((a, g) => a + g.pliki.length, 0) + 24,
    czytane_bajty: lista.reduce((a, g) => a + g.rozmiar * g.pliki.length, 0), bledy: 0,
    do_odzyskania: lista.reduce((a, g) => a + g.do_odzyskania, 0), anulowano: false,
  };
}

const KATEGORIE: [string, string, string[]][] = [
  ['wideo', 'Filmy', ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'webm']],
  ['gry', 'Gry', []],
  ['obrazy', 'Zdjęcia i obrazy', ['jpg', 'jpeg', 'png', 'heic', 'gif', 'webp']],
  ['muzyka', 'Muzyka', ['mp3', 'flac', 'm4a', 'ogg', 'wav']],
  ['dokumenty', 'Dokumenty', ['pdf', 'docx', 'doc', 'xlsx', 'pptx', 'txt', 'odt']],
  ['archiwa', 'Archiwa', ['zip', 'rar', '7z']],
  ['instalatory', 'Instalatory', ['exe', 'msi', 'iso', 'cab']],
  ['kod', 'Kod i narzędzia programisty', ['js', 'ts', 'py', 'whl']],
  ['system', 'System', ['dll', 'sys', 'mum', 'manifest', 'edb']],
  ['inne', 'Inne', []],
];

function kategoria(w: Wezel): string {
  const s = w.sciezka.toLowerCase();
  if (s.includes('\\steamapps\\') || s.includes('\\my games\\')) return 'gry';
  if (s.startsWith('c:\\windows\\') || s.startsWith('c:\\windows.old\\') || /^c:\\(pagefile|hiberfil|swapfile)\.sys$/.test(s)) return 'system';
  if (s.includes('\\node_modules\\') || s.includes('\\npm-cache\\') || s.includes('\\pip\\')) return 'kod';
  const roz = w.nazwa.includes('.') ? w.nazwa.split('.').pop()!.toLowerCase() : '';
  for (const [id, , rozszerzenia] of KATEGORIE) if (rozszerzenia.includes(roz)) return id;
  return 'inne';
}

function kategorie(d: Drzewo): WynikKategorii[] {
  const mapa = new Map<string, { w: WynikKategorii; pliki: Wezel[] }>();
  for (const [id, nazwa] of KATEGORIE) mapa.set(id, { w: { kategoria: id, nazwa, rozmiar: 0, na_dysku: 0, pliki: 0, najwieksze: [] }, pliki: [] });
  for (const p of d.pliki()) {
    const m = mapa.get(kategoria(p))!;
    m.w.rozmiar += p.rozmiar;
    m.w.na_dysku += p.na_dysku;
    m.w.pliki += 1;
    m.pliki.push(p);
  }
  return [...mapa.values()]
    .filter((m) => m.w.pliki > 0)
    .map((m) => ({ ...m.w, najwieksze: m.pliki.sort((a, b) => b.na_dysku - a.na_dysku).slice(0, 5).map((p) => d.wiersz(p, true)) }))
    .sort((a, b) => b.na_dysku - a.na_dysku);
}

function stare(d: Drzewo, minMb: number, dni: number): StaryPlik[] {
  const wynik: StaryPlik[] = [];
  for (const p of d.pliki()) {
    const ile = Math.floor((TERAZ - p.mtime) / DZIEN);
    if (p.na_dysku < minMb * MB || ile < dni) continue;
    // Gry i system sa uzywane, choc pliki dawno nie zmieniane: data otwarcia jest swieza.
    const k = kategoria(p);
    if (k === 'gry' || k === 'system') continue;
    wynik.push({ sciezka: p.sciezka, rozmiar: p.rozmiar, na_dysku: p.na_dysku, zmieniony: p.mtime, otwarty: null, dni: ile });
  }
  return wynik.sort((a, b) => b.na_dysku - a.na_dysku).slice(0, 200);
}

// --- Backend ---

const czekaj = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Uproszczona kopia blokady z porzadki/ochrona.rs, zeby podglad zachowywal sie jak aplikacja. */
function zablokowana(sciezka: string): string | null {
  const s = sciezka.toLowerCase().replace(/\\+$/, '');
  if (/^[a-z]:$/.test(s)) return 'Korzeń dysku jest chroniony.';
  if (s.startsWith('c:\\windows')) {
    if (/^c:\\windows\\(temp|softwaredistribution\\download)(\\|$)/.test(s)) return null;
    return 'Folder Windows jest chroniony.';
  }
  if (/^c:\\program files( \(x86\))?(\\|$)/.test(s)) return 'Program Files jest chronione.';
  return null;
}

export function udawanyBackend(): Backend {
  const drzewa = new Map<string, Drzewo>();
  const drzewoDla = (sciezka: string) => {
    const klucz = sciezka.toUpperCase().startsWith('D') ? 'D' : 'C';
    if (!drzewa.has(klucz)) drzewa.set(klucz, new Drzewo(klucz === 'D' ? DRZEWO_D : DRZEWO_C));
    return drzewa.get(klucz)!;
  };
  let biezace: Drzewo | null = null;
  let anulowano = false;
  let trwa = false;
  const historia: WpisHistorii[] = [];

  // zReguly: miejsca regul sa w Rust wyjatkami blokady (Ochrona::dodaj_miejsce_reguly), wiec
  // zostaje tylko blokada korzenia dysku.
  const usunSciezki = (sciezki: string[], tryb: Tryb, opis: string, zReguly = false): WynikUsuwania => {
    const w: WynikUsuwania = { tryb, pozycje: 0, pliki: 0, rozmiar: 0, na_dysku: 0, pominiete: [], pominiete_razem: 0 };
    for (const s of sciezki) {
      const powod = zablokowana(s) && (!zReguly || /^[a-z]:\\?$/i.test(s)) ? zablokowana(s) : null;
      const d = biezace?.znajdz(s) ? biezace : c;
      const wezel = d.znajdz(s);
      if (powod || !wezel) {
        w.pominiete.push({ sciezka: s, powod: powod ?? 'Nie ma już takiego pliku.' });
        w.pominiete_razem++;
        continue;
      }
      w.pozycje++;
      w.pliki += wezel.rodzaj === 'plik' ? 1 : wezel.pliki;
      w.rozmiar += wezel.rozmiar;
      w.na_dysku += wezel.na_dysku;
      d.usun(wezel);
    }
    if (biezace && w.pozycje) w.korzen = biezace.wiersz(biezace.wezly[0], true);
    historia.push({ czas: Math.floor(Date.now() / 1000), rodzaj: 'usuwanie', opis, tryb, pozycje: w.pozycje, pliki: w.pliki, na_dysku: w.na_dysku, pominiete: w.pominiete_razem });
    return w;
  };

  const c = drzewoDla('C:\\');
  const dyski: Dysk[] = [
    { litera: 'C:', sciezka: 'C:\\', etykieta: 'Windows', rozmiar: 476 * GB + 312 * MB, wolne: 0, system_plikow: 'NTFS', rodzaj: 'staly' },
    { litera: 'D:', sciezka: 'D:\\', etykieta: 'Dane', rozmiar: 931 * GB + 524 * MB, wolne: 0, system_plikow: 'NTFS', rodzaj: 'staly' },
    { litera: 'E:', sciezka: 'E:\\', etykieta: 'PENDRIVE', rozmiar: 28 * GB + 840 * MB, wolne: 21 * GB + 128 * MB, system_plikow: 'exFAT', rodzaj: 'wymienny' },
  ];
  dyski[0].wolne = dyski[0].rozmiar - c.wezly[0].na_dysku;
  dyski[1].wolne = dyski[1].rozmiar - drzewoDla('D:\\').wezly[0].na_dysku;

  return {
    udawany: true,
    async listaDyskow() {
      await czekaj(120);
      return dyski.map((d) => ({ ...d }));
    },
    async skanuj(sciezka, postep) {
      if (trwa) throw new Error('Skan już trwa. Anuluj go albo poczekaj na koniec.');
      trwa = true;
      anulowano = false;
      const d = drzewoDla(sciezka);
      const korzen = d.wezly[0];
      const czas = 4200;
      const start = performance.now();
      let ostatnie = 0;
      for (;;) {
        await czekaj(100);
        const ms = performance.now() - start;
        // Nierowne tempo: szybko po malych plikach, wolniej na duzych katalogach gier.
        const t = Math.min(ms / czas, 1);
        const f = Math.min(1, t + 0.04 * Math.sin(t * 9) * (1 - t));
        const kat = d.katalogi[Math.min(d.katalogi.length - 1, Math.floor(f * d.katalogi.length))];
        const pliki = Math.round(korzen.pliki * f);
        postep({
          pliki, katalogi: Math.round(d.katalogi.length * f), bajty: Math.round(korzen.rozmiar * f), bledy: Math.round(3 * f),
          pliki_na_s: Math.round(((pliki - ostatnie) * 1000) / 100), czas_ms: Math.round(ms), biezacy: kat.sciezka,
          koniec: t >= 1 || anulowano,
        });
        ostatnie = pliki;
        if (t >= 1 || anulowano) break;
      }
      trwa = false;
      biezace = d;
      const wynik: Podsumowanie = {
        korzen: d.wiersz(korzen, true), pliki: korzen.pliki, katalogi: d.katalogi.length, dowiazania: 14,
        twarde_linki_pominiete: 0, inne_systemy: 0, bledy_razem: 3,
        bledy: [
          { sciezka: 'C:\\System Volume Information', powod: 'Odmowa dostępu' },
          { sciezka: 'C:\\Windows\\CSC', powod: 'Odmowa dostępu' },
          { sciezka: 'C:\\ProgramData\\Microsoft\\Crypto\\SystemKeys', powod: 'Odmowa dostępu' },
        ],
        czas_ms: Math.round(performance.now() - start), anulowano, pamiec_drzewa: d.wezly.length * 48,
      };
      return wynik;
    },
    async anuluj() {
      anulowano = true;
      return trwa;
    },
    async dzieci(idWezla) {
      if (!biezace) throw new Error('Najpierw uruchom skan.');
      const w = biezace.wezly[idWezla];
      if (!w) throw new Error(`Nie ma węzła ${idWezla}.`);
      return w.dzieci.map((id) => biezace!.wiersz(biezace!.wezly[id]));
    },
    async porzadki() {
      await czekaj(900);
      return znaleziska(c);
    },
    async duplikaty() {
      if (!biezace) throw new Error('Najpierw uruchom skan.');
      await czekaj(1400);
      return duplikaty(biezace);
    },
    async kategorie() {
      if (!biezace) throw new Error('Najpierw uruchom skan.');
      await czekaj(300);
      return kategorie(biezace);
    },
    async starePliki(minMb, dni) {
      if (!biezace) throw new Error('Najpierw uruchom skan.');
      await czekaj(250);
      return stare(biezace, minMb, dni);
    },
    async usun(sciezki, tryb, opis) {
      await czekaj(600);
      return usunSciezki(sciezki, tryb, opis);
    },
    async porzadkiUsun(ids, tryb) {
      await czekaj(900);
      const sciezki = znaleziska(c)
        .filter((z) => ids.includes(z.id) && z.sposob !== 'narzedzie' && z.id !== 'kosz' && z.poziom !== 'nie-ruszaj')
        .flatMap((z) => z.miejsca.map((m) => m.sciezka));
      return usunSciezki(sciezki, tryb, `Do sprzątnięcia: ${ids.length}`, true);
    },
    async narzedzie(id) {
      await czekaj(1200);
      const z = znaleziska(biezace ?? c).find((x) => x.id === id);
      const odzyskane = z?.na_dysku ?? 0;
      historia.push({ czas: Math.floor(Date.now() / 1000), rodzaj: 'narzedzie', opis: `${z?.nazwa ?? id}: zakończone`, pozycje: 1, pliki: 0, na_dysku: odzyskane, pominiete: 0 });
      return { id, ok: true, kod: 0, odzyskane, opis: 'zakończone' };
    },
    async historia() {
      return [...historia].reverse();
    },
    async admin() {
      return { admin: false, mozna_podniesc: true };
    },
    async uruchomJakoAdmin() {
      throw new Error('W podglądzie przeglądarki nie da się uruchomić aplikacji jako administrator.');
    },
    async autoskan() {
      return null;
    },
    async autoskanKoniec() {},
  };
}
