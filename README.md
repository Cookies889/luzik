# Luzik

Pokazuje, co zajmuje dysk na Windows 11 i co można bezpiecznie usunąć.

## Instalacja

1. Wejdź w [Releases](https://github.com/Cookies889/luzik/releases/latest) i pobierz plik `Luzik_..._x64-setup.exe`.
2. Uruchom go. Windows pokaże niebieskie okno SmartScreen „System Windows ochronił ten komputer”,
   bo instalator nie jest podpisany płatnym certyfikatem. Kliknij **Więcej informacji**, potem **Uruchom mimo to**.
3. Instalator jest po polsku i nie wymaga hasła administratora.

## Aktualizacje

Luzik sam sprawdza nowe wersje przy starcie i co 6 godzin. Gdy jest nowa, w prawym dolnym rogu
pojawia się pasek z listą zmian i przyciskiem **Zaktualizuj**: aplikacja pobierze wersję, zainstaluje
ją i uruchomi się ponownie. Paczki aktualizacji są podpisane, więc Luzik nie zainstaluje cudzego pliku.

## Dla autora

- Nowa wersja: podbij `version` w `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` i `package.json`,
  potem utwórz tag `vX.Y.Z` i wyślij go na GitHuba. Workflow `wydanie` zbuduje instalator i `latest.json`.
- Klucz prywatny podpisu leży poza repo (`~/.config/luzik-klucz/luzik.key`) i w sekrecie
  `TAURI_SIGNING_PRIVATE_KEY`. Bez niego nie da się wydać aktualizacji, którą przyjmą zainstalowane kopie.
