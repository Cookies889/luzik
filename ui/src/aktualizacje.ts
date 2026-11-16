// Aktualizacje z GitHub Releases: przy starcie i co 6 h aplikacja pyta latest.json
// (adres i klucz publiczny w tauri.conf.json, plugins.updater). Gdy jest nowsza wersja,
// w prawym dolnym rogu pojawia sie pasek z lista zmian i przyciskiem Zaktualizuj:
// pobieranie z postepem, instalacja, restart.
// W zwyklej przegladarce ?aktualizacja=1 pokazuje pasek z przykladowa wersja (podglad).

import { esc, rozmiar } from './format';

const CO_ILE_MS = 6 * 60 * 60 * 1000;

interface Nowa {
  wersja: string;
  zmiany: string;
  pobierz: (postep: (pobrane: number, razem: number | null) => void) => Promise<void>;
  restart: () => Promise<void>;
}

let pasek: HTMLElement | null = null;
let pokazana = '';

async function sprawdzTauri(): Promise<Nowa | null> {
  const { check } = await import('@tauri-apps/plugin-updater');
  const { relaunch } = await import('@tauri-apps/plugin-process');
  const u = await check();
  if (!u) return null;
  return {
    wersja: u.version,
    zmiany: u.body ?? '',
    pobierz: async (postep) => {
      let pobrane = 0;
      let razem: number | null = null;
      await u.downloadAndInstall((z) => {
        if (z.event === 'Started') razem = z.data.contentLength ?? null;
        else if (z.event === 'Progress') pobrane += z.data.chunkLength;
        postep(pobrane, razem);
      });
    },
    restart: relaunch,
  };
}

function udawana(): Nowa {
  return {
    wersja: '0.1.1',
    zmiany: '- Szybszy skan dysku C\n- Poprawka okna potwierdzenia',
    pobierz: async (postep) => {
      const razem = 4_800_000;
      for (let p = 0; p <= razem; p += 600_000) {
        postep(p, razem);
        await new Promise((r) => setTimeout(r, 120));
      }
    },
    restart: async () => location.reload(),
  };
}

function listaZmian(tekst: string): string {
  const linie = tekst.split('\n').map((l) => l.replace(/^\s*[-*]\s*/, '').trim()).filter(Boolean).slice(0, 6);
  if (!linie.length) return '';
  return `<ul class="akt-zmiany">${linie.map((l) => `<li>${esc(l)}</li>`).join('')}</ul>`;
}

function pokaz(n: Nowa) {
  if (pokazana === n.wersja && pasek) return;
  pokazana = n.wersja;
  pasek?.remove();
  pasek = document.createElement('section');
  pasek.className = 'aktualizacja';
  pasek.setAttribute('aria-label', 'Nowa wersja');
  pasek.innerHTML = `
    <div class="akt-gora">
      <p class="akt-tytul">Nowa wersja ${esc(n.wersja)}</p>
      <button class="akt-zamknij" aria-label="Później" title="Później">×</button>
    </div>
    ${listaZmian(n.zmiany)}
    <div class="akt-dol">
      <button class="glowny akt-zaktualizuj">Zaktualizuj</button>
      <p class="akt-stan slaby" role="status"></p>
    </div>
    <div class="pasek akt-postep" hidden><span style="--u:0"></span></div>`;
  document.body.append(pasek);
  const el = pasek;
  const przycisk = el.querySelector<HTMLButtonElement>('.akt-zaktualizuj')!;
  const stan = el.querySelector<HTMLElement>('.akt-stan')!;
  const postep = el.querySelector<HTMLElement>('.akt-postep')!;
  el.querySelector('.akt-zamknij')!.addEventListener('click', () => {
    el.remove();
    if (pasek === el) pasek = null;
  });
  przycisk.addEventListener('click', async () => {
    przycisk.disabled = true;
    postep.hidden = false;
    stan.textContent = 'Pobieranie…';
    try {
      await n.pobierz((pobrane, razem) => {
        if (razem) {
          postep.querySelector('span')!.style.setProperty('--u', String(Math.min(1, pobrane / razem)));
          stan.textContent = `${rozmiar(pobrane)} z ${rozmiar(razem)}`;
        } else {
          stan.textContent = rozmiar(pobrane);
        }
      });
      stan.textContent = 'Uruchamiam ponownie…';
      await n.restart();
    } catch (e) {
      przycisk.disabled = false;
      postep.hidden = true;
      stan.textContent = `Nie udało się: ${e instanceof Error ? e.message : String(e)}`;
    }
  });
}

async function sprawdz(tauri: boolean) {
  try {
    const n = tauri ? await sprawdzTauri() : udawana();
    if (n) pokaz(n);
  } catch (e) {
    // Brak sieci albo GitHub nie odpowiada: sprobujemy przy nastepnym sprawdzeniu.
    console.warn('Luzik: sprawdzanie aktualizacji nie wyszło', e);
  }
}

export function sledzAktualizacje(udawany: boolean) {
  if (udawany) {
    if (new URLSearchParams(location.search).get('aktualizacja')) void sprawdz(false);
    return;
  }
  void sprawdz(true);
  setInterval(() => void sprawdz(true), CO_ILE_MS);
}
