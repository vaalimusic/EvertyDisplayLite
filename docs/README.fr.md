<div align="center">
  <img src="../assets/logo_mark.svg" width="96" alt="Logo EvertyDisplay Lite">
  <h1>EvertyDisplay Lite</h1>
  <p><strong>Un écran de plus. Sans matériel supplémentaire.</strong></p>
  <p><a href="../README.md">English</a> · <a href="README.ru.md">Русский</a> · <a href="README.ar.md">العربية</a> · <a href="README.es.md">Español</a> · <a href="README.de.md">Deutsch</a> · Français</p>
</div>

EvertyDisplay Lite crée un écran virtuel Windows qui se comporte comme un
moniteur physique : le pointeur et les fenêtres franchissent ses bords, tandis
que sa position suit la topologie spatiale configurée dans Windows.

## Fonctionnalités

- topologie d’affichage native de Windows ;
- déplacement des fenêtres avec Drag-to-Teleport ;
- Live PiP mémorisant sa taille et sa position ;
- Viewport plein écran propulsé par Direct3D 11 ;
- OSD, raccourcis, mode jeu et récupération fiable ;
- interface disponible en six langues.

L’édition Lite officielle **n’impose aucune limite artificielle de produit ou de
licence** aux écrans virtuels et active les dispositions multi-écrans. Le pilote
MttVDD signé prend techniquement en charge jusqu’à **cinq écrans virtuels actifs** ;
EvertyDisplay expose toute cette capacité et bloque proprement une sixième
tentative dans l’interface.

## Compilation

Il faut Windows 10/11 x64, Rust stable avec MSVC, Visual Studio C++ Build Tools
et un SDK Windows récent.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Le workflow d’installation télécharge MttVDD 24.12.24 depuis sa version amont
officielle, vérifie le SHA-256 épinglé et les signatures Authenticode, puis le
conditionne. Aucune clé de signature n’est stockée dans le dépôt. L’installeur
local se construit avec `./scripts/build-installer.ps1` et Inno Setup 6.

## Copyleft fort

Le code est distribué exclusivement sous **GNU GPLv3**. Toute distribution d’une
version modifiée ou dérivée doit fournir aux destinataires le code source
correspondant et les mêmes libertés. Seul [LICENSE](../LICENSE) fait foi ; ce
résumé ne constitue pas un conseil juridique.

Auteur : **Arthur Valiev (Артур Валиев)** · [info@everty.ru](mailto:info@everty.ru)
