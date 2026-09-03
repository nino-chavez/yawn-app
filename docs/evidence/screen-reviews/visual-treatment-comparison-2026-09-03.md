# Yawn visual-treatment comparison — 2026-09-03

## Result

The operator selected **Tonal Ledger — Canvas** on 2026-09-03. Marquee
Document — Quiet and every earlier treatment are rejected and retained only
for archaeology. The selection promotes the center pane as the document
surface, the violet-neutral tonal system, quiet selected-row locator, 16px
reading text, and bounded personal-notes and recovery surfaces.

The first blind review had ranked Precision Utility first. The operator did
not select it and later said Private Notebook was closer, while still not
polished enough to promote. The two later waves separated and refined the
qualities that led to the final Canvas selection.

The second wave therefore does not iterate Precision. It separates three
possible reasons Private Notebook felt closer: a bounded reading object,
warmer editorial material, and stronger hierarchy. Tonal Ledger, Quiet Folio,
and Marquee Document test those reasons independently. They are not ranked in
this record because they have not had an independent cold review.

Selection is not installed-app acceptance. The static treatment is now ported
into the product source; installed rendering, window resizing, large text, and
the complete unavailable-status disclosure remain separate verification.

## What was held constant

All six treatments use the same Concept A structure, semantic HTML, copy,
meeting list, and fixtures at 1080 by 900. Each treatment changed only its
visual layer. The primary comparison was the sparse state that exposed the
installed screen's failure. Dense note plus source inspector and
needs-attention recovery were cross-state checks.

The 36 frames are under
`apps/desktop/ui-harness/visual-treatments/out/`. The comparison page is
`apps/desktop/ui-harness/visual-treatments/compare.html`.

## First-wave blind review

The reviewer saw the rendered frames before reading any treatment rationale,
CSS, design brief, or implementation source.

| Rank | Treatment | What survived the comparison | What limited it |
|---|---|---|---|
| 1 | Precision utility | The document stayed primary while selection, management controls, notes, and source evidence kept their rank in all six frames. | The first pass clipped the unavailable toolbar sentence, floated recovery too low, and left dark secondary text faint. |
| 2 | Native editorial | The note gained a calm, distinct reading voice and the notes field became legibly editable. | Display type made long titles too forceful and turned the dense three-pane state into a document spread. |
| 3 | Private notebook | Warm light appearance, selection, and recovery treatment felt considered. | The narrow column and serif treatment read as a notebook or article product under dense content. |

All three improved the observed failure screen: the selected row stopped
dominating the document, operator notes became visibly editable, management
actions moved down the hierarchy, and sparse content no longer sat as a small
cluster in a flat charcoal void.

## Precision refinement

The reviewer did not clear the first Precision pass. A bounded second pass
made four corrections:

- Replaced the visibly clipped unavailable sentence with `Finishing` or
  `Needs attention`. The full reason remains in the document for assistive
  technology; the product still needs a focusable disclosure for that text.
- Top-aligned the recovery document and attached its warning panel to the
  selected meeting header.
- Raised dark secondary text contrast without making metadata compete with
  the note.
- Reduced the empty notes editor and added a quiet insertion edge so it reads
  as available writing space instead of a fixed blank panel.

The second blind read marked the toolbar, recovery placement, and notes
affordance resolved. Dark secondary contrast improved enough to stop blocking
the static frame, but it still needs an installed-display check.

## Operator follow-up and second wave

The operator's later judgment overrides the recommendation as a product
decision: Private Notebook was closer, but not close enough. Direct inspection
of Minder's current design record and rendered Marquee concept supplied a
useful distinction. Minder's present physical build has too many bordered
objects and competing actions to copy. Marquee's useful principle is one
decisive current object, low-chroma tonal surfaces, and supporting rows that
recede.

| Treatment | What it preserves from Private Notebook | What it tests from Minder |
|---|---|---|
| Tonal Ledger | One contained, personal reading object. | Low-chroma violet, system sans, and a clear dominant surface. |
| Quiet Folio | A warm editorial reading voice and visibly writable notes. | Chrome recedes; warmth stays inside the note instead of tinting the whole product. |
| Marquee Document | The note, rather than controls, carries the window. | One full-bleed dominant object with almost no enclosing material. |

All three were rendered in sparse, dense, and needs-attention states in both
appearances. This in-session cross-state inspection found no clipping,
hierarchy reversal, or failed attention layout. That is an implementation
check, not a cold-review verdict.

## Operator convergence and finalist iteration

The operator retained Tonal Ledger and Marquee Document and rejected Native
Editorial, Private Notebook, Precision Utility, and Quiet Folio. The rejection
is a design decision. The rejected files remain available for archaeology, but
the active comparison no longer presents them as candidates.

The next pass keeps the two parent treatments visible as baselines and adds one
bounded refinement of each:

| Refinement | Parent strength retained | Correction under test |
|---|---|---|
| Tonal Ledger — Canvas | Low-chroma tone and a focused reading object. | The pane supplies containment; the tall rounded card is removed. |
| Marquee Document — Quiet | Full-bleed note and decisive hierarchy. | The title is smaller and section headings return to sentence case. |

The active comparison now contains 24 frames: two refinements and two parent
baselines, each in three states and both appearances. The refinements remain
unranked until cold review is complete.

Rendered inspection found both corrections intact across sparse, dense, and
needs-attention states:

- Tonal Ledger — Canvas removes the large card without losing a bounded reading
  area. Its editable notes surface remains the clearest difference from the
  full-bleed alternative.
- Marquee Document — Quiet keeps the note dominant while the smaller title and
  sentence-case headings stop reading like an editorial publishing system.
- Neither refinement clips, reverses the document/selection hierarchy, or
  separates recovery from the selected meeting header.

The first capture exposed sub-AA placeholder and metadata colors in both
refinements. The faint and secondary light/dark tokens were raised before the
final capture. The checked pairs now range from 4.53:1 to 5.47:1. All 24 active
frames are 1080 by 900. This is still an implementer inspection, not the
independent cold review required for selection.

## Next boundary

The operator selection is now the visual authority in `DESIGN.md`, and its
rules are ported into `packages/design-tokens/tokens.json` and
`apps/desktop/ui/styles.css`. A source port is not installed acceptance. Fresh
installed captures still need to cover the sparse note, dense note with
inspector, needs-attention recovery, active recording, paused recording,
Settings, and both appearances, followed by a cold review.

## Source-port and Preview receipt

The selected rules were ported on 2026-09-03. Verification covered the real
frontend in both appearances, including a populated note, selected row,
personal notes, and startup recovery. The token generator check and all 150 UI
tests passed. The external-model runtime staged successfully, and
`Yawn Preview.app` passed `prepare-preview-bundle.sh verify`.

Independent bundle checks reported Developer ID Application team
`34VZ63G58M`, and the bundled permission probe carried
`com.apple.security.device.audio-input = true`. This proves the exact local
Preview bundle and its source port. It does not prove notarization, public
release, installation in Applications, real-device interaction, or installed
visual acceptance. The installed capture helper also requires the operator to
be away because it drives the GUI; that condition was not asserted in this
session, so no synthetic installed-app capture was taken.
