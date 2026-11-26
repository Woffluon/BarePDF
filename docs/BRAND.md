# BarePDF — Brand & Logo Guidelines

**Tagline (EN):** The document. Nothing else.  
**Tagline (TR):** Yalnızca belge. Fazlası yok.

---

## 1. The Logo

- **Idea:** Concept A (*Mimari Kesit* / Architectural Cutaway) — a 45°-chamfered `192 × 216` document monolith opened by a 28-unit 45°-terminated cutaway channel and an inner `68 × 92` local page aperture on a `256 × 256` coordinate grid.
- **Construction:** Pure vector `<path>` using `fill-rule="evenodd"` so every cutaway and inner window is a true hole (no white mask overlays, no `<text>`, no strokes, no filters, no gradients).
- **Versions:**
  - **Symbol only:** `assets/logo.svg`, `packaging/linux/icons/barepdf.svg`, `website/public/favicon.svg`, `website/public/images/logo.svg` (`256 × 256`, `#f69423`).
  - **Horizontal lockup:** `931 × 256` vector composition combining the `256 × 256` symbol with outlined geometric `<path>` letterforms for `BarePDF` (used inside README/social banners).
  - **App tile & web icons:** Symbol centered on a `#16171a` rounded tile (`icon-dark.png`, `app.ico`, `favicon.ico`) or `#fcfcfd` light tile (`icon-white.png`).

---

## 2. Clear Space

Maintain a clear zone of at least **1 × `u`** around all sides of the symbol or lockup, where **`u = 32` units on the `256 × 256` grid** (`1/6` of the symbol width, equal to the structural wall thickness of the mark). Never apply a fixed pixel distance; scale the clear zone proportionally with the rendered mark.

---

## 3. Minimum Size

| Version | Screen | Print | Notes |
|---|---|---|---|
| Horizontal lockup | `96 px` wide | `25 mm` wide | Drop tagline below `180 px` wide |
| Symbol only (master SVG) | `16 px` | `6 mm` | Keep `fill-rule="evenodd"` crisp |
| Small-size `.ico` / favicon (`16 px`, `24 px`) | `16 px`, `24 px` | — | Use pixel-grid-snapped widened-gap cut on `#16171a` tile |

---

## 4. Colour

Desktop UI tokens (`crates/barepdf-ui/ui/tokens.slint`) and website CSS tokens (`website/src/styles/tokens.css`) are authoritative and preserved across surfaces.

### Brand Accents

| Name | HEX | RGB | CMYK (approx.) | Pantone (approx.) | Use |
|---|---|---|---|---|---|
| Desktop Amber | `#f69423` | `246, 148, 35` | `0, 48, 92, 0` | `144 C` | Master SVG symbol, dark icons, desktop active UI |
| Desktop Accent Hover | `#faad52` | `250, 173, 82` | `0, 38, 76, 0` | `1365 C` | Desktop hover state |
| Desktop Accent Pressed | `#df7d13` | `223, 125, 19` | `4, 58, 98, 0` | `151 C` | Desktop pressed state |
| Web Light Amber | `#e5730a` | `229, 115, 10` | `4, 62, 100, 0` | `158 C` | Light web accent, `icon-white.png`, `banner-white.png` (`≥ 3:1` on `#fcfcfd`) |
| Web Dark Amber | `#f2861c` | `242, 134, 28` | `0, 54, 95, 0` | `1505 C` | Dark web accent, manifesto rules |

### Neutral Surfaces & Ink

| Name | HEX | RGB | CMYK (approx.) | Use |
|---|---|---|---|---|
| Web Canvas Light | `#fcfcfd` | `252, 252, 253` | `1, 1, 0, 0` | Light site background, light banner, welcome PDF |
| Web Surface Light | `#ffffff` | `255, 255, 255` | `0, 0, 0, 0` | Light cards and elevated containers |
| Web Text Light | `#121316` | `18, 19, 22` | `78, 70, 64, 83` | Primary light-mode typography, light wordmark |
| Web Canvas Dark | `#0f1012` | `15, 16, 18` | `78, 71, 66, 86` | Dark site background, OG image, manifesto cover |
| Web Surface Dark | `#16171a` | `22, 23, 26` | `76, 69, 63, 81` | Dark app-icon tile, dark cards |
| Web Text Dark | `#f3f4f6` | `243, 244, 246` | `4, 3, 2, 0` | Primary dark-mode typography, dark wordmark |
| Desktop Window Light | `#f3f3f3` | `243, 243, 243` | `4, 3, 3, 0` | Desktop window chrome (light) |
| Desktop Window Dark | `#202020` | `32, 32, 32` | `71, 65, 64, 75` | Desktop title bar / chrome (dark) |
| Desktop Canvas Dark | `#181818` | `24, 24, 24` | `73, 67, 66, 80` | Desktop reading viewport (dark) |

