// Luzik: trzy ekrany (dyski, skan, wynik) i piec zakladek wyniku.
// Backend wybiera api.ts: Tauri w oknie aplikacji, udawany w zwyklej przegladarce.

import './styl.css';
import type {
  Backend, Dysk, Pominiety, Podsumowanie, Postep, StanAdmina, StaryPlik, Tryb, WpisHistorii,
  WynikDuplikatow, WynikKategorii, WynikUsuwania, Znalezisko,
} from './api';
import { wybierzBackend } from './api';
import {
  czas, data, esc, liczba, odmiana, pliki, procent, rozmiar, skrocSciezke, wiek, wJednostce,
} from './format';
import { Sunburst, type Wezel } from './sunburst';
import { oklch } from './kolor';
import { sledzAktualizacje } from './aktualizacje';

const app = document.getElementById('app')!;
let backend: Backend;

// Komendy silnika, ktore zajmuja dysk (porzadki, duplikaty), ida po kolei: Rust odrzuca dwie naraz.
let kolejka: Promise<unknown> = Promise.resolve();
function poKolei<T>(f: () => Promise<T>): Promise<T> {
  const p = kolejka.then(f, f);
  kolejka = p.catch(() => undefined);
  return p;
}

// --- wspolne kawalki ---

let stanAdmina: StanAdmina = { admin: false, mozna_podniesc: false };

const naglowek = (prawa = '') => `
  <header class="gora">
    <div class="marka"><img src="./ikona.svg" alt="" width="24" height="24"><span>Luzik</span></div>
    ${backend.udawany ? '<span class="znacznik" title="Okno działa w przeglądarce, bez silnika Tauri">dane przykładowe</span>' : ''}
    <div class="gora-prawa">${prawa}<button class="drugi" data-akcja="historia">Historia</button>${
      stanAdmina.mozna_podniesc
        ? '<button class="drugi" data-akcja="admin" title="Skan zobaczy też foldery systemu i innych użytkowników">Uruchom jako administrator</button>'
        : ''
    }</div>
  </header>`;

// --- okna: potwierdzenie, historia, komunikat ---

function okno(html: string): HTMLDialogElement {
  const d = document.createElement('dialog');
  d.className = 'okno';
  d.innerHTML = html;
  document.body.append(d);
  d.addEventListener('close', () => d.remove());
  d.showModal();
  return d;
}

function komunikat(tytul: string, tekst: string) {
  okno(`<form method="dialog"><h2 class="okno-tytul">${esc(tytul)}</h2><p class="okno-tekst">${esc(tekst)}</p>
    <div class="okno-akcje"><button class="glowny" value="ok" autofocus>Zamknij</button></div></form>`);
}

interface Pozycja {
  nazwa: string;
  sciezka?: string;
  bajty: number;
  opis?: string;
}

/** Okno potwierdzenia: lista, suma i tryb (domyslnie Kosz). Zwraca tryb albo null po Anuluj. */
function potwierdz(o: { tytul: string; pozycje: Pozycja[]; zTrybem: boolean; uwaga?: string }): Promise<Tryb | null> {
  const suma = o.pozycje.reduce((a, p) => a + p.bajty, 0);
  const n = o.pozycje.length;
  const POKAZ = 200;
  const d = okno(`
    <form method="dialog" class="potwierdzenie">
      <h2 class="okno-tytul">${esc(o.tytul)}</h2>
      <div class="pt-suma"><b>${rozmiar(suma)}</b><span class="slaby">${liczba(n)} ${odmiana(n, 'pozycja', 'pozycje', 'pozycji')}</span></div>
      <ul class="pt-lista">${o.pozycje
        .slice(0, POKAZ)
        .map(
          (p) => `<li><span class="pt-nazwa"><span class="pt-n">${esc(p.nazwa)}</span>${p.sciezka ? `<span class="sciezka slaby" title="${esc(p.sciezka)}">${esc(p.sciezka)}</span>` : ''}${p.opis ? `<span class="slaby pt-opis">${esc(p.opis)}</span>` : ''}</span><span class="pt-rozmiar">${rozmiar(p.bajty)}</span></li>`,
        )
        .join('')}${n > POKAZ ? `<li class="slaby">i ${liczba(n - POKAZ)} więcej</li>` : ''}</ul>
      ${
        o.zTrybem
          ? `<fieldset class="tryb"><legend class="etykieta">Tryb</legend>
        <label><input type="radio" name="tryb" value="kosz" checked><span><b>Do Kosza</b><span class="slaby">Można przywrócić z Kosza.</span></span></label>
        <label><input type="radio" name="tryb" value="trwale"><span><b>Na stałe</b><span class="slaby">Bez Kosza, nie da się cofnąć.</span></span></label>
      </fieldset>`
          : ''
      }
      ${o.uwaga ? `<p class="pt-uwaga slaby">${esc(o.uwaga)}</p>` : ''}
      <div class="okno-akcje"><button class="drugi" value="nie" autofocus>Anuluj</button><button class="glowny pt-tak" value="tak"></button></div>
    </form>`);
  const tak = d.querySelector<HTMLButtonElement>('.pt-tak')!;
  const wybrany = () => (d.querySelector<HTMLInputElement>('input[name=tryb]:checked')?.value as Tryb | undefined) ?? null;
  const napis = () => (tak.textContent = !o.zTrybem ? 'Uruchom' : wybrany() === 'trwale' ? 'Usuń na stałe' : 'Przenieś do Kosza');
  d.querySelectorAll('input[name=tryb]').forEach((r) => r.addEventListener('change', napis));
  napis();
  return new Promise((res) => d.addEventListener('close', () => res(d.returnValue === 'tak' ? wybrany() ?? 'kosz' : null)));
}

async function oknoHistorii() {
  let h: WpisHistorii[];
  try {
    h = await backend.historia();
  } catch (e) {
    return komunikat('Historia porządków', String((e as Error)?.message ?? e));
  }
  const opis = (w: WpisHistorii) =>
    w.rodzaj === 'narzedzie'
      ? 'narzędzie Windows'
      : `${pliki(w.pliki)}, ${w.tryb === 'trwale' ? 'na stałe' : 'do Kosza'}${w.pominiete ? `, pominięte: ${liczba(w.pominiete)}` : ''}`;
  okno(`<form method="dialog" class="historia">
    <h2 class="okno-tytul">Historia porządków</h2>
    ${
      h.length
        ? `<ol class="hist">${h.map((w) => `<li><span class="h-opis"><b>${esc(w.opis)}</b><span class="slaby">${data(w.czas)}, ${opis(w)}</span></span><span class="h-rozmiar">${rozmiar(w.na_dysku)}</span></li>`).join('')}</ol>`
        : '<p class="okno-tekst slaby">Jeszcze nic nie usunięto.</p>'
    }
    <div class="okno-akcje"><button class="glowny" value="ok" autofocus>Zamknij</button></div>
  </form>`);
}

