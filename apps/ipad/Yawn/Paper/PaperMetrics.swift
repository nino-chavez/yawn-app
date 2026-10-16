import SwiftUI

/// Geometry from DESIGN.md ("Composition, iPad" and "Components and states"),
/// in points. Text sizes are not here: they are Dynamic Type styles at the
/// point of use.
enum PaperMetrics {
    /// One device pixel at 2x: the hairline.
    static let hairline: Double = 0.5
    /// The rail's distance from the page's left edge, and the line itself.
    static let railInset: Double = 64
    static let railWidth: Double = 2
    /// A note's dot on the rail.
    static let markDiameter: Double = 9
    /// Where ink starts, and what it keeps clear at the right for the tools.
    static let inkInset: Double = 92
    static let toolColumnWidth: Double = 56
    static let toolSpacing: Double = 18
    /// The iPad capsule, its side padding, the recording badge, the consent
    /// card's button. A capsule is 36pt tall; its touch target is 44pt.
    static let capsuleHeight: Double = 36
    static let capsulePadding: Double = 14
    static let touchTarget: Double = 44
    static let badgeHeight: Double = 32
    static let startButtonHeight: Double = 50
    /// The consent card.
    static let consentCardWidth: Double = 420
    static let consentCardRadius: Double = 18
    /// The floating meeting list.
    static let sidebarWidth: Double = 320
    static let sidebarRadius: Double = 20
    static let sidebarInset: Double = 8
    /// A row's corner and the selected row's locator.
    static let rowRadius: Double = 6
    static let locatorWidth: Double = 3
}
