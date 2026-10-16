# Rail palette contrast — measured 2026-10-15

Measured with the WCAG 2.1 relative-luminance formula (sRGB) in a short Python
check on 2026-10-15, on the values in `DESIGN.md`. The soft tints (`rgba`)
are not text surfaces; a Moment or Attention foreground on its soft tint reads
against the page beneath, which is the pair measured. 4.5:1 is the floor for
text, 3:1 for large text and for marks that carry meaning.

The concept frames' first values failed: Label 3 (`#8f8a7e` light, `#7a756a`
dark) read 2.68:1 to 3.80:1 on the page, chrome and sidebar, and light
Attention (`#a9701d`) read 3.97:1 on the page. They were darkened (light) and
lightened (dark) to the values below before the port, and the Rail frames
were recaptured with them.

| Appearance | Pair | Foreground | Background | Ratio | Text floor |
|---|---|---|---|---:|---|
| Light | Label on Page | `#201e1a` | `#faf9f5` | 15.79:1 | meets |
| Light | Label 2 on Page | `#645f55` | `#faf9f5` | 6.02:1 | meets |
| Light | Label 3 on Page | `#67625a` | `#faf9f5` | 5.74:1 | meets |
| Light | Label 3 on Window chrome | `#67625a` | `#ecebe6` | 5.07:1 | meets |
| Light | Label 3 on Sidebar | `#67625a` | `#e5e3dc` | 4.71:1 | meets |
| Light | Label 3 on Panel | `#67625a` | `#f3f2ed` | 5.40:1 | meets |
| Light | Label 2 on Panel | `#645f55` | `#f3f2ed` | 5.66:1 | meets |
| Light | Moment on Page | `#2f5fa8` | `#faf9f5` | 6.00:1 | meets |
| Light | On moment on Moment | `#ffffff` | `#2f5fa8` | 6.32:1 | meets |
| Light | Record on Page | `#c7392c` | `#faf9f5` | 4.92:1 | meets |
| Light | On record on Record | `#fffaf7` | `#c7392c` | 5.00:1 | meets |
| Light | Attention on Page | `#8f5d12` | `#faf9f5` | 5.33:1 | meets |
| Light | On graphite on Graphite | `#faf9f5` | `#3d3a34` | 10.76:1 | meets |
| Light | Label on Control | `#201e1a` | `#fffefb` | 16.50:1 | meets |
| Dark | Label on Page | `#ece7db` | `#262521` | 12.43:1 | meets |
| Dark | Label 2 on Page | `#a39d8f` | `#262521` | 5.68:1 | meets |
| Dark | Label 3 on Page | `#999487` | `#262521` | 5.07:1 | meets |
| Dark | Label 3 on Window chrome | `#999487` | `#1b1a17` | 5.75:1 | meets |
| Dark | Label 3 on Sidebar | `#999487` | `#1f1e1a` | 5.51:1 | meets |
| Dark | Label 3 on Panel | `#999487` | `#2b2a25` | 4.75:1 | meets |
| Dark | Label 2 on Panel | `#a39d8f` | `#2b2a25` | 5.32:1 | meets |
| Dark | Moment on Page | `#8fb4ff` | `#262521` | 7.41:1 | meets |
| Dark | On moment on Moment | `#0f1a2e` | `#8fb4ff` | 8.40:1 | meets |
| Dark | Record on Page | `#ff6a5c` | `#262521` | 5.46:1 | meets |
| Dark | On record on Record | `#1b0c09` | `#ff6a5c` | 6.77:1 | meets |
| Dark | Attention on Page | `#d9a24a` | `#262521` | 6.73:1 | meets |
| Dark | On graphite on Graphite | `#1b1a17` | `#d6cfbf` | 11.22:1 | meets |
| Dark | Label on Control | `#ece7db` | `#35332d` | 10.24:1 | meets |

Text pairs below 4.5:1: 0.

## Marks and guides

The rail line (`#cfcabd` light, `#4d4a42` dark) reads 1.55:1 and 1.73:1 against the
page, and the scrub track is a hairline. Both are guides: the meaning sits on
the dots, the playhead and the ticks, which use Label (`#201e1a` / `#ece7db`)
or Moment and clear 3:1 as marks. A reviewer who cannot see the line still
sees every mark.