// --- wynik usuwania ---

interface Odzysk {
  tryb: Tryb | null;
  na_dysku: number;
  pozycje: number;
  pliki: number;
  narzedzia: number;
  pominiete: Pominiety[];
  pominiete_razem: number;
}

const nowyOdzysk = (tryb: Tryb | null): Odzysk => ({ tryb, na_dysku: 0, pozycje: 0, pliki: 0, narzedzia: 0, pominiete: [], pominiete_razem: 0 });

function dolacz(o: Odzysk, w: WynikUsuwania) {
  o.na_dysku += w.na_dysku;
  o.pozycje += w.pozycje;
  o.pliki += w.pliki;
  o.pominiete.push(...w.pominiete);
  o.pominiete_razem += w.pominiete_razem;
  poUsunieciu?.(w);
}

/** Pasek „Odzyskano” z licznikiem, ktory dojezdza od zera, i lista pominietych z powodem. */
function pokazOdzysk(gdzie: HTMLElement, o: Odzysk) {
  const el = document.createElement('div');
  el.className = 'odzyskano';
  el.setAttribute('role', 'status');
  const czesci = [
    o.pliki ? `${pliki(o.pliki)} ${o.tryb === 'trwale' ? 'usunięte na stałe' : 'w Koszu'}` : '',
    o.narzedzia ? `narzędzia Windows: ${liczba(o.narzedzia)}` : '',
  ].filter(Boolean);
  const gdzieTrafilo = czesci.length ? czesci.join(', ') : 'Nic nie usunięto';
  el.innerHTML = `
    <div><p class="etykieta">Odzyskano</p><p class="odzyskano-suma">0 B</p></div>
    <p class="slaby odzyskano-opis">${gdzieTrafilo}. <button class="link" data-akcja="historia">Wpis w historii</button></p>
    ${
      o.pominiete_razem
        ? `<details class="pominiete"><summary>Pominięte: ${liczba(o.pominiete_razem)}</summary><ul>${o.pominiete
            .map((p) => `<li><span class="sciezka" title="${esc(p.sciezka)}">${esc(p.sciezka)}</span><span class="slaby">${esc(p.powod)}</span></li>`)
            .join('')}</ul></details>`
        : ''
    }`;
  gdzie.prepend(el);
  licznik(el.querySelector<HTMLElement>('.odzyskano-suma')!, rozmiar, 600)(o.na_dysku);
}

/** Ustawia ekran wyniku po usunieciu (nowy korzen, panele do przeliczenia). */
let poUsunieciu: ((w: WynikUsuwania) => void) | null = null;
let ostatniePodsumowanie: Podsumowanie | null = null;

function blad(miejsce: HTMLElement, e: unknown, ponow?: () => void) {
  miejsce.innerHTML = `<div class="pusto"><p>${esc(String((e as Error)?.message ?? e))}</p>${ponow ? '<button class="drugi">Spróbuj jeszcze raz</button>' : ''}</div>`;
  miejsce.querySelector('button')?.addEventListener('click', () => ponow?.());
}

/** Licznik, ktory dojezdza do wartosci zamiast skakac (dla sumy do odzyskania). */
function licznik(el: HTMLElement, format: (n: number) => string, czasMs = 400) {
  let pokazane = 0;
  let cel = 0;
  let ramka = 0;
  const krok = (od: number, start: number) => (t: number) => {
    const p = Math.min(1, (t - start) / czasMs);
    const e = 1 - (1 - p) ** 3;
    pokazane = od + (cel - od) * e;
    el.textContent = format(pokazane);
    if (p < 1) ramka = requestAnimationFrame(krok(od, start));
  };
  return (n: number) => {
    cel = n;
    cancelAnimationFrame(ramka);
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      pokazane = n;
      el.textContent = format(n);
      return;
    }
    const start = performance.now();
    ramka = requestAnimationFrame(krok(pokazane, start));
  };
}

function kaskada(kontener: Element) {
  [...kontener.children].forEach((el, i) => {
    (el as HTMLElement).style.setProperty('--i', String(Math.min(i, 8)));
    el.classList.add('wejscie');
  });
}

// --- ekran 1: dyski ---

async function ekranDyskow() {
  app.innerHTML = `${naglowek()}<main class="dyski"><h1 class="etykieta">Dyski</h1><div class="lista-dyskow" aria-busy="true"></div></main>`;
  const lista = app.querySelector<HTMLElement>('.lista-dyskow')!;
  let dyski: Dysk[];
  try {
    dyski = await backend.listaDyskow();
  } catch (e) {
    return blad(lista, e, ekranDyskow);
  }
  lista.removeAttribute('aria-busy');
  if (!dyski.length) {
    lista.innerHTML = '<div class="pusto"><p>Nie widać żadnego dysku. Sprawdź uprawnienia aplikacji.</p></div>';
    return;
  }
  lista.innerHTML = dyski
    .map((d) => {
      const zajete = d.rozmiar - d.wolne;
      const ulamek = d.rozmiar ? zajete / d.rozmiar : 0;
      const malo = ulamek > 0.9;
      return `
      <article class="dysk${malo ? ' malo' : ''}">
        <div class="dysk-gora">
          <span class="litera">${esc(d.litera)}</span>
          <span class="dysk-nazwa">${esc(d.etykieta || 'Dysk lokalny')}</span>
          <span class="dysk-fs">${esc(d.system_plikow)}${d.rodzaj === 'wymienny' ? ', wymienny' : ''}</span>
        </div>
        <div class="pasek" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(ulamek * 100)}" aria-label="Zajętość ${esc(d.litera)}">
          <span style="--u:${ulamek.toFixed(4)}"></span>
        </div>
        <div class="dysk-liczby">
          <span><b>${rozmiar(d.wolne)}</b> wolne</span>
          <span class="slaby">z ${rozmiar(d.rozmiar)}, zajęte ${procent(zajete, d.rozmiar)}</span>
        </div>
        <button class="glowny" data-sciezka="${esc(d.sciezka)}">Skanuj ${esc(d.litera)}</button>
      </article>`;
    })
    .join('');
  kaskada(lista);
  lista.querySelectorAll<HTMLButtonElement>('button[data-sciezka]').forEach((b, i) =>
    b.addEventListener('click', () => ekranSkanu(dyski[i])),
  );
}

