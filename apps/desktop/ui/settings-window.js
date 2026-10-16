// The standalone Settings host. The page itself is settings.js; the main
// window mounts the same page in its document pane.
import { applyAppearance, watchSystemAppearance } from "./appearance.mjs";
import { mountSettings } from "./settings.js";

applyAppearance();
watchSystemAppearance();
mountSettings(document.querySelector(".settings-shell"));
