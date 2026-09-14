<div align="center">
  <img src="../assets/logo_mark.svg" width="96" alt="EvertyDisplay Lite Logo">
  <h1>EvertyDisplay Lite</h1>
  <p><strong>Ein Bildschirm mehr. Ohne zusätzliche Hardware.</strong></p>
  <p><a href="../README.md">English</a> · <a href="README.ru.md">Русский</a> · <a href="README.ar.md">العربية</a> · <a href="README.es.md">Español</a> · Deutsch · <a href="README.fr.md">Français</a></p>
</div>

EvertyDisplay Lite erstellt einen zusätzlichen virtuellen Windows-Bildschirm,
der sich wie ein physischer Monitor anfühlt. Mauszeiger und Fenster bewegen sich
über seine Grenzen, und die Anordnung folgt der räumlichen Windows-Topologie.

## Funktionen

- native Windows-Bildschirmtopologie;
- Drag-to-Teleport für Fenster;
- Live PiP mit gespeicherter Größe und Position;
- Vollbild-Viewport über Direct3D 11;
- OSD, Tastenkürzel, Spielmodus und zuverlässige Wiederherstellung;
- Benutzeroberfläche in sechs Sprachen.

Die offizielle Lite-Ausgabe hat **keine künstliche Produkt- oder Lizenzgrenze**
für virtuelle Bildschirme. Mehrbildschirm-Layouts sind vollständig aktiviert.
Der aktuelle signierte MttVDD-Treiber unterstützt technisch bis zu **fünf aktive
virtuelle Bildschirme**; EvertyDisplay stellt den gesamten Bereich bereit und
verhindert einen sechsten Versuch mit einem klaren UI-Status.

## Bauen

Benötigt werden Windows 10/11 x64, stabiles Rust mit MSVC, Visual Studio C++
Build Tools und ein aktuelles Windows SDK.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Der Installer-Workflow lädt MttVDD 24.12.24 aus dem offiziellen Upstream-Release,
prüft den festgelegten SHA-256 sowie die Authenticode-Signaturen und verpackt ihn
erst danach. Signaturschlüssel werden niemals im Repository gespeichert. Lokal
wird der Installer mit `./scripts/build-installer.ps1` und Inno Setup 6 gebaut.

## Starkes Copyleft

Der Code steht ausschließlich unter **GNU GPLv3**. Wer eine geänderte oder
abgeleitete Version verteilt, muss den Empfängern den zugehörigen Quellcode und
dieselben Freiheiten bereitstellen. Maßgeblich ist [LICENSE](../LICENSE); diese
Zusammenfassung ist keine Rechtsberatung.

Autor: **Arthur Valiev (Артур Валиев)** · [info@everty.ru](mailto:info@everty.ru)
