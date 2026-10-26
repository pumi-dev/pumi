// A UI theme: the default's colours, metrics and fonts, changed where you say.
// `ui_theme = "{{name}}"` picks it (Settings → Appearance).
import { defineTheme } from "pumi";

export default defineTheme({
  name: "{{name}}",
  colors: {
    dark: { accent: "#7aa2f7" },
    light: { accent: "#2e5bd8" },
  },
  metrics: { radius: 8 },
});