// --- ekran 2: skan ---

async function ekranSkanu(dysk: Dysk) {
  const zajete = Math.max(1, dysk.rozmiar - dysk.wolne);
  app.innerHTML = `${naglowek()}
  <main class="skan">
    <p class="etykieta">Skanowanie ${esc(dysk.litera)}</p>
    <div class="skan-licznik"><span class="gb">0,0</span><span class="jedn">GB</span></div>
    <p class="slaby skan-z">z ${rozmiar(zajete)} zajętych na dysku</p>
    <div class="pasek skan-pasek"><span style="--u:0"></span></div>
    <dl class="skan-liczby">
      <div><dt>Pliki</dt><dd class="l-pliki">0</dd></div>
      <div><dt>Foldery</dt><dd class="l-foldery">0</dd></div>
      <div><dt>Czas</dt><dd class="l-czas">0 s</dd></div>
    </dl>
    <p class="biezacy" title=""></p>
    <button class="drugi anuluj">Anuluj</button>
  </main>`;
  const $ = <T extends HTMLElement>(s: string) => app.querySelector<T>(s)!;
  const gb = $('.gb');
  const pasek = $<HTMLSpanElement>('.skan-pasek span');
  const lPliki = $('.l-pliki');
  const lFoldery = $('.l-foldery');
  const lCzas = $('.l-czas');
  const biez = $('.biezacy');

  // Zdarzenia przychodza co 100 ms; wyswietlane liczby gonia je co klatke, wiec rosna plynnie.
  const cel: Postep = { pliki: 0, katalogi: 0, bajty: 0, bledy: 0, pliki_na_s: 0, czas_ms: 0, biezacy: '', koniec: false };
  const widac = { pliki: 0, katalogi: 0, bajty: 0 };
  let dziala = true;
  let poprzednia = performance.now();
  const klatka = (t: number) => {
    const dt = t - poprzednia;
    poprzednia = t;
    const k = 1 - Math.exp(-dt / 140);
    widac.pliki += (cel.pliki - widac.pliki) * k;
    widac.katalogi += (cel.katalogi - widac.katalogi) * k;
    widac.bajty += (cel.bajty - widac.bajty) * k;
    gb.textContent = wJednostce(widac.bajty, 'GB');
    lPliki.textContent = liczba(widac.pliki);
    lFoldery.textContent = liczba(widac.katalogi);
    pasek.style.setProperty('--u', Math.min(0.99, widac.bajty / zajete).toFixed(4));
    if (dziala) requestAnimationFrame(klatka);
  };
  requestAnimationFrame(klatka);

  $('.anuluj').addEventListener('click', async (e) => {
    (e.currentTarget as HTMLButtonElement).disabled = true;
    (e.currentTarget as HTMLButtonElement).textContent = 'Zatrzymuję…';
    await backend.anuluj();
  });

  let podsumowanie: Podsumowanie;
  try {
    podsumowanie = await backend.skanuj(dysk.sciezka, (p) => {
      Object.assign(cel, p);
      lCzas.textContent = czas(p.czas_ms);
      biez.textContent = skrocSciezke(p.biezacy, 80);
      biez.title = p.biezacy;
    });
  } catch (e) {
    dziala = false;
    return blad(app.querySelector('main')!, e, () => ekranSkanu(dysk));
  }
  dziala = false;
  if (podsumowanie.anulowano) return ekranDyskow();
  ekranWyniku(dysk, podsumowanie);
}

// --- ekran 3: wynik ---

type Zakladka = 'mapa' | 'porzadki' | 'duplikaty' | 'stare' | 'typy';
const ZAKLADKI: [Zakladka, string][] = [
  ['mapa', 'Mapa dysku'],
  ['porzadki', 'Do sprzątnięcia'],
  ['duplikaty', 'Duplikaty'],
  ['stare', 'Duże i stare pliki'],
  ['typy', 'Typy plików'],
];

function ekranWyniku(dysk: Dysk, p: Podsumowanie) {
  ostatniePodsumowanie = p;
  const opisSkanu = () =>
    `${esc(dysk.litera)} ${rozmiar(p.korzen.na_dysku)}, ${pliki(p.korzen.pliki)}, ${czas(p.czas_ms)}${p.bledy_razem ? `, bez dostępu: ${liczba(p.bledy_razem)}` : ''}`;
  const opis = opisSkanu();
  // Po usunieciu drzewo w Rust jest juz poprawione: nowy korzen, a panele liczone z drzewa
  // (mapa, typy, duze i stare) buduja sie od nowa przy nastepnym otwarciu.
  poUsunieciu = (w) => {
    if (!w.korzen) return;
    p.korzen = w.korzen;
    const pod = app.querySelector('.podsumowanie');
    if (pod) pod.innerHTML = opisSkanu();
    for (const z of ['mapa', 'typy', 'stare'] as Zakladka[]) {
      const el = panele.get(z);
      if (!el || !el.hidden) continue;
      el.remove();
      panele.delete(z);
    }
  };
  app.innerHTML = `${naglowek(`<span class="podsumowanie slaby">${opis}</span><button class="drugi zmien">Zmień dysk</button>`)}
  <nav class="zakladki" role="tablist">
    ${ZAKLADKI.map(([id, nazwa], i) => `<button role="tab" data-z="${id}" aria-selected="${i === 0}">${nazwa}<span class="z-liczba" data-licznik="${id}"></span></button>`).join('')}
  </nav>
  <section class="tresc"></section>`;
  app.querySelector('.zmien')!.addEventListener('click', ekranDyskow);
  const tresc = app.querySelector<HTMLElement>('.tresc')!;
  const panele = new Map<Zakladka, HTMLElement>();
  const otworz = (z: Zakladka) => {
    app.querySelectorAll<HTMLButtonElement>('[role=tab]').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.z === z)));
    for (const [id, el] of panele) el.hidden = id !== z;
    if (!panele.has(z)) {
      const el = document.createElement('div');
      el.className = `panel panel-${z}`;
      el.setAttribute('role', 'tabpanel');
      tresc.append(el);
      panele.set(z, el);
      ({ mapa: () => panelMapy(el, p), porzadki: () => panelPorzadkow(el), duplikaty: () => panelDuplikatow(el), stare: () => panelStarych(el), typy: () => panelTypow(el, p) })[z]();
    }
    const el = panele.get(z)!;
    el.classList.remove('pokaz');
    void el.offsetWidth;
    el.classList.add('pokaz');
  };
  app.querySelectorAll<HTMLButtonElement>('[role=tab]').forEach((b) => b.addEventListener('click', () => otworz(b.dataset.z as Zakladka)));
  otworz('mapa');
  // Znaleziska licza sie w tle od razu, zeby liczba na zakladce byla gotowa.
  void zaladujPorzadki();
}

