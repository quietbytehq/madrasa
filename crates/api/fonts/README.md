# Fonts embedded in the API for PDF reports

| File | Family | Licence |
| --- | --- | --- |
| `Inter-Regular.ttf`, `Inter-Bold.ttf` | Inter 4.1 (rsms.me/inter) | SIL Open Font License 1.1 (`LICENSE-Inter.txt`) |
| `NotoNastaliqUrdu-Regular.ttf`, `NotoNastaliqUrdu-Bold.ttf` | Noto Nastaliq Urdu (Google Fonts, v23) | SIL Open Font License 1.1 (`LICENSE-NotoNastaliqUrdu.txt`) |

They are compiled into the executable (`crates/api/src/reports/pdf.rs`) so that reports render
offline with the same fonts as the UI, with Urdu shaped and right-to-left.
