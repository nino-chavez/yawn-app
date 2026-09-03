// One left click at a screen point, for the installed-app capture pass.
//
// Why this exists rather than `osascript -e 'tell application "System Events"
// to click at {x, y}'`: that form is meant for UI *elements*, and at a bare
// coordinate it does nothing and reports success. On 2026-09-02 it cost a frame
// -- a capture named `02-dark-note` came back as a second copy of the previous
// screen, because the click that should have changed the selection had silently
// no-oped. `cap_frame`'s read-back caught it; the click itself gave no signal.
//
// Why a click is needed at all: `Yawn Preview` is a Tauri app, and its window
// exposes only an AXGroup and the three traffic-light buttons to accessibility.
// There are no named elements inside the web view to target, and the app's
// menus reach New Recording, Stop Recording, Show/Hide Sidebar, Open Full
// Transcript and Settings -- not meeting selection, not Generate note, not the
// retry or delete sheets. Those states cannot be reached any other way.
//
// Build: swiftc -O scripts/capture-click.swift -o scripts/.bin/capture-click
// (`cap_click` in capture-installed-screens.sh does this on first use.)
//
// This posts a real HID event to whatever is frontmost. `cap_click` asserts
// Yawn is frontmost immediately before calling it, and refuses otherwise --
// that check is not optional, and it is the whole reason this is safe to run.

import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3, let x = Double(args[1]), let y = Double(args[2]) else {
    FileHandle.standardError.write("usage: capture-click <x> <y>\n".data(using: .utf8)!)
    exit(2)
}

let point = CGPoint(x: x, y: y)
guard let source = CGEventSource(stateID: .hidSystemState) else {
    FileHandle.standardError.write("could not create an event source\n".data(using: .utf8)!)
    exit(3)
}

// Move first, then click. A down/up pair at a point the cursor has not reached
// lands correctly for most controls but not for anything that reacts to hover
// before the press, which several of these surfaces do.
CGEvent(mouseEventSource: source, mouseType: .mouseMoved,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(60_000)
CGEvent(mouseEventSource: source, mouseType: .leftMouseDown,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(40_000)
CGEvent(mouseEventSource: source, mouseType: .leftMouseUp,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