// --- mapa: sunburst i lista ---

function panelMapy(el: HTMLElement, p: Podsumowanie) {
  el.innerHTML = `
    <div class="mapa">
      <div class="mapa-lewa">
        <nav class="okruszki" aria-label="Ścieżka"></nav>
        <div class="wykres">
          <canvas aria-label="Wykres zajętości: kliknij fragment, żeby wejść do folderu"></canvas>
          <button class="srodek" aria-label="Wyżej"><span class="s-nazwa"></span><span class="s-rozmiar"></span><span class="s-wyzej">wyżej</span></button>
        </div>
      </div>
      <div class="mapa-prawa">
        <div class="lista-naglowek"><h2 class="l-nazwa"></h2><span class="l-suma slaby"></span></div>
        <ol class="lista-folderow"></ol>
      </div>
    </div>
    <div class="dymek" hidden></div>`;
  const okruszki = el.querySelector<HTMLElement>('.okruszki')!;
  const lista = el.querySelector<HTMLOListElement>('.lista-folderow')!;
  const dymek = el.querySelector<HTMLElement>('.dymek')!;
  const srodek = el.querySelector<HTMLButtonElement>('.srodek')!;
  const ciemny = () => matchMedia('(prefers-color-scheme: dark)').matches;

  const sb = new Sunburst(el.querySelector('canvas')!, p.korzen, {
    pobierzDzieci: (id) => backend.dzieci(id),
    naZmiane: (c) => pokaz(c),
    naNajechanie: (n, x, y) => {
      lista.querySelectorAll('.najechany').forEach((li) => li.classList.remove('najechany'));
      if (!n) return void (dymek.hidden = true);
      lista.querySelector(`[data-id="${n.w.id}"]`)?.classList.add('najechany');
      const r = el.getBoundingClientRect();
      dymek.hidden = false;
      dymek.innerHTML = `<b>${esc(n.w.nazwa)}</b><span>${rozmiar(n.w.na_dysku)}, ${procent(n.w.na_dysku, sb.centrum.w.na_dysku)} z ${esc(sb.centrum.w.nazwa)}</span>${n.w.rodzaj === 'katalog' ? `<span class="slaby">${pliki(n.w.pliki)}</span>` : ''}`;
      dymek.style.transform = `translate(${Math.round(x - r.left + 14)}px, ${Math.round(y - r.top + 14)}px)`;
    },
  });
  srodek.addEventListener('click', () => void sb.oddal());

  function pokaz(c: Wezel) {
    // okruszki
    const droga = sb.sciezka(c);
    okruszki.innerHTML = droga
      .map((n, i) => (i === droga.length - 1 ? `<span aria-current="page">${esc(n.w.nazwa)}</span>` : `<button data-i="${i}">${esc(n.w.nazwa)}</button>`))
      .join('<span class="sep" aria-hidden="true">/</span>');
    okruszki.querySelectorAll<HTMLButtonElement>('button').forEach((b) => b.addEventListener('click', () => void sb.przejdz(droga[Number(b.dataset.i)])));
    okruszki.scrollLeft = okruszki.scrollWidth;
    // srodek
    srodek.querySelector('.s-nazwa')!.textContent = c.w.nazwa;
    srodek.querySelector('.s-rozmiar')!.textContent = rozmiar(c.w.na_dysku);
    srodek.disabled = !c.rodzic;
    // lista
    el.querySelector('.l-nazwa')!.textContent = c.w.nazwa;
    el.querySelector('.l-suma')!.textContent = `${rozmiar(c.w.na_dysku)}, ${pliki(c.w.pliki)}`;
    const dz = c.dzieci ?? [];
    const max = dz[0]?.w.na_dysku || 1;
    const POKAZ = 60;
    lista.innerHTML =
      dz
        .slice(0, POKAZ)
        .map((n) => {
          const katalog = n.w.rodzaj === 'katalog' && n.w.liczba_dzieci > 0;
          const kolor = oklch(ciemny() ? 0.68 : 0.66, katalog ? 0.12 : 0.045, n.odcien);
          return `<li data-id="${n.w.id}" class="${katalog ? 'folder' : 'plik'}" ${katalog ? 'tabindex="0" role="button"' : ''} title="${esc(n.w.nazwa)}">
            <span class="kropka" style="background:${kolor}"></span>
            <span class="f-nazwa">${esc(n.w.nazwa)}${katalog ? '<span class="strz" aria-hidden="true">›</span>' : ''}</span>
            <span class="f-rozmiar">${rozmiar(n.w.na_dysku)}</span>
            <span class="f-pasek"><span style="--u:${(n.w.na_dysku / max).toFixed(4)};background:${kolor}"></span></span>
            <span class="f-opis slaby">${katalog ? pliki(n.w.pliki) : 'plik'}, ${procent(n.w.na_dysku, c.w.na_dysku)}</span>
          </li>`;
        })
        .join('') +
      (dz.length > POKAZ
        ? `<li class="reszta slaby">i ${liczba(dz.length - POKAZ)} ${odmiana(dz.length - POKAZ, 'mniejszy element', 'mniejsze elementy', 'mniejszych elementów')}, razem ${rozmiar(dz.slice(POKAZ).reduce((a, n) => a + n.w.na_dysku, 0))}</li>`
        : '');
    if (!dz.length) lista.innerHTML = '<li class="reszta slaby">Pusty folder.</li>';
    kaskada(lista);
    const poId = new Map(dz.map((n) => [String(n.w.id), n]));
    lista.querySelectorAll<HTMLLIElement>('li[data-id]').forEach((li) => {
      const n = poId.get(li.dataset.id!)!;
      li.addEventListener('mouseenter', () => sb.podswietl(n));
      li.addEventListener('mouseleave', () => sb.podswietl(null));
      if (!li.classList.contains('folder')) return;
      li.addEventListener('click', () => void sb.przejdz(n));
      li.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          void sb.przejdz(n);
        }
      });
    });
  }
}

// --- do sprzatniecia ---

