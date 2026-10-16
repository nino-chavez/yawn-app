// The standalone Settings host. The page itself is settings.js; the main
// window mounts the same page in its document pane.
import { applyAppearance, watchSystemAppearance } from "./appearance.mjs";
import { applyTextSize } from "./text-size.mjs";
import { mountSettings } from "./settings.js";

applyAppearance();
watchSystemAppearance();
applyTextSize();
mountSettings(document.querySelector(".settings-shell"));
