// Typy danych z silnika (pola jak w serde po stronie Rust) i wybor backendu:
// w oknie Tauri prawdziwe komendy, w zwyklej przegladarce udawany backend z ui/src/udawany.ts.

export interface Dysk {
  litera: string;
  sciezka: string;
  etykieta: string;
  rozmiar: number;
  wolne: number;
  system_plikow: string;
  rodzaj: string;
}

export interface Wiersz {
  id: number;
  nazwa: string;
  rodzaj: 'plik' | 'katalog' | 'dowiazanie' | 'inny';
  rozmiar: number;
  na_dysku: number;
  pliki: number;
  mtime: number;
  liczba_dzieci: number;
  duplikat: boolean;
  niedostepny: boolean;
  inny_system: boolean;
  sciezka?: string;
}

export interface Postep {
  pliki: number;
  katalogi: number;
  bajty: number;
  bledy: number;
  pliki_na_s: number;
  czas_ms: number;
  biezacy: string;
  koniec: boolean;
}

export interface Podsumowanie {
  korzen: Wiersz;
  pliki: number;
  katalogi: number;
  dowiazania: number;
  twarde_linki_pominiete: number;
  inne_systemy: number;
  bledy_razem: number;
  bledy: { sciezka: string; powod: string }[];
  czas_ms: number;
  anulowano: boolean;
  pamiec_drzewa: number;
}

export type Poziom = 'bezpieczne' | 'sprawdz' | 'nie-ruszaj';
export type Sposob = 'kosz' | 'trwale' | 'narzedzie' | 'brak';

export interface Miejsce {
  sciezka: string;
  uzytkownik?: string;
  rozmiar: number;
  na_dysku: number;
  pliki: number;
  bledy: number;
}

export interface Znalezisko {
  id: string;
  nazwa: string;
  wyjasnienie: string;
  poziom: Poziom;
  sposob: Sposob;
  polecenie?: string;
  wymaga_admina: boolean;
  rozmiar_znany: boolean;
  rozmiar: number;
  na_dysku: number;
  pliki: number;
  bledy: number;
  miejsca: Miejsce[];
}

export interface GrupaDuplikatow {
  rozmiar: number;
  skrot: string;
  pliki: { sciezka: string; na_dysku: number; mtime: number }[];
  do_odzyskania: number;
}

export interface WynikDuplikatow {
  grupy: GrupaDuplikatow[];
  kandydaci: number;
  czytane_pliki: number;
  czytane_bajty: number;
  bledy: number;
  do_odzyskania: number;
  anulowano: boolean;
}

export interface WynikKategorii {
  kategoria: string;
  nazwa: string;
  rozmiar: number;
  na_dysku: number;
  pliki: number;
  najwieksze: Wiersz[];
}

export interface StaryPlik {
  sciezka: string;
  rozmiar: number;
  na_dysku: number;
  zmieniony: number;
  otwarty: number | null;
  dni: number;
}

export type Tryb = 'kosz' | 'trwale';

export interface Pominiety {
  sciezka: string;
  powod: string;
}

export interface WynikUsuwania {
  tryb: Tryb;
  pozycje: number;
  pliki: number;
  rozmiar: number;
  na_dysku: number;
  pominiete: Pominiety[];
  pominiete_razem: number;
  /** Korzen skanu po zmianach, gdy usuniete sciezki lezaly w skanie. */
  korzen?: Wiersz;
}

export interface WynikNarzedzia {
  id: string;
  ok: boolean;
  kod?: number;
  odzyskane: number;
  opis: string;
}

export interface WpisHistorii {
  czas: number;
  rodzaj: 'usuwanie' | 'narzedzie';
  opis: string;
  tryb?: Tryb;
  pozycje: number;
  pliki: number;
  na_dysku: number;
  pominiete: number;
}

export interface StanAdmina {
  admin: boolean;
  mozna_podniesc: boolean;
}

/** Wspolny interfejs prawdziwego i udawanego backendu. */
export interface Backend {
  udawany: boolean;
  listaDyskow(): Promise<Dysk[]>;
  skanuj(sciezka: string, postep: (p: Postep) => void): Promise<Podsumowanie>;
  anuluj(): Promise<boolean>;
  dzieci(idWezla: number): Promise<Wiersz[]>;
  porzadki(): Promise<Znalezisko[]>;
  duplikaty(): Promise<WynikDuplikatow>;
  kategorie(): Promise<WynikKategorii[]>;
  starePliki(minMb: number, dni: number): Promise<StaryPlik[]>;
  usun(sciezki: string[], tryb: Tryb, opis: string): Promise<WynikUsuwania>;
  porzadkiUsun(ids: string[], tryb: Tryb): Promise<WynikUsuwania>;
  narzedzie(id: string): Promise<WynikNarzedzia>;
  historia(): Promise<WpisHistorii[]>;
  admin(): Promise<StanAdmina>;
  uruchomJakoAdmin(): Promise<void>;
  /** Sciezka z LUZIK_AUTOSKAN (samosprawdzenie prawdziwego okna) albo null. */
  autoskan(): Promise<string | null>;
  autoskanKoniec(wynik: unknown): Promise<void>;
}

function wTauri(): boolean {
  return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window);
}

async function backendTauri(): Promise<Backend> {
  const { invoke } = await import('@tauri-apps/api/core');
  const { listen } = await import('@tauri-apps/api/event');
  return {
    udawany: false,
    listaDyskow: () => invoke<Dysk[]>('lista_dyskow'),
    async skanuj(sciezka, postep) {
      const odepnij = await listen<Postep>('skan-postep', (e) => postep(e.payload));
      try {
        return await invoke<Podsumowanie>('skanuj', { sciezka });
      } finally {
        odepnij();
      }
    },
    anuluj: () => invoke<boolean>('anuluj'),
    dzieci: (idWezla) => invoke<Wiersz[]>('dzieci', { idWezla }),
    porzadki: () => invoke<Znalezisko[]>('porzadki_skanuj'),
    duplikaty: () => invoke<WynikDuplikatow>('duplikaty'),
    kategorie: () => invoke<WynikKategorii[]>('kategorie'),
    starePliki: (minMb, dni) => invoke<StaryPlik[]>('stare_pliki', { minMb, dni, n: 200 }),
    usun: (sciezki, tryb, opis) => invoke<WynikUsuwania>('usun', { sciezki, tryb, opis }),
    porzadkiUsun: (ids, tryb) => invoke<WynikUsuwania>('porzadki_usun', { ids, tryb }),
    narzedzie: (id) => invoke<WynikNarzedzia>('narzedzie', { id }),
    historia: () => invoke<WpisHistorii[]>('historia'),
    admin: () => invoke<StanAdmina>('admin'),
    uruchomJakoAdmin: () => invoke<void>('uruchom_jako_admin'),
    autoskan: async () => (await invoke<string | null>('autoskan')) ?? null,
    autoskanKoniec: (wynik) => invoke<void>('autoskan_koniec', { wynik }),
  };
}

export async function wybierzBackend(): Promise<Backend> {
  if (wTauri()) return backendTauri();
  const { udawanyBackend } = await import('./udawany');
  return udawanyBackend();
}
