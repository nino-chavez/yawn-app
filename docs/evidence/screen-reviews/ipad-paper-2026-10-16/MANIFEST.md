# iPad paper captures, 2026-10-16

Build: the `design/ipad-paper` worktree at the commit this review names, Debug
configuration, installed on the iPad Air 11-inch (M4) simulator
(`1518BEE4-FD71-4B7A-8504-0A1A65C9AF73`), iOS 26.5, portrait, 820 by 1180
points at 2x. Not device captures: the operator's iPad was not connected.

Simulator state read with `xcrun simctl ui` before and after the run:
appearance light, content size large, increase contrast disabled. Each frame
names the one setting it changed; every setting was restored afterwards.

| File | State | Appearance | Text size | Increase contrast |
|---|---|---|---|---|
| first-run-light.png | first run, sidebar closed | light | large (default) | off |
| first-run-dark.png | first run, sidebar closed | dark | large | off |
| first-run-light-xxxl.png | first run, sidebar closed | light | extra-extra-extra-large | off |
| first-run-light-increased-contrast.png | first run, sidebar closed | light | large | on |
| sidebar-light.png | first run, sidebar open (no meetings) | light | large | off |
| sidebar-dark.png | first run, sidebar open (no meetings) | dark | large | off |
| specimen-light.png | the prepared components (`-paperSpecimen`, debug builds only) | light | large | off |
| specimen-dark.png | the prepared components | dark | large | off |
| specimen-light-xxxl.png | the prepared components | light | extra-extra-extra-large | off |

The cold packet used neutral names: i1 first-run-light, i2 first-run-dark,
i3 first-run-light-xxxl, i4 first-run-light-increased-contrast, i5
sidebar-light, i6 sidebar-dark, i7 specimen-light, i8 specimen-dark, i9
specimen-light-xxxl.