let porzadki: Promise<Znalezisko[]> | null = null;
function zaladujPorzadki() {
  porzadki ??= poKolei(() => backend.porzadki());
  porzadki
    .then((z) => {
      const suma = z.filter((x) => x.poziom === 'bezpieczne').reduce((a, x) => a + x.na_dysku, 0);
      const el = app.querySelector<HTMLElement>('[data-licznik=porzadki]');
      if (el) el.textContent = rozmiar(suma);
    })
    .catch(() => undefined);
  return porzadki;
}

const POZIOMY: [Znalezisko['poziom'], string, string][] = [
  ['bezpieczne', 'Bezpieczne', 'Windows albo program odtworzy to sam.'],
  ['sprawdz', 'Do sprawdzenia', 'Zwykle można, ale coś się traci. Przeczytaj opis.'],
  ['nie-ruszaj', 'Nie ruszaj', 'Tylko wyjaśnienie, czym to jest.'],
];

const SPOSOB: Record<Znalezisko['sposob'], string> = {
  kosz: 'do Kosza',
  trwale: 'usuwane na stałe',
  narzedzie: 'narzędziem Windows',
  brak: 'nie usuwać',
};

async function panelPorzadkow(el: HTMLElement, odzysk?: Odzysk) {
  el.innerHTML = '<div class="ladowanie"><span class="krecek" aria-hidden="true"></span>Sprawdzam znane miejsca Windows…</div>';
  let lista: Znalezisko[];
  try {
    porzadki = null;
    lista = await zaladujPorzadki();
  } catch (e) {
    porzadki = null;
    return blad(el, e, () => panelPorzadkow(el));
  }
  if (!lista.length) {
    el.innerHTML = '<div class="pusto"><p>Nic do sprzątnięcia w znanych miejscach Windows.</p></div>';
    if (odzysk) pokazOdzysk(el, odzysk);
    return;
  }
  const zazn = new Set(lista.filter((z) => z.poziom === 'bezpieczne' && z.rozmiar_znany).map((z) => z.id));
  el.innerHTML = `
    <div class="odzysk">
      <div><p class="etykieta">Do odzyskania</p><p class="odzysk-suma"><span class="liczba-odzysk">0 B</span></p><p class="slaby odzysk-opis"></p></div>
      <div class="odzysk-akcja">
        <button class="glowny wyczysc" aria-describedby="odzysk-uwaga">Wyczyść zaznaczone</button>
        <p class="slaby" id="odzysk-uwaga">Przed usunięciem pokażę listę.</p>
      </div>
    </div>
    <div class="grupy">
      ${POZIOMY.map(([poziom, nazwa, opis]) => {
        const z = lista.filter((x) => x.poziom === poziom);
        if (!z.length) return '';
        const suma = z.reduce((a, x) => a + x.na_dysku, 0);
        return `<section class="grupa grupa-${poziom}">
          <header><h2><span class="znak" aria-hidden="true"></span>${nazwa}</h2><span class="slaby">${opis}</span><span class="g-suma">${rozmiar(suma)}</span></header>
          <ul>${z.map((x) => wierszZnaleziska(x, zazn.has(x.id))).join('')}</ul>
        </section>`;
      }).join('')}
    </div>`;
  el.querySelectorAll('.grupa ul').forEach(kaskada);
  const suma = el.querySelector<HTMLElement>('.liczba-odzysk')!;
  const ustaw = licznik(suma, (n) => rozmiar(n));
  const opis = el.querySelector<HTMLElement>('.odzysk-opis')!;
  const przelicz = () => {
    const wybrane = lista.filter((z) => zazn.has(z.id));
    const bajty = wybrane.reduce((a, z) => a + z.na_dysku, 0);
    ustaw(bajty);
    const admin = wybrane.filter((z) => z.wymaga_admina).length;
    opis.textContent = `${liczba(wybrane.length)} ${odmiana(wybrane.length, 'pozycja', 'pozycje', 'pozycji')} z ${liczba(lista.length)}${admin ? `, ${liczba(admin)} wymaga praw administratora` : ''}`;
    wyczysc.disabled = !wybrane.length;
  };
  const wyczysc = el.querySelector<HTMLButtonElement>('.wyczysc')!;
  wyczysc.addEventListener('click', async () => {
    // Poziom „nie ruszaj” nie ma pola wyboru, a tu odpada drugi raz, gdyby trafil do zbioru.
    const wybrane = lista.filter((z) => zazn.has(z.id) && z.poziom !== 'nie-ruszaj' && z.sposob !== 'brak');
    // Kosz opróżnia SHEmptyRecycleBinW (komenda narzedzie), porzadki_usun go pomija.
    const zNarzedziem = (z: Znalezisko) => z.sposob === 'narzedzie' || z.id === 'kosz';
    const pliki_ = wybrane.filter((z) => !zNarzedziem(z));
    const narzedzia = wybrane.filter(zNarzedziem);
    const tryb = await potwierdz({
      tytul: 'Wyczyścić zaznaczone?',
      pozycje: wybrane.map((z) => ({
        nazwa: z.nazwa,
        bajty: z.na_dysku,
        opis: zNarzedziem(z) ? `narzędzie Windows${z.wymaga_admina ? ', zapyta o prawa administratora' : ''}` : pliki(z.pliki),
      })),
      zTrybem: pliki_.length > 0,
      uwaga: narzedzia.length ? 'Narzędzia Windows usuwają na stałe, bez Kosza.' : undefined,
    });
    if (!tryb) return;
    wyczysc.disabled = true;
    wyczysc.textContent = 'Czyszczę…';
    const o = nowyOdzysk(pliki_.length ? tryb : null);
    try {
      if (pliki_.length) dolacz(o, await poKolei(() => backend.porzadkiUsun(pliki_.map((z) => z.id), tryb)));
      for (const z of narzedzia) {
        const w = await poKolei(() => backend.narzedzie(z.id));
        o.na_dysku += w.odzyskane;
        if (w.ok) o.narzedzia++;
        else {
          o.pominiete.push({ sciezka: z.nazwa, powod: w.opis });
          o.pominiete_razem++;
        }
      }
    } catch (e) {
      o.pominiete.push({ sciezka: 'Luzik', powod: String((e as Error)?.message ?? e) });
      o.pominiete_razem++;
    }
    await panelPorzadkow(el, o);
  });
  el.querySelectorAll<HTMLInputElement>('input[type=checkbox]').forEach((c) =>
    c.addEventListener('change', () => {
      if (c.checked) zazn.add(c.value);
      else zazn.delete(c.value);
      przelicz();
    }),
  );
  el.querySelectorAll<HTMLButtonElement>('.pokaz-miejsca').forEach((b) =>
    b.addEventListener('click', () => {
      const ul = b.nextElementSibling as HTMLElement;
      const otwarte = b.getAttribute('aria-expanded') === 'true';
      b.setAttribute('aria-expanded', String(!otwarte));
      ul.hidden = otwarte;
    }),
  );
  przelicz();
  if (odzysk) pokazOdzysk(el, odzysk);
}