*(Pantone equivalents are approximations; check against a physical swatch book before offset printing.)*

### Approved Colour Pairings

- `#f69423` symbol on `#16171a`, `#0f1012`, `#202020`, or `#181818` (7.3:1 – 7.8:1 contrast).
- `#e5730a` symbol on `#fcfcfd` or `#ffffff` (3.3:1 graphic contrast).
- `#f69423` symbol on `#f3f3f3` / `#fcfcfd` in 18–38 px UI/browser contexts (`title_bar.slint`, `Header.astro`, `favicon.svg`).
- Monochrome `#121316` on light grounds; monochrome `#f3f4f6` on dark grounds.

---

## 5. Typography

- **Primary UI & Editorial Sans (Web / Docs / PDF):** `Geist` (`Geist-Variable.woff2` and `Geist-Italic-Variable.woff2`, weights `100..900`, SIL Open Font License 1.1).
  - CSS fallback stack: `"Geist", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`
- **Technical & Spec Monospace (Web / Docs / PDF):** `JetBrains Mono` (`JetBrainsMono-Variable.woff2`, weights `100..800`, SIL Open Font License 1.1).
  - CSS fallback stack: `"JetBrains Mono", ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace`
- **CSP & Privacy Rule:** Self-hosted locally in `website/public/fonts/` (`font-src 'self'`). Zero external font CDN requests. The Slint desktop application uses native OS system fonts and does not bundle web fonts.

---

## 6. Asset Inventory

| File | Format / Dimensions | Purpose |
|---|---|---|
| `assets/logo.svg`, `packaging/linux/icons/barepdf.svg`, `website/public/favicon.svg`, `website/public/images/logo.svg` | SVG (`256 × 256`) | Master Concept A vector mark (`#f69423`, `fill-rule="evenodd"`) |
| `assets/app.ico`, `website/public/images/app.ico` | ICO (`16, 24, 32, 48, 256 px`) | Windows executable & Explorer resource icon |
| `website/public/favicon.ico` | ICO (`16, 32, 48 px`) | Legacy browser favicon fallback |
| `assets/icon-white.png`, `website/public/images/icon-white.png` | PNG (`512 × 512`) | Light tile icon & PWA manifest icon |
| `assets/icon-dark.png`, `website/public/images/icon-dark.png` | PNG (`512 × 512`) | Dark `#16171a` tile icon |
| `assets/banner-white.png`, `website/public/images/banner-white.png` | PNG (`3780 × 1890`) | Light-mode README & social banner |
| `assets/banner-dark.png`, `website/public/images/banner-dark.png` | PNG (`3780 × 1890`) | Dark-mode README & social banner |
| `website/public/images/og-image.png` | PNG (`1200 × 630`) | Open Graph / Twitter social preview card |
| `website/public/images/manifesto-cover.png` | PNG (`1024 × 1536`) | Architectural Swiss poster for `/manifesto` |
| `assets/barepdf-welcome.pdf` | PDF (2 pages, A4) | Embedded first-launch EN/TR welcome document |

---

## 7. Don'ts

- Do not stretch, squash, shear, or rotate the symbol or lockup.
- Do not recolour outside the approved palette or add gradients, blurs, drop shadows, glows, or outer strokes.
- Do not rearrange or rescale the symbol relative to the `BarePDF` wordmark inside lockups.
- Do not recreate the wordmark by typing `BarePDF` in an unmodified stock font inside logo files; always use the outlined `<path>` wordmark.
- Do not place `#f69423` or `#f3f4f6` marks directly over noisy photographs without a solid `#16171a` or `#fcfcfd` container.
- Do not ship `<text>`, embedded rasters (`<image>`), or editor metadata inside master `.svg` files.

---

## 8. Master Files & Maintenance

- Repository: `https://github.com/Woffluon/BarePDF`
- Master symbol: `assets/logo.svg`
- Desktop color tokens: `crates/barepdf-ui/ui/tokens.slint`
- Web color & font tokens: `website/src/styles/tokens.css`
