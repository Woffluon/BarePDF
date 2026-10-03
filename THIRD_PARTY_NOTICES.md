# Third-party notices

This document lists the third-party software components, runtime libraries, Rust crates, and graphical assets distributed with or linked into BarePDF.

---

## PDFium (Google / Foxit / Chromium Project)

BarePDF distributes `pdfium.dll` (built from the Chromium PDFium project via [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)) under the BSD-3-Clause and Apache-2.0 licenses.

### BSD 3-Clause License (Chromium / PDFium)

Copyright 2014 PDFium Authors. All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of Google Inc. nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

### Apache License 2.0 Notice

Portions of PDFium and its bundled dependencies (and the `pdfium-binaries` build distribution scripts, Copyright Benoit Blanchon) are licensed under the Apache License, Version 2.0 (the "License"); you may not use these components except in compliance with the License. You may obtain a copy of the License at:

https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.

---

## Slint UI Framework

BarePDF uses the [Slint](https://slint.dev/) GUI toolkit (`slint` crate, Copyright SixtyFPS GmbH) under the **Royalty-free Desktop, Mobile, and Web Applications License** / **GNU GPLv3** / **Ambassador Program** terms as published by SixtyFPS GmbH, and displays the unmodified `MadeWithSlint` attribution badge on the project website.

### Slint Attribution Badge

`website/public/made-with-slint.svg` is the unmodified MadeWithSlint logo from
[Slint's brand assets](https://slint.dev/brand-guidelines), licensed under
[CC BY-ND 4.0](https://creativecommons.org/licenses/by-nd/4.0/).

---

## Rust Crates (Direct Dependencies)

BarePDF links the following open-source Rust crates from [crates.io](https://crates.io):

| Crate | License | Repository / Copyright |
| :--- | :--- | :--- |
| `arboard` | MIT OR Apache-2.0 | Copyright (c) 1Password / Artur Kovacs and contributors (`https://github.com/1Password/arboard`) |
| `crossbeam-channel` | MIT OR Apache-2.0 | Copyright (c) The Crossbeam Project Developers (`https://github.com/crossbeam-rs/crossbeam`) |
| `ed25519-dalek` | BSD-3-Clause | Copyright (c) 2017-2023 Isis Agora Lovecruft, Henry de Valence (`https://github.com/dalek-cryptography/curve25519-dalek`) |
| `lru` | MIT | Copyright (c) Jerome Froelich (`https://github.com/jeromefroe/lru-rs`) |
| `pdfium-render` | MIT OR Apache-2.0 | Copyright (c) Alastairov (`https://github.com/ajrcarey/pdfium-render`) |
| `raw-window-handle` | MIT OR Apache-2.0 OR Zlib | Copyright (c) The Rust Windowing contributors (`https://github.com/rust-windowing/raw-window-handle`) |
| `rfd` | MIT | Copyright (c) PolyMeilex (`https://github.com/PolyMeilex/rfd`) |
| `semver` | MIT OR Apache-2.0 | Copyright (c) David Tolnay (`https://github.com/dtolnay/semver`) |
| `serde` / `serde_json` | MIT OR Apache-2.0 | Copyright (c) Erick Tryzelaar, David Tolnay (`https://github.com/serde-rs/serde`) |
| `sha2` | MIT OR Apache-2.0 | Copyright (c) RustCrypto Developers (`https://github.com/RustCrypto/hashes`) |
| `slint` | GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0 | Copyright (c) SixtyFPS GmbH (`https://github.com/slint-ui/slint`) |
| `tempfile` | MIT OR Apache-2.0 | Copyright (c) Steven Allen (`https://github.com/Stebalien/tempfile`) |
| `thiserror` | MIT OR Apache-2.0 | Copyright (c) David Tolnay (`https://github.com/dtolnay/thiserror`) |
| `tracing` / `tracing-subscriber` | MIT | Copyright (c) Tokio Contributors (`https://github.com/tokio-rs/tracing`) |
| `ureq` | MIT OR Apache-2.0 | Copyright (c) Martin Algesten and contributors (`https://github.com/algesten/ureq`) |
| `windows` / `windows-core` / `windows-sys` | MIT OR Apache-2.0 | Copyright (c) Microsoft Corporation (`https://github.com/microsoft/windows-rs`) |
| `winres` | MIT | Copyright (c) Maxenceermann (`https://github.com/mxre/winres`) |
| `zeroize` | Apache-2.0 OR MIT | Copyright (c) The Rust Project Developers / Tony Arcieri (`https://github.com/RustCrypto/utils`) |

Transitive Rust dependencies (including `rustls`, `webpki-roots`, `winit`, `femtovg`, `ab_glyph`, `accesskit`, `curve25519-dalek`, and `ring`/`aws-lc-rs` where applicable) are licensed under permissive open-source licenses (`MIT`, `Apache-2.0`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, or `MPL-2.0`) as recorded in `Cargo.lock`.

---

## Microsoft Fluent UI System Icons

The icons under `assets/icons/` are from Microsoft Fluent UI System Icons,
package version 1.1.335.

MIT License

Copyright (c) 2020 Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