function wierszZnaleziska(z: Znalezisko, zaznaczone: boolean): string {
  const mozna = z.poziom !== 'nie-ruszaj' && z.rozmiar_znany && z.sposob !== 'brak';
  const id = `z-${z.id}`;
  const tagi = [SPOSOB[z.sposob], z.wymaga_admina ? 'prawa administratora' : '', z.miejsca.some((m) => m.bledy) ? 'część bez dostępu' : '']
    .filter(Boolean)
    .map((t) => `<span class="tag">${t}</span>`)
    .join('');
  const miejsca = z.miejsca.slice(0, 40);
  return `<li class="znalezisko">
    ${mozna ? `<input type="checkbox" id="${id}" value="${z.id}" ${zaznaczone ? 'checked' : ''}>` : '<span class="bez-pola" aria-hidden="true"></span>'}
    <div class="zn-tresc">
      <label for="${mozna ? id : ''}" class="zn-nazwa">${esc(z.nazwa)}</label>
      <p class="zn-opis">${esc(z.wyjasnienie)}</p>
      <div class="tagi">${tagi}${z.polecenie ? `<code>${esc(z.polecenie)}</code>` : ''}</div>
      <button class="pokaz-miejsca" aria-expanded="false">${z.miejsca.length === 1 ? 'Pokaż miejsce' : `Pokaż miejsca (${liczba(z.miejsca.length)})`}</button>
      <ul class="miejsca" hidden>${miejsca.map((m) => `<li><span class="sciezka" title="${esc(m.sciezka)}">${esc(m.sciezka)}</span><span>${z.rozmiar_znany ? rozmiar(m.na_dysku) : ''}</span></li>`).join('')}${z.miejsca.length > miejsca.length ? `<li class="slaby">i ${liczba(z.miejsca.length - miejsca.length)} więcej</li>` : ''}</ul>
    </div>
    <div class="zn-rozmiar">${z.rozmiar_znany ? `<b>${rozmiar(z.na_dysku)}</b><span class="slaby">${pliki(z.pliki)}</span>` : '<span class="slaby">rozmiar poda narzędzie Windows</span>'}</div>
  </li>`;
}

// --- duplikaty ---

async function panelDuplikatow(el: HTMLElement) {
  el.innerHTML = '<div class="ladowanie"><span class="krecek" aria-hidden="true"></span>Porównuję zawartość plików większych niż 1 MB… <button class="drugi anuluj-dup">Anuluj</button></div>';
  el.querySelector('.anuluj-dup')!.addEventListener('click', () => void backend.anuluj());
  let w: WynikDuplikatow;
  try {
    w = await poKolei(() => backend.duplikaty());
  } catch (e) {
    return blad(el, e, () => panelDuplikatow(el));
  }
  rysujDuplikaty(el, w);
}

/** Lista grup; po usunieciu rysowana od nowa z tych samych grup bez usunietych plikow (bez czytania dysku). */
function rysujDuplikaty(el: HTMLElement, w: WynikDuplikatow, odzysk?: Odzysk) {
  const licz = app.querySelector<HTMLElement>('[data-licznik=duplikaty]');
  if (licz) licz.textContent = w.grupy.length ? rozmiar(w.do_odzyskania) : '';
  if (!w.grupy.length) {
    el.innerHTML = `<div class="pusto"><p>${w.anulowano ? 'Szukanie przerwane.' : `Brak powtórzonych plików wśród ${liczba(w.kandydaci)} sprawdzonych.`}</p><button class="drugi">Szukaj jeszcze raz</button></div>`;
    el.querySelector('button')!.addEventListener('click', () => panelDuplikatow(el));
    if (odzysk) pokazOdzysk(el, odzysk);
    return;
  }
  el.innerHTML = `
    <div class="odzysk">
      <div><p class="etykieta">Zaznaczone</p><p class="odzysk-suma"><span class="liczba-odzysk">0 B</span></p>
      <p class="slaby">${liczba(w.grupy.length)} ${odmiana(w.grupy.length, 'grupa', 'grupy', 'grup')} takich samych plików, razem do odzyskania ${rozmiar(w.do_odzyskania)}${w.anulowano ? ', szukanie przerwane' : ''}</p></div>
      <div class="odzysk-akcja">
        <button class="glowny usun-zazn" aria-describedby="dup-uwaga">Usuń zaznaczone</button>
        <p class="slaby" id="dup-uwaga"></p>
      </div>
    </div>
    <ul class="dup-grupy">${w.grupy
      .map(
        (g, gi) => `<li class="dup">
        <header><b>${liczba(g.pliki.length)} ${odmiana(g.pliki.length, 'kopia', 'kopie', 'kopii')}</b><span class="slaby">${rozmiar(g.rozmiar)} każda</span><span class="g-suma">${rozmiar(g.do_odzyskania)}</span></header>
        <ul>${g.pliki.map((f, fi) => `<li><label class="wybor"><input type="checkbox" data-g="${gi}" data-f="${fi}" ${fi > 0 ? 'checked' : ''}><span class="sciezka" title="${esc(f.sciezka)}">${esc(f.sciezka)}</span></label><span class="slaby">${data(f.mtime)}</span></li>`).join('')}</ul>
      </li>`,
      )
      .join('')}</ul>`;
  kaskada(el.querySelector('.dup-grupy')!);
  const pola = [...el.querySelectorAll<HTMLInputElement>('.dup input[type=checkbox]')];
  const przycisk = el.querySelector<HTMLButtonElement>('.usun-zazn')!;
  const uwaga = el.querySelector<HTMLElement>('#dup-uwaga')!;
  const ustaw = licznik(el.querySelector<HTMLElement>('.liczba-odzysk')!, rozmiar);
  const wybrane = () => pola.filter((p) => p.checked).map((p) => w.grupy[Number(p.dataset.g)].pliki[Number(p.dataset.f)]);
  const przelicz = () => {
    const wszystkie = w.grupy.findIndex((g, gi) => g.pliki.every((_, fi) => pola.find((p) => p.dataset.g === String(gi) && p.dataset.f === String(fi))!.checked));
    const lista = wybrane();
    ustaw(lista.reduce((a, f) => a + f.na_dysku, 0));
    przycisk.disabled = !lista.length || wszystkie >= 0;
    uwaga.textContent = wszystkie >= 0 ? 'W każdej grupie zostaw co najmniej jedną kopię.' : `${pliki(lista.length)}, pierwsza kopia z grupy zostaje`;
  };
  pola.forEach((p) => p.addEventListener('change', przelicz));
  przelicz();
  przycisk.addEventListener('click', async () => {
    const lista = wybrane();
    const tryb = await potwierdz({
      tytul: 'Usunąć zaznaczone kopie?',
      pozycje: lista.map((f) => ({ nazwa: f.sciezka.split(/[\\/]/).pop()!, sciezka: f.sciezka, bajty: f.na_dysku })),
      zTrybem: true,
    });
    if (!tryb) return;
    przycisk.disabled = true;
    przycisk.textContent = 'Usuwam…';
    const o = nowyOdzysk(tryb);
    try {
      dolacz(o, await poKolei(() => backend.usun(lista.map((f) => f.sciezka), tryb, `Duplikaty: ${liczba(lista.length)}`)));
    } catch (e) {
      o.pominiete.push({ sciezka: 'Luzik', powod: String((e as Error)?.message ?? e) });
      o.pominiete_razem++;
    }
    // Pliki pominiete zostaja na liscie, usuniete znikaja; grupa z jedna kopia przestaje byc grupa.
    const zostaly = new Set(o.pominiete.map((p) => p.sciezka));
    const usuniete = new Set(lista.map((f) => f.sciezka).filter((s) => !zostaly.has(s)));
    const grupy = w.grupy
      .map((g) => {
        const pl = g.pliki.filter((f) => !usuniete.has(f.sciezka));
        return { ...g, pliki: pl, do_odzyskania: pl.slice(1).reduce((a, f) => a + f.na_dysku, 0) };
      })
      .filter((g) => g.pliki.length > 1);
    rysujDuplikaty(el, { ...w, grupy, do_odzyskania: grupy.reduce((a, g) => a + g.do_odzyskania, 0) }, o);
  });
  if (odzysk) pokazOdzysk(el, odzysk);
}

