<div align="center">
  <img src="assets/logo_mark.svg" width="108" alt="EvertyDisplay Lite logo">
  <h1>EvertyDisplay Lite</h1>
  <p><strong>One more screen. No more hardware.</strong></p>
  <p>A native-feeling virtual display for Windows, built in Rust.</p>

  <p>
    <a href="https://desk.everty.ru/evertydisplay">Website</a> ·
    <a href="#build-from-source">Build</a> ·
    <a href="SECURITY.md">Security</a> ·
    <a href="LICENSE">GPLv3</a>
  </p>

  <p>
    <a href="README.md">English</a> ·
    <a href="docs/README.ru.md">Русский</a> ·
    <a href="docs/README.ar.md">العربية</a> ·
    <a href="docs/README.es.md">Español</a> ·
    <a href="docs/README.de.md">Deutsch</a> ·
    <a href="docs/README.fr.md">Français</a>
  </p>

  <p>
    <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows">
    <img alt="Rust" src="https://img.shields.io/badge/Rust-stable-CE412B?style=flat-square&logo=rust">
    <img alt="License GPL-3.0-only" src="https://img.shields.io/badge/license-GPL--3.0--only-7B3F00?style=flat-square">
    <img alt="Lite: one virtual display" src="https://img.shields.io/badge/Lite-1%20virtual%20display-6C5CE7?style=flat-square">
  </p>
</div>

---

EvertyDisplay Lite turns one Windows desktop into a spatial workspace with an
additional virtual display. Move the pointer and windows across the boundary,
preview the virtual screen through Live PiP, and arrange the topology like real
monitors in Windows Settings.

It is not a remote desktop and not a collection of floating windows. The virtual
output participates in the Windows display topology, while EvertyDisplay adds
the orchestration that makes it comfortable to use.

## What it feels like

```text
┌──────────────────────── MAIN ────────────────────────┐
│  browser · editor · work                             │
└───────────────────────────┬──────────────────────────┘
                            │ native spatial boundary
┌───────────────────────────┴──────────────────────────┐
│  VIRTUAL · video · reference · secondary workspace  │
└──────────────────────────────────────────────────────┘
```

- Native Windows topology and cursor transitions
- Drag-to-Teleport for moving windows across display boundaries
- Live PiP with persistent per-display size and position
- Fullscreen Viewport powered by Direct3D 11 / Desktop Duplication
- OSD with an optional compact display map
- Gaming guard, hotkeys and focus-aware window activation
- Russian, English, Arabic, Spanish, German and French UI
- Recovery logic for driver restarts, interrupted additions and stale configs

## The Lite promise

The official Lite edition supports **one active virtual display**. This is not
just a disabled button:

1. the background service is the authority;
2. direct IPC requests for another display are rejected;
3. a saved Pro topology is clamped during startup;
4. surplus identities are disabled, not destroyed;
5. multi-display grid commands are rejected by both UI and service.

GPL gives every recipient the freedom to inspect and modify the source. The
one-display rule therefore defines the official Lite build; it is deliberately
not described as DRM.

## Architecture

```text
Iced UI ──named pipe──▶ Service ──▶ Product policy (Lite = 1)
                           ├──────▶ Windows topology / cursor / windows
                           ├──────▶ IddCx driver manager
                           └──────▶ D3D11 Viewport + Live PiP
```

| Crate | Responsibility |
|---|---|
| `ipc` | Versioned messages, topology and capability contract |
| `driver-manager` | Windows display enumeration and VDD control |
| `renderer` | D3D11 capture, Viewport, PiP and cursor mapping |
| `product-policy` | Authoritative Lite capability boundary |
| `service` | Recovery, topology, input, focus, OSD and tray |
| `ui` | Iced settings app and localization |

## Build from source

You need Windows 10/11 x64, stable Rust with the MSVC toolchain, Visual Studio
C++ Build Tools and a recent Windows SDK.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Outputs:

```text
target/release/EvertyDisplay.exe
target/release/multitor-service.exe
```

### About the virtual display driver

This repository contains the Rust-side driver integration but intentionally
does not publish signing keys, certificates, `devcon.exe`, archives or the
signed driver payload used in official releases. Install an official build from
the [EvertyDisplay page](https://desk.everty.ru/evertydisplay) to use the
supported signed driver.

The driver payload will enter the repository only after its complete source
provenance and redistribution terms have been audited. A convenient installer
is never worth quietly publishing somebody else's binary or a signing secret.

## Strong copyleft, by design

EvertyDisplay Lite is licensed under **GNU GPL version 3 only**. In practical
terms, if you distribute a modified or derivative build, recipients must receive
the corresponding source and the same GPLv3 freedoms. You may use the program
privately without publishing private modifications merely because you ran it.

The full legal text in [LICENSE](LICENSE) is authoritative. This summary is not
legal advice. EvertyDisplay Pro and related hosted services may be offered
separately under commercial terms by the copyright holder.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting code. Security issues
involving elevation, driver installation, named pipes or process execution must
be reported privately according to [SECURITY.md](SECURITY.md).

---

<div align="center">
  <strong>Designed and created by Arthur Valiev</strong><br>
  <a href="https://desk.everty.ru/evertydisplay">desk.everty.ru/evertydisplay</a> ·
  <a href="mailto:info@everty.ru">info@everty.ru</a>
</div>
