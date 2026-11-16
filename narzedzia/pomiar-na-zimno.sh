#!/usr/bin/env bash
# Pomiar na zimno: luzik-cli kontra du -s na tym samym katalogu, przed kazdym biegiem
# pamiec podreczna systemu plikow jest czyszczona (to wymaga sudo, stad osobny skrypt).
#   ./narzedzia/pomiar-na-zimno.sh [katalog]     domyslnie /usr
set -euo pipefail
cel="${1:-/usr}"
tu="$(cd "$(dirname "$0")/../src-tauri" && pwd)"
(cd "$tu" && cargo build --release --bin luzik-cli >/dev/null 2>&1)
cli="$(cd "$tu" && cargo metadata --format-version 1 --no-deps | sed -E 's/.*"target_directory":"([^"]+)".*/\1/')/release/luzik-cli"

na_zimno() { sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null; }
czas() { local t0 t1; t0=$(date +%s%N); "$@" >/dev/null 2>&1 || true; t1=$(date +%s%N); echo $(( (t1 - t0) / 1000000 )); }

sudo -v
na_zimno; du_ms=$(czas du -s "$cel")
na_zimno; luzik_ms=$(czas "$cli" skanuj "$cel")
echo "du -s $cel na zimno:      ${du_ms} ms"
echo "luzik-cli $cel na zimno:  ${luzik_ms} ms"