// --- duze i stare ---

async function panelStarych(el: HTMLElement) {
  let minMb = 100;
  let dni = 180;
  let odzysk: Odzysk | null = null;
  el.innerHTML = `
    <div class="filtry">
      <div class="filtr" role="group" aria-label="Najmniejszy rozmiar"><span class="etykieta">Większe niż</span>
        ${[100, 500, 1024].map((m) => `<button class="chip" data-mb="${m}" aria-pressed="${m === minMb}">${m >= 1024 ? '1 GB' : `${m} MB`}</button>`).join('')}</div>
      <div class="filtr" role="group" aria-label="Nieruszane od"><span class="etykieta">Nieruszane od</span>
        ${[90, 180, 365].map((d) => `<button class="chip" data-dni="${d}" aria-pressed="${d === dni}">${wiek(d)}</button>`).join('')}</div>
    </div>
    <div class="stare-wynik"></div>`;
  const wynik = el.querySelector<HTMLElement>('.stare-wynik')!;
  const odswiez = async () => {
    wynik.innerHTML = '<div class="ladowanie"><span class="krecek" aria-hidden="true"></span>Szukam…</div>';
    let lista: StaryPlik[];
    try {
      lista = await backend.starePliki(minMb, dni);
    } catch (e) {
      return blad(wynik, e, odswiez);
    }
    const pokaz = odzysk;
    odzysk = null;
    if (!lista.length) {
      wynik.innerHTML = '<div class="pusto"><p>Brak takich plików. Zmniejsz próg rozmiaru albo wieku.</p></div>';
      if (pokaz) pokazOdzysk(wynik, pokaz);
      return;
    }
    const suma = lista.reduce((a, s) => a + s.na_dysku, 0);
    wynik.innerHTML = `<div class="stare-suma"><p><b>${rozmiar(suma)}</b> <span class="slaby">w ${pliki(lista.length)}</span></p>
        <button class="glowny usun-zazn" disabled>Usuń zaznaczone</button></div>
      <ol class="tabela">${lista
        .map((s, i) => {
          const nazwa = s.sciezka.split(/[\\/]/).pop()!;
          return `<li><input type="checkbox" data-i="${i}" aria-label="Zaznacz ${esc(nazwa)}"><span class="t-nazwa"><b title="${esc(nazwa)}">${esc(nazwa)}</b><span class="sciezka slaby" title="${esc(s.sciezka)}">${esc(s.sciezka.slice(0, s.sciezka.length - nazwa.length))}</span></span>
          <span class="t-wiek slaby">nieruszany ${wiek(s.dni)}</span><span class="t-rozmiar">${rozmiar(s.na_dysku)}</span></li>`;
        })
        .join('')}</ol>`;
    kaskada(wynik.querySelector('.tabela')!);
    const pola = [...wynik.querySelectorAll<HTMLInputElement>('.tabela input')];
    const przycisk = wynik.querySelector<HTMLButtonElement>('.usun-zazn')!;
    const wybrane = () => pola.filter((p) => p.checked).map((p) => lista[Number(p.dataset.i)]);
    pola.forEach((p) =>
      p.addEventListener('change', () => {
        const n = wybrane().length;
        przycisk.disabled = !n;
        przycisk.textContent = n ? `Usuń zaznaczone (${liczba(n)})` : 'Usuń zaznaczone';
      }),
    );
    przycisk.addEventListener('click', async () => {
      const zazn = wybrane();
      const tryb = await potwierdz({
        tytul: 'Usunąć zaznaczone pliki?',
        pozycje: zazn.map((s) => ({ nazwa: s.sciezka.split(/[\\/]/).pop()!, sciezka: s.sciezka, bajty: s.na_dysku, opis: `nieruszany ${wiek(s.dni)}` })),
        zTrybem: true,
      });
      if (!tryb) return;
      przycisk.disabled = true;
      przycisk.textContent = 'Usuwam…';
      const o = nowyOdzysk(tryb);
      try {
        dolacz(o, await poKolei(() => backend.usun(zazn.map((s) => s.sciezka), tryb, `Duże i stare pliki: ${liczba(zazn.length)}`)));
      } catch (e) {
        o.pominiete.push({ sciezka: 'Luzik', powod: String((e as Error)?.message ?? e) });
        o.pominiete_razem++;
      }
      odzysk = o;
      void odswiez();
    });
    if (pokaz) pokazOdzysk(wynik, pokaz);
  };
  el.querySelectorAll<HTMLButtonElement>('.chip').forEach((b) =>
    b.addEventListener('click', () => {
      if (b.dataset.mb) minMb = Number(b.dataset.mb);
      if (b.dataset.dni) dni = Number(b.dataset.dni);
      el.querySelectorAll<HTMLButtonElement>(b.dataset.mb ? '[data-mb]' : '[data-dni]').forEach((c) => c.setAttribute('aria-pressed', String(c === b)));
      void odswiez();
    }),
  );
  void odswiez();
}

