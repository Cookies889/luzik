// Wykres sloneczny na canvas: srodek to biezacy folder, pierscienie to kolejne poziomy.
// Klik w luk przybliza (450 ms), klik w srodek oddala. Przejscie liczy kazda klatke z jednego
// zlozonego ukladu (stary widok poza celem + nowy widok celu), wiec pierwsza i ostatnia klatka
// sa dokladnie widokami przed i po, bez skoku na koncu.

import type { Wiersz } from './api';
import { ODCIENIE, oklch } from './kolor';

export interface Wezel {
  w: Wiersz;
  rodzic: Wezel | null;
  dzieci: Wezel[] | null;
  odcien: number;
}

interface Luk {
  wezel: Wezel | null; // null = zlaczone male elementy
  rodzic: Wezel;
  d: number;
  a0: number;
  a1: number;
}

interface Widok {
  x0: number;
  x1: number;
  s: number;
}

const PIERSCIENIE = 4;
const MIN_KAT = 0.0035; // ulamek obrotu; mniejsze elementy ida do jednego szarego luku
export const CZAS_PRZEJSCIA = 450;

/** cubic-bezier(0.4, 0, 0.2, 1) */
function krzywa(t: number): number {
  const x1 = 0.4, y1 = 0, x2 = 0.2, y2 = 1;
  const bx = (u: number) => 3 * u * (1 - u) ** 2 * x1 + 3 * u ** 2 * (1 - u) * x2 + u ** 3;
  const by = (u: number) => 3 * u * (1 - u) ** 2 * y1 + 3 * u ** 2 * (1 - u) * y2 + u ** 3;
  let lo = 0, hi = 1, u = t;
  for (let i = 0; i < 24; i++) {
    u = (lo + hi) / 2;
    if (bx(u) < t) lo = u;
    else hi = u;
  }
  return by(u);
}

export interface OpcjeSunburst {
  pobierzDzieci(id: number): Promise<Wiersz[]>;
  naZmiane(centrum: Wezel): void;
  naNajechanie(w: Wezel | null, x: number, y: number): void;
}

export class Sunburst {
  korzen: Wezel;
  centrum: Wezel;
  private luki: Luk[] = [];
  private animacja = false;
  private podswietlony: Wezel | null = null;
  private ladowanie = new Map<number, Promise<Wezel[]>>();
  private ctx: CanvasRenderingContext2D;
  private bok = 0;
  private dpr = 1;
  private ostatniWidok: { luki: Luk[]; v: Widok } = { luki: [], v: { x0: 0, x1: 1, s: 0 } };
  readonly gotowy: Promise<void>;

