<div align="center">
  <img src="../assets/logo_mark.svg" width="96" alt="Logotipo de EvertyDisplay Lite">
  <h1>EvertyDisplay Lite</h1>
  <p><strong>Una pantalla más. Sin más hardware.</strong></p>
  <p><a href="../README.md">English</a> · <a href="README.ru.md">Русский</a> · <a href="README.ar.md">العربية</a> · Español · <a href="README.de.md">Deutsch</a> · <a href="README.fr.md">Français</a></p>
</div>

EvertyDisplay Lite crea una pantalla virtual adicional en Windows que se siente
como un monitor físico: mueve el puntero y las ventanas a través de sus bordes y
colócala espacialmente junto a tus otras pantallas.

## Funciones

- topología espacial nativa de Windows;
- traslado de ventanas con Drag-to-Teleport;
- Live PiP que conserva tamaño y posición;
- Viewport a pantalla completa mediante Direct3D 11;
- OSD, atajos, modo de juego y recuperación ante fallos;
- interfaz disponible en seis idiomas.

La versión oficial Lite **no impone límites artificiales de producto o licencia**
a las pantallas virtuales y habilita las disposiciones multipantalla. El
controlador MttVDD firmado admite técnicamente hasta **cinco pantallas virtuales
activas**; EvertyDisplay expone toda esa capacidad y bloquea un sexto intento
con un estado claro en la interfaz.

## Compilación

Requiere Windows 10/11 x64, Rust estable con MSVC, Visual Studio C++ Build Tools
y un Windows SDK reciente.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

El workflow del instalador descarga MttVDD 24.12.24 desde su versión oficial,
verifica el SHA-256 fijado y las firmas Authenticode, y solo entonces lo empaqueta.
Las claves de firma nunca se guardan en el repositorio. El instalador local se
genera con `./scripts/build-installer.ps1` e Inno Setup 6.

## Copyleft fuerte

El código se distribuye exclusivamente bajo **GNU GPLv3**. Si distribuyes una
versión modificada o derivada, sus destinatarios deben recibir el código fuente
correspondiente y las mismas libertades. El texto de [LICENSE](../LICENSE) es el
que prevalece; este resumen no constituye asesoramiento legal.

Autor: **Arthur Valiev (Артур Валиев)** · [info@everty.ru](mailto:info@everty.ru)