// --- typy plikow ---

async function panelTypow(el: HTMLElement, p: Podsumowanie) {
  el.innerHTML = '<div class="ladowanie"><span class="krecek" aria-hidden="true"></span>Liczę rodzaje plików…</div>';
  let lista: WynikKategorii[];
  try {
    lista = await backend.kategorie();
  } catch (e) {
    return blad(el, e, () => panelTypow(el, p));
  }
  const razem = lista.reduce((a, k) => a + k.na_dysku, 0) || p.korzen.na_dysku;
  const max = lista[0]?.na_dysku || 1;
  el.innerHTML = `<ol class="typy">${lista
    .map(
      (k) => `<li class="typ">
      <button class="typ-wiersz" aria-expanded="false">
        <span class="t-nazwa">${esc(k.nazwa)}</span>
        <span class="f-pasek"><span style="--u:${(k.na_dysku / max).toFixed(4)}"></span></span>
        <span class="t-rozmiar">${rozmiar(k.na_dysku)}</span>
        <span class="t-proc slaby">${procent(k.na_dysku, razem)}</span>
        <span class="t-pliki slaby">${pliki(k.pliki)}</span>
      </button>
      <ul class="najwieksze" hidden>${k.najwieksze.map((w) => `<li><span class="sciezka" title="${esc(w.sciezka ?? w.nazwa)}">${esc(w.sciezka ?? w.nazwa)}</span><span>${rozmiar(w.na_dysku)}</span></li>`).join('')}</ul>
    </li>`,
    )
    .join('')}</ol>`;
  kaskada(el.querySelector('.typy')!);
  el.querySelectorAll<HTMLButtonElement>('.typ-wiersz').forEach((b) =>
    b.addEventListener('click', () => {
      const ul = b.nextElementSibling as HTMLElement;
      const otwarte = b.getAttribute('aria-expanded') === 'true';
      b.setAttribute('aria-expanded', String(!otwarte));
      ul.hidden = otwarte;
    }),
  );
}

// --- LUZIK_AUTOSKAN: samosprawdzenie prawdziwego okna ---

async function czekajNa(f: () => number, ms: number): Promise<number> {
  const koniec = performance.now() + ms;
  for (;;) {
    const n = f();
    if (n > 0 || performance.now() > koniec) return n;
    await new Promise((r) => setTimeout(r, 100));
  }
}

/** Okno samo woła prawdziwe komendy (lista_dyskow, skanuj, dzieci, porzadki_skanuj, kategorie),
 *  liczy wiersze listy na mapie i oddaje jedną linię JSON na stdout, po czym aplikacja się zamyka. */
async function autoskan(sciezka: string) {
  const t0 = performance.now();
  try {
    const dyski = await backend.listaDyskow();
    const dysk = dyski.filter((d) => sciezka.startsWith(d.sciezka)).sort((a, b) => b.sciezka.length - a.sciezka.length)[0];
    const cel: Dysk = {
      ...(dysk ?? { etykieta: '', rozmiar: 0, wolne: 0, system_plikow: '', rodzaj: 'staly' }),
      litera: sciezka,
      sciezka,
    };
    ostatniePodsumowanie = null;
    await ekranSkanu(cel);
    const p = ostatniePodsumowanie as Podsumowanie | null;
    if (!p) throw new Error(app.querySelector('.pusto p')?.textContent ?? 'Skan się nie skończył.');
    const wiersze = await czekajNa(() => app.querySelectorAll('.lista-folderow li[data-id]').length, 20_000);
    let porzadkiBlad: string | undefined;
    const znaleziska = await zaladujPorzadki().catch((e) => {
      porzadkiBlad = String((e as Error)?.message ?? e);
      return [] as Znalezisko[];
    });
    const kategorie = await poKolei(() => backend.kategorie());
    await backend.autoskanKoniec({
      sciezka,
      suma_bajtow: p.korzen.rozmiar,
      na_dysku: p.korzen.na_dysku,
      pliki: p.pliki,
      katalogi: p.katalogi,
      wiersze,
      dyski: dyski.length,
      znaleziska: znaleziska.length,
      porzadki_blad: porzadkiBlad,
      kategorie: kategorie.length,
      bledy_razem: p.bledy_razem,
      czas_ms: Math.round(performance.now() - t0),
    });
  } catch (e) {
    await backend.autoskanKoniec({ sciezka, blad: String((e as Error)?.message ?? e) });
  }
}

// --- start ---

app.addEventListener('click', (e) => {
  const b = (e.target as Element).closest<HTMLButtonElement>('[data-akcja]');
  if (b?.dataset.akcja === 'historia') void oknoHistorii();
  if (b?.dataset.akcja === 'admin') {
    backend.uruchomJakoAdmin().catch((er) => komunikat('Uruchom jako administrator', String((er as Error)?.message ?? er)));
  }
});

(async () => {
  backend = await wybierzBackend();
  stanAdmina = await backend.admin().catch(() => stanAdmina);
  const auto = await backend.autoskan().catch(() => null);
  if (auto) return autoskan(auto);
  sledzAktualizacje(backend.udawany);
  await ekranDyskow();
  // Podglad w przegladarce: ?ekran=wynik od razu skanuje pierwszy dysk (zrzuty i testy),
  // ?ekran=porzadki|duplikaty|stare|typy otwiera te zakladke, a &potwierdz=1 otwiera okno usuwania.
  const param = new URLSearchParams(location.search);
  const ekran = param.get('ekran');
  if (backend.udawany && ekran) {
    const dyski = await backend.listaDyskow();
    await ekranSkanu(dyski[0]);
    app.querySelector<HTMLButtonElement>(`[role=tab][data-z="${ekran}"]`)?.click();
    if (param.get('potwierdz')) {
      const przycisk = await czekajNa(() => app.querySelectorAll('.panel:not([hidden]) :is(.wyczysc, .usun-zazn):not(:disabled)').length, 5000);
      if (przycisk) app.querySelector<HTMLButtonElement>('.panel:not([hidden]) :is(.wyczysc, .usun-zazn):not(:disabled)')!.click();
    }
  }
})();
