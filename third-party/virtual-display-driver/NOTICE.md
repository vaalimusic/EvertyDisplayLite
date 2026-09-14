# Virtual Display Driver notice

EvertyDisplay Lite installer builds include the signed MttVDD component from
[Virtual Display Driver](https://github.com/VirtualDrivers/Virtual-Display-Driver),
release `24.12.24`.

Upstream authors include the Virtual Display Driver Development Team and
MikeTheTech. The component is distributed under the MIT License reproduced in
this directory. The driver signature remains attributable to its signer and
does not imply endorsement of EvertyDisplay.

The binary payload is not stored in this repository. The installer build script
downloads the official upstream release, verifies its pinned SHA-256 and checks
the embedded Authenticode signatures before packaging it.

EvertyDisplay application author: **Артур Валиев (Arthur Valiev)**

https://desk.everty.ru/evertydisplay
