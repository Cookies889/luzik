// Lista zmian z latest.json (pole notes) zamieniona na punkty dla czlowieka.
// Notes pisze workflow wydanie z ZMIANY.md, ale starsze wydania braly tresc commita,
// wiec tu i tak odpada wszystko, co jest dla programisty: stopki gita (Co-Authored-By,
// Signed-off-by), naglowki markdown i linie z sama wersja.

const STOPKA_GITA = /^[A-Za-z][A-Za-z-]*-by:\s|^(Change-Id|Refs|Closes|Fixes):\s/i;
const NAGLOWEK = /^#{1,6}\s/;
const SAMA_WERSJA = /^(Luzik\s+)?v?\d+\.\d+\.\d+\s*:?\s*$/i;

export const MAKS_PUNKTOW = 6;

export function punktyZmian(tekst: string): string[] {
  return tekst
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !STOPKA_GITA.test(l) && !NAGLOWEK.test(l))
    .map((l) => l.replace(/^[-*]\s*/, '').trim())
    .filter((l) => l && !SAMA_WERSJA.test(l))
    .slice(0, MAKS_PUNKTOW);
}
