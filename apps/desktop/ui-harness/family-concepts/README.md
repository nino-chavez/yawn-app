# family-concepts

Three divergent concepts for Yawn on Mac and iPad, 2026-10-15 (judged-screen
pattern § 2c). Each renders the same nine states from `shared/fixtures.js` in
both appearances. Not product code: nothing here ships. The brief they answer
is `docs/experience-brief-family-2026-10-15.md`. The frames and the comparison
page live in `docs/design/concepts/family-2026-10-15/`.

## Concepts

- `1-notebook/` Notebook, the system language. On the Mac the operator's notes
  are the document and the transcript is a labeled section under them, with
  one slim transport. Secondary actions sit in the toolbar menu. Settings is
  Preferences-style with toolbar tabs. On iPad, Concept C in system materials.
- `2-rail/` Rail, the paper language. On the Mac the iPad's time rail runs
  down the page. The transcript turn nearest each note sits level with it and
  the rest fold into a count. The rail is the scrubber. Settings has a
  System Settings-style sidebar. On iPad, Concept C on paper. **This depends
  on a moment per typed line, which the product does not store; the Mac
  frames say so.**
- `3-record/` Record, the instrument language. On the Mac the transcript is
  the record and the document. The operator's notes are a pinned strip above
  it. A full-width timeline with a tick per turn is the scrubber. While
  recording, the live transcript streams under the notes. Settings is a
  single-column form. On iPad, Concept C with the transcript given equal
  width after the meeting.

The decided frames do not change between concepts. The Mac window (Concept A,
selected 2026-09-02) and the iPad page (Concept C, selected 2026-10-14) are
built once, in `shared/chrome.js` and `shared/ipad-c.js`. Each concept skins
them through its `styles.css` and composes the Mac document pane in its
`concept.js`.

## States

Mac, 1080 by 900 (states 1 to 5): after with transcript ready; during, 4:12
in; needs attention; Settings; first run. iPad, 1180 by 820 landscape (states
6 to 9): during; after with replay paused at 11:30; the consent step; the
meeting list open beside a page.

## capture.sh

    ./capture.sh <outdir> [concepts="1 2 3"] [states="1 2 3 4 5 6 7 8 9"] [themes="light dark"]

Renders every concept, state and appearance at 2x into `outdir`.

- Serves this directory on port 8793 (`PORT` overrides it) through `preview`.
- Each frame is one navigation, a device-metrics override, and
  `Page.captureScreenshot` with a 2x clip. `browse-screenshot` returns 1x even
  under the override, so it is not used.
- Uses the session's own browse-tool tab. Run `browse-tabs close-owned` when
  done.

To view one frame: `preview . 8793`, then open
`http://localhost:8793/2-rail/mac.html?state=1&theme=dark`.

## What the frames cannot show

- The iPad handwriting is a font (Bradley Hand). The 2026-10-14 SwiftUI
  concepts used the same stand-in. Real finger strokes come from the device
  build of the selected concept.
- Fold counts ("+ 20 turns") assume a turn every ten seconds, the pace the
  iPad test recording measured. The invented transcript itself is sparse.
- The Mac note moments in Concept 2 are invented. The product stores one note
  text with no time per line.
- Settings tab and sidebar glyphs are gray squares standing in for SF Symbols.
- The iPad frames are HTML at iPad Air 11-inch points, not SwiftUI. Materials,
  blur and the window resize handle are absent. The selected concept is
  accepted on its SwiftUI build, cold-reviewed on the device.
