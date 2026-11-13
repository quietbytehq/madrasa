# ADR 0008: Brand identity in the product

- Status: accepted
- Date: 2026-11-13

## Context

The owner supplied a brand sheet (`assets/brand/Madrasa_Brand_Identity_Showcase.png`): an
emblem (a gold dome with a crescent over a night-blue arch, a white minaret with a gold light,
an open book), a serif wordmark with a sans tagline, an app icon on a night-blue tile, and
light and dark versions. The product still showed an "M" in a navy square, Tauri's default
icon on Android and colours chosen before the brand existed.

The brand had to reach the desktop app (Windows, Linux), the parent app (Android), the Nexus
web console and printed documents, without changing any layout or feature.

## Decision

1. **The SVGs are drawn from code.** `assets/brand/tools/build_svg.py` draws the emblem as
   geometry and turns the wordmark (Merriweather Black) and tagline (Inter Medium) into
   outlines, so the files look the same with no fonts installed: on GitHub, in installers,
   in PDFs. It writes `madrasa-icon.svg`, `madrasa-mark.svg` and the horizontal and vertical
   logos for light and dark backgrounds. Edit the script, not the SVGs.
2. **One picture for every icon.** `assets/brand/generate-icons.sh` makes every app icon from
   `madrasa-icon.svg` with the Tauri CLI: the desktop sets of both apps (Windows `.ico`, Linux
   PNGs, macOS `.icns`, Store logos) and the Android launcher icons (legacy, round and
   adaptive, every density). The adaptive icon's two layers are cut from the same SVG (the
   tile; the emblem, smaller, inside the launcher's safe circle). The Android icons are
   committed and the APK build copies them in, rather than regenerating them from a PNG.
3. **Colours as tokens.** Navy, blue, selection, text and border move to the brand's night
   blue and royal blue; a `gold` token (with `gold-bg`) is added; dark mode is re-tinted to the
   brand's night blue. Status colours (success, warning, danger) are unchanged. The desktop
   app, the parent app and the console share one set (`tokens.css`, `tailwind.config.ts`).
4. **Fonts.** Inter stays the interface font. `font-display` (Merriweather Black, bundled
   through Fontsource, OFL) is the wordmark's serif and is used only for the product name;
   Urdu falls through to Noto Nastaliq.
5. **Where the logo appears.** The emblem replaces the "M" square at the same size (sidebar
   header, login and first-run screens, console, parent app), so no layout moves. The startup
   screen shows the vertical logo: in the HTML before the app's code runs, then in the app
   while it starts. Server PDFs (vouchers, report cards, offer letters) and the on-screen print
   views carry the emblem in their header; the PDF reads the SVG compiled into the server.
6. **No layout or feature changes.** Screens were photographed before and after with
   Playwright against the demo school to check it.

## Consequences

- One file to change for a new icon; one script to change for a new logo.
- The console's Docker build copies `assets/brand` (the console imports the SVGs).
- Navy and blue are a little deeper than before; contrast of white on blue rises.
- Rebuilding the SVGs needs Python with fontTools and the two Fontsource packages; building
  the apps does not.