  constructor(private canvas: HTMLCanvasElement, korzen: Wiersz, private opcje: OpcjeSunburst) {
    this.korzen = { w: korzen, rodzic: null, dzieci: null, odcien: ODCIENIE[0] };
    this.centrum = this.korzen;
    this.ctx = canvas.getContext('2d')!;
    new ResizeObserver(() => this.dopasuj()).observe(canvas.parentElement!);
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => this.rysujOstatni());
    canvas.addEventListener('mousemove', (e) => this.ruch(e));
    canvas.addEventListener('mouseleave', () => this.najedz(null, 0, 0));
    canvas.addEventListener('click', (e) => this.klik(e));
    this.dopasuj();
    this.gotowy = this.zaladuj(this.korzen, 1, 1).then(() => {
      this.luki = this.uklad(this.korzen, PIERSCIENIE + 1);
      this.rysuj(this.luki, { x0: 0, x1: 1, s: 0 });
      this.opcje.naZmiane(this.centrum);
    });
  }

  // --- dane ---

  async dzieci(n: Wezel): Promise<Wezel[]> {
    if (n.dzieci) return n.dzieci;
    if (n.w.rodzaj !== 'katalog' || n.w.liczba_dzieci === 0) return (n.dzieci = []);
    let p = this.ladowanie.get(n.w.id);
    if (!p) {
      p = this.opcje.pobierzDzieci(n.w.id).then((lista) => {
        const posort = [...lista].sort((a, b) => b.na_dysku - a.na_dysku);
        n.dzieci = posort.map((w, i) => ({
          w,
          rodzic: n,
          dzieci: null,
          odcien: n === this.korzen ? ODCIENIE[i % ODCIENIE.length] : n.odcien + przesuniecie(i),
        }));
        return n.dzieci;
      });
      this.ladowanie.set(n.w.id, p);
    }
    return p;
  }

  /** Laduje dzieci do glebokosci PIERSCIENIE + 1 (zapasowy pierscien wjezdza przy przejsciu). */
  private async zaladuj(n: Wezel, d: number, rozpietosc: number): Promise<void> {
    if (d > PIERSCIENIE + 1 || rozpietosc < MIN_KAT) return;
    const dz = await this.dzieci(n);
    const suma = sumaDzieci(n);
    if (suma <= 0) return;
    await Promise.all(dz.map((c) => this.zaladuj(c, d + 1, (rozpietosc * c.w.na_dysku) / suma)));
  }

  private uklad(centrum: Wezel, glebokosc: number): Luk[] {
    const luki: Luk[] = [];
    const rek = (n: Wezel, d: number, a0: number, a1: number) => {
      if (d > glebokosc || !n.dzieci?.length) return;
      const suma = sumaDzieci(n);
      if (suma <= 0) return;
      let a = a0;
      let reszta = 0;
      for (const c of n.dzieci) {
        const r = ((a1 - a0) * c.w.na_dysku) / suma;
        if (r < MIN_KAT) {
          reszta += r;
          continue;
        }
        luki.push({ wezel: c, rodzic: n, d, a0: a, a1: a + r });
        rek(c, d + 1, a, a + r);
        a += r;
      }
      if (reszta > 0) luki.push({ wezel: null, rodzic: n, d, a0: a, a1: Math.min(a1, a + reszta) });
    };
    rek(centrum, 1, 0, 1);
    return luki;
  }

  /** Zakres katowy potomka `cel` w ukladzie `od` (bez laczenia malych). */
  private zakres(od: Wezel, cel: Wezel): { x0: number; x1: number; k: number } {
    const droga: Wezel[] = [];
    for (let n: Wezel | null = cel; n && n !== od; n = n.rodzic) droga.unshift(n);
    let x0 = 0, x1 = 1;
    for (const n of droga) {
      const r = n.rodzic!;
      const suma = sumaDzieci(r);
      let przed = 0;
      for (const c of r.dzieci ?? []) {
        if (c === n) break;
        przed += c.w.na_dysku;
      }
      const szer = x1 - x0;
      x0 = x0 + (szer * przed) / suma;
      x1 = x0 + (szer * n.w.na_dysku) / suma;
    }
    return { x0, x1, k: droga.length };
  }

  // --- przejscia ---

  get trwaPrzejscie(): boolean {
    return this.animacja;
  }

  sciezka(n: Wezel = this.centrum): Wezel[] {
    const s: Wezel[] = [];
    for (let x: Wezel | null = n; x; x = x.rodzic) s.unshift(x);
    return s;
  }

  async przejdz(cel: Wezel): Promise<void> {
    if (this.animacja || cel === this.centrum) return;
    if (cel.w.rodzaj !== 'katalog' || cel.w.liczba_dzieci === 0) return;
    const wPoddrzewie = (n: Wezel, przodek: Wezel) => {
      for (let x: Wezel | null = n; x; x = x.rodzic) if (x === przodek) return true;
      return false;
    };
    let gora: Wezel, dol: Wezel, wGlab: boolean;
    if (wPoddrzewie(cel, this.centrum)) [gora, dol, wGlab] = [this.centrum, cel, true];
    else if (wPoddrzewie(this.centrum, cel)) [gora, dol, wGlab] = [cel, this.centrum, false];
    else return;

    this.animacja = true;
    await this.zaladuj(cel, 1, 1);
    const { x0, x1, k } = this.zakres(gora, dol);
    const lGora = this.uklad(gora, k + PIERSCIENIE + 1);
    const lDol = this.uklad(dol, PIERSCIENIE + 1);
    const zlozone = lGora
      .filter((l) => !wPoddrzewie(l.rodzic, dol))
      .concat(lDol.map((l) => ({ ...l, d: l.d + k, a0: x0 + l.a0 * (x1 - x0), a1: x0 + l.a1 * (x1 - x0) })));
    const pelny: Widok = { x0: 0, x1: 1, s: 0 };
    const przyblizony: Widok = { x0, x1, s: k };
    const [od, doW] = wGlab ? [pelny, przyblizony] : [przyblizony, pelny];

    this.centrum = cel;
    this.podswietlony = null;
    this.opcje.naNajechanie(null, 0, 0);
    this.opcje.naZmiane(cel);

    const czas = matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : CZAS_PRZEJSCIA;
    await new Promise<void>((gotowe) => {
      const start = performance.now();
      const klatka = (teraz: number) => {
        const t = czas ? Math.min(1, (teraz - start) / czas) : 1;
        const e = krzywa(t);
        this.rysuj(zlozone, {
          x0: od.x0 + (doW.x0 - od.x0) * e,
          x1: od.x1 + (doW.x1 - od.x1) * e,
          s: od.s + (doW.s - od.s) * e,
        });
        if (t < 1) requestAnimationFrame(klatka);
        else gotowe();
      };
      requestAnimationFrame(klatka);
    });
    this.luki = this.uklad(cel, PIERSCIENIE + 1);
    this.rysuj(this.luki, pelny);
    this.animacja = false;
  }

  oddal(): Promise<void> {
    return this.centrum.rodzic ? this.przejdz(this.centrum.rodzic) : Promise.resolve();
  }

  podswietl(n: Wezel | null): void {
    if (this.podswietlony === n) return;
    this.podswietlony = n;
    if (!this.animacja) this.rysuj(this.luki, { x0: 0, x1: 1, s: 0 });
  }

  // --- rysowanie ---

  private dopasuj(): void {
    const rodzic = this.canvas.parentElement!;
    const bok = Math.floor(Math.min(rodzic.clientWidth, rodzic.clientHeight));
    if (bok <= 0) return;
    this.dpr = window.devicePixelRatio || 1;
    this.bok = bok;
    this.canvas.style.width = `${bok}px`;
    this.canvas.style.height = `${bok}px`;
    this.canvas.width = Math.round(bok * this.dpr);
    this.canvas.height = Math.round(bok * this.dpr);
    rodzic.style.setProperty('--otwor', `${Math.round(this.promienie().r0 * 2)}px`);
    this.rysujOstatni();
  }

  private promienie() {
    const R = this.bok / 2 - 2;
    const r0 = R * 0.3;
    return { R, r0, w: (R - r0) / PIERSCIENIE };
  }

  private rysujOstatni(): void {
    this.rysuj(this.ostatniWidok.luki, this.ostatniWidok.v);
  }

  private rysuj(luki: Luk[], v: Widok): void {
    this.ostatniWidok = { luki, v };
    const ctx = this.ctx;
    const { R, r0, w } = this.promienie();
    const ciemny = matchMedia('(prefers-color-scheme: dark)').matches;
    const styl = getComputedStyle(this.canvas);
    const tlo = styl.getPropertyValue('--tlo').trim() || (ciemny ? '#15191c' : '#f6f7f8');
    const f = (dd: number) => (dd <= 1 ? r0 * Math.max(dd, 0) : r0 + (dd - 1) * w);

    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.bok, this.bok);
    const cx = this.bok / 2;
    const cy = this.bok / 2;
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, Math.PI * 2);
    ctx.clip();
    ctx.lineWidth = 1.25;
    ctx.strokeStyle = tlo;

    const szer = v.x1 - v.x0;
    for (const l of luki) {
      const a0 = Math.max(0, Math.min(1, (l.a0 - v.x0) / szer));
      const a1 = Math.max(0, Math.min(1, (l.a1 - v.x0) / szer));
      if (a1 - a0 < 1e-4) continue;
      const dd = l.d - v.s;
      if (dd <= 0 || dd >= PIERSCIENIE + 1) continue;
      const alfa = Math.min(1, dd, PIERSCIENIE + 1 - dd);
      const ri = f(dd);
      const ro = Math.min(f(dd + 1), R + 1);
      if (ro - ri < 0.5) continue;
      ctx.fillStyle = this.kolor(l, dd, ciemny, alfa);
      const s = a0 * Math.PI * 2 - Math.PI / 2;
      const e = a1 * Math.PI * 2 - Math.PI / 2;
      ctx.beginPath();
      ctx.arc(cx, cy, ro, s, e);
      ctx.arc(cx, cy, ri, e, s, true);
      ctx.closePath();
      ctx.fill();
      if ((a1 - a0) * ro * Math.PI * 2 > 3) ctx.stroke();
    }
    ctx.restore();
  }

  private kolor(l: Luk, dd: number, ciemny: boolean, alfa: number): string {
    const hover = l.wezel !== null && l.wezel === this.podswietlony;
    const baza = ciemny ? 0.68 : 0.66;
    const krok = ciemny ? 0.045 : 0.055;
    const jasnosc = Math.min(0.9, baza + (dd - 1) * krok + (hover ? 0.1 : 0));
    if (!l.wezel) return oklch(ciemny ? 0.42 : 0.84, 0.008, 220, alfa);
    const plik = l.wezel.w.rodzaj !== 'katalog';
    return oklch(jasnosc, plik ? 0.045 : 0.12, l.wezel.odcien, alfa);
  }

  // --- mysz ---

  private trafienie(e: MouseEvent): Luk | 'srodek' | null {
    const r = this.canvas.getBoundingClientRect();
    const dx = e.clientX - r.left - this.bok / 2;
    const dy = e.clientY - r.top - this.bok / 2;
    const odl = Math.hypot(dx, dy);
    const { R, r0, w } = this.promienie();
    if (odl < r0) return 'srodek';
    if (odl > R) return null;
    const d = 1 + Math.floor((odl - r0) / w);
    let a = (Math.atan2(dy, dx) + Math.PI / 2) / (Math.PI * 2);
    if (a < 0) a += 1;
    return this.luki.find((l) => l.d === d && a >= l.a0 && a < l.a1) ?? null;
  }

  private ruch(e: MouseEvent): void {
    if (this.animacja) return;
    const t = this.trafienie(e);
    const n = t && t !== 'srodek' ? t.wezel : null;
    this.canvas.style.cursor =
      t === 'srodek' ? (this.centrum.rodzic ? 'zoom-out' : 'default') : n && n.w.liczba_dzieci > 0 ? 'zoom-in' : 'default';
    this.najedz(n, e.clientX, e.clientY);
  }

  private najedz(n: Wezel | null, x: number, y: number): void {
    this.podswietl(n);
    this.opcje.naNajechanie(n, x, y);
  }

  private klik(e: MouseEvent): void {
    const t = this.trafienie(e);
    if (t === 'srodek') void this.oddal();
    else if (t?.wezel) void this.przejdz(t.wezel);
  }
}

function sumaDzieci(n: Wezel): number {
  const s = (n.dzieci ?? []).reduce((a, c) => a + c.w.na_dysku, 0);
  return Math.max(s, n.w.na_dysku);
}

/** Rodzenstwo ma odcienie blisko rodzica: 0, +12, -12, +24, -24, potem w kolko. */
function przesuniecie(i: number): number {
  if (i === 0) return 0;
  const krok = Math.ceil(i / 2) * 12;
  const p = ((krok - 12) % 36) + 12;
  return i % 2 ? p : -p;
}
