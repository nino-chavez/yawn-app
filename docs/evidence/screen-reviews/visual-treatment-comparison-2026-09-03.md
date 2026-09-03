# Yawn visual-treatment comparison — 2026-09-03

## Result

Precision utility is the recommended leader. After one refinement pass, it
clears the static harness comparison bar across sparse, dense, and
needs-attention states in dark and light appearance.

This is not operator selection and not installed-app acceptance. No treatment
has been ported into the product. Interaction, native rendering, window
resizing, large text, and the complete unavailable-status disclosure remain
native-app work.

## What was held constant

All three treatments used the same Concept A structure, semantic HTML, copy,
meeting list, and fixtures at 1080 by 900. Each treatment changed only its
visual layer. The primary comparison was the sparse state that exposed the
installed screen's failure. Dense note plus source inspector and
needs-attention recovery were cross-state checks.

The 18 frames are under
`apps/desktop/ui-harness/visual-treatments/out/`. The comparison page is
`apps/desktop/ui-harness/visual-treatments/compare.html`.

## Blind review

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

## Next boundary

An operator selection can promote one treatment into `DESIGN.md`. Only then
should its visual rules be ported into `apps/desktop/ui/styles.css`. A port is
not accepted until fresh installed captures cover the sparse note, dense note
with inspector, needs-attention recovery, active recording, paused recording,
Settings, and both appearances, followed by a cold review.
