// Liczby po polsku: spacja nierozdzielajaca w tysiacach, przecinek dziesietny.
// Jednostki dwojkowe z nazwami jak w Eksploratorze Windows (1 GB = 1024 MB), zeby liczby sie zgadzaly.

const NBSP = ' ';
const calk = new Intl.NumberFormat('pl-PL', { maximumFractionDigits: 0, useGrouping: 'always' } as unknown as Intl.NumberFormatOptions);
const jeden = new Intl.NumberFormat('pl-PL', { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: 'always' } as unknown as Intl.NumberFormatOptions);

export function liczba(n: number): string {
  return calk.format(Math.round(n)).replace(/\s/g, NBSP);
}

const JEDNOSTKI = ['B', 'KB', 'MB', 'GB', 'TB'];

/** 0 B, 812 KB, 4,3 GB, 70,9 GB, 1,2 TB. */
export function rozmiar(b: number): string {
  if (!Number.isFinite(b) || b <= 0) return `0${NBSP}B`;
  let i = 0;
  let v = b;
  while (v >= 1024 && i < JEDNOSTKI.length - 1) {
    v /= 1024;
    i++;
  }
  const tekst = i === 0 ? liczba(v) : v >= 100 ? liczba(v) : jeden.format(v).replace(/\s/g, NBSP);
  return `${tekst}${NBSP}${JEDNOSTKI[i]}`;
}

/** Rozmiar w zadanej jednostce (licznik, ktory nie skacze miedzy MB i GB). */
export function wJednostce(b: number, jednostka: 'GB' | 'MB'): string {
  const v = b / (jednostka === 'GB' ? 1024 ** 3 : 1024 ** 2);
  return jeden.format(v).replace(/\s/g, NBSP);
}

export function procent(czesc: number, calosc: number): string {
  if (calosc <= 0) return '0%';
  const p = (czesc / calosc) * 100;
  if (p > 0 && p < 1) return '<1%';
  return `${Math.round(p)}%`;
}

/** Polska odmiana: 1 plik, 2 pliki, 5 plików, 22 pliki, 112 plików. */
export function odmiana(n: number, jeden: string, kilka: string, wiele: string): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (n === 1) return jeden;
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return kilka;
  return wiele;
}

export const pliki = (n: number) => `${liczba(n)}${NBSP}${odmiana(n, 'plik', 'pliki', 'plików')}`;
export const foldery = (n: number) => `${liczba(n)}${NBSP}${odmiana(n, 'folder', 'foldery', 'folderów')}`;

export function czas(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}${NBSP}s`;
  return `${Math.floor(s / 60)}${NBSP}min ${s % 60}${NBSP}s`;
}

export function wiek(dni: number): string {
  if (dni < 60) return `${dni}${NBSP}${odmiana(dni, 'dzień', 'dni', 'dni')}`;
  const mies = Math.floor(dni / 30.44);
  if (mies < 24) return `${mies}${NBSP}${odmiana(mies, 'miesiąc', 'miesiące', 'miesięcy')}`;
  const lata = Math.floor(dni / 365.25);
  return `${lata}${NBSP}${odmiana(lata, 'rok', 'lata', 'lat')}`;
}

export function data(sekundy: number): string {
  return new Date(sekundy * 1000).toLocaleDateString('pl-PL', { day: 'numeric', month: 'short', year: 'numeric' });
}

/** Skraca sciezke ze srodka, zostawiajac poczatek dysku i koniec. */
export function skrocSciezke(s: string, max = 72): string {
  if (s.length <= max) return s;
  const koniec = s.slice(-(max - 8));
  const ciecie = koniec.indexOf('\\');
  return `${s.slice(0, 3)}…${ciecie > 0 && ciecie < 20 ? koniec.slice(ciecie) : koniec}`;
}

export function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}
