import SwiftUI

/// The paper palette (DESIGN.md, Color) as the asset catalog colors that
/// `packages/design-tokens/generate.js` writes into `Paper.xcassets` from
/// `tokens.json`. Views take every color from here and nowhere else: no
/// literal color lives in a view, so the Mac and the iPad cannot drift.
enum Paper {
    /// Toolbar, nav bar, status bar.
    static let windowChrome = Color(.paperWindowChrome)
    /// The meeting list.
    static let sidebar = Color(.paperSidebar)
    /// The page.
    static let page = Color(.paperPage)
    /// The transcript column.
    static let panel = Color(.paperPanel)
    /// Separators.
    static let hairline = Color(.paperHairline)
    /// The scrub track; increased contrast.
    static let hairlineStrong = Color(.paperHairlineStrong)

    /// Primary text.
    static let label = Color(.paperLabel)
    /// Secondary text, transcript turns.
    static let label2 = Color(.paperLabel2)
    /// Captions, times, group labels.
    static let label3 = Color(.paperLabel3)
    /// Disabled, placeholders.
    static let label4 = Color(.paperLabel4)

    /// The chrome's accent: selection locator, primary button, active control.
    static let graphite = Color(.paperGraphite)
    static let onGraphite = Color(.paperOnGraphite)
    /// Selected row surface.
    static let selection = Color(.paperSelection)
    /// Capsules, the search field.
    static let control = Color(.paperControl)
    /// Their one border.
    static let controlBorder = Color(.paperControlBorder)

    /// The rail line.
    static let rail = Color(.paperRail)
    /// A note's dot on the rail.
    static let railMark = Color(.paperRailMark)

    /// Recording only.
    static let record = Color(.paperRecord)
    static let onRecord = Color(.paperOnRecord)
    /// The recording badge, paused.
    static let recordSoft = Color(.paperRecordSoft)

    /// A condition that needs attention.
    static let attention = Color(.paperAttention)
    /// The bounded attention surface.
    static let attentionSoft = Color(.paperAttentionSoft)

    /// The playhead, a tapped line, the current turn, the active pen.
    static let moment = Color(.paperMoment)
    static let onMoment = Color(.paperOnMoment)
    /// The current turn's surface, the active tool's capsule.
    static let momentSoft = Color(.paperMomentSoft)

    /// Strokes written after the playhead during replay.
    static let inkDim = Color(.paperInkDim)
}
