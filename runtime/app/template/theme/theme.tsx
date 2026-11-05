// A UI theme: the default's colours, metrics and fonts, changed where you say.
// `ui_theme = "{{name}}"` picks it (Settings → Appearance).
import { defineTheme } from "pumi";

export default defineTheme({
  name: "{{name}}",
  colors: { accent: "#7aa2f7" },
  metrics: { radius: 8 },
});
