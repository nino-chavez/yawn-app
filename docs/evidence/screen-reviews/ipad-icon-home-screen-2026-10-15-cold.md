# iPad app icon C2 on the home screen — cold review, 2026-10-15

**Kind:** cold screen review, one pass, by a fresh subagent. The reviewer saw
one image and five neutral questions. It had no source, no design notes, no
rationale and no other concept. The image is a crop of a screenshot taken on
the operator's iPad Air (iPadOS 27.0) with `xcrun devicectl device capture
screenshot`, after the research build with the C2 icon
([decision](../../../decisions/ipad-icon-selection-2026-10-14.md)) was
installed. It shows the icon beside three Apple icons from the same home
screen: Freeform, Stocks and Voice Memos.

Frame: [`ipad-icon-2026-10-15/home-screen-neighbors.png`](ipad-icon-2026-10-15/home-screen-neighbors.png).

**Question:** Before reading the label, what does the app seem to do? Are
there letters in it? Could it be confused with a neighbor? Does it fit its
name? What one change would help most?

| Question | Reviewer's answer |
|---|---|
| What it does | First guess a handwriting or drawing app; second, a note-taking or sketching app, "pen scribble on a timeline or ruler" |
| What it shows | A vertical dotted line with purple dots, and a purple squiggle curving out of it to the right. **No letters or words read** |
| Confusable | Closest to Freeform, which also shows one squiggly pen stroke. Voice Memos and Stocks share a vertical line with a dot on it, so the family feel is strong. It is the only light icon in the row |
| Fits "iPad Ink" | Yes: the squiggle reads as ink. The rail adds "a hint of something beyond plain drawing" |
| One change | Make the stroke larger and bolder, and drop or thicken the dotted line: at home-screen size the rail and dots are faint, so the stroke carries the icon |

**Verdict:** pass with one revise item.

- **Pass.** It reads as handwriting or notes, which is the job. The letter
  risk recorded in the decision ("n" and "e" at 167 px) did not show: the
  reviewer read no letters at home-screen size.
- **Revise item.** The rail, the part that says "tied to a moment", is faint
  on the device. The reviewer read it as "a ruler or timeline" only on close
  description, and its first answer did not mention time. Thickening the rail
  and raising the quiet marks' contrast is the candidate fix. The operator
  asked for it the same day; it is applied, with a device before and after in
  [`ipad-icon-2026-10-15/home-screen-rail-before-after.png`](ipad-icon-2026-10-15/home-screen-rail-before-after.png).
  The revised icon has not had its own cold pass.

**Agent's own observation from the full screenshot.** Apple's icons on the
same page use the same vocabulary. Freeform is a loose stroke; Stocks and
Voice Memos each have a vertical line with a dot marker. C2 combines the two.
It stays distinct by being the only white tile with violet ink, not by its
shapes.

**Limits.** One pass, one reviewer, one light home screen. Dark and tinted
home screens were not captured on the device; they rely on the `ictool`
renders in `docs/design/concepts/ipad-icon-2026-10-14/`.
