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

L’édition Lite officielle prend en charge **un écran virtuel actif**. La limite
est imposée par le service d’arrière-plan : une requête IPC directe ou une
ancienne configuration ne peut pas activer un second écran. Les identités en
surplus sont désactivées sans détruire la sauvegarde.

## Compilation

Il faut Windows 10/11 x64, Rust stable avec MSVC, Visual Studio C++ Build Tools
et un SDK Windows récent.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Le dépôt ne publie ni clés de signature, ni certificats, ni `devcon.exe`, ni la
charge signée du pilote. Le pilote pris en charge est fourni dans la version
officielle sur la [page EvertyDisplay](https://desk.everty.ru/evertydisplay).

## Copyleft fort

Le code est distribué exclusivement sous **GNU GPLv3**. Toute distribution d’une
version modifiée ou dérivée doit fournir aux destinataires le code source
correspondant et les mêmes libertés. Seul [LICENSE](../LICENSE) fait foi ; ce
résumé ne constitue pas un conseil juridique.

Auteur : **Arthur Valiev (Артур Валиев)** · [info@everty.ru](mailto:info@everty.ru)
