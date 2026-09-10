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

Die offizielle Lite-Ausgabe unterstützt **einen aktiven virtuellen Bildschirm**.
Der Hintergrunddienst erzwingt das Limit; ein direkter IPC-Aufruf oder eine alte
Konfiguration kann keinen zweiten Bildschirm aktivieren. Überschüssige
Identitäten werden deaktiviert, ohne das Backup zu zerstören.

## Bauen

Benötigt werden Windows 10/11 x64, stabiles Rust mit MSVC, Visual Studio C++
Build Tools und ein aktuelles Windows SDK.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Signaturschlüssel, Zertifikate, `devcon.exe` und der signierte Treiber werden
nicht im Repository veröffentlicht. Der unterstützte Treiber ist im offiziellen
Build auf der [EvertyDisplay-Seite](https://desk.everty.ru/evertydisplay) enthalten.

## Starkes Copyleft

Der Code steht ausschließlich unter **GNU GPLv3**. Wer eine geänderte oder
abgeleitete Version verteilt, muss den Empfängern den zugehörigen Quellcode und
dieselben Freiheiten bereitstellen. Maßgeblich ist [LICENSE](../LICENSE); diese
Zusammenfassung ist keine Rechtsberatung.

Autor: **Arthur Valiev (Артур Валиев)** · [info@everty.ru](mailto:info@everty.ru)
