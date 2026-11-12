import type { Config } from "tailwindcss";
import desktop from "../desktop/tailwind.config";

/**
 * The desktop app's theme (tokens, fonts, radii), unchanged. Only the list of files to scan
 * differs: this app's screens and the desktop components it uses.
 */
export default {
  ...desktop,
  content: [
    "./index.html",
    "./src/**/*.{ts,tsx}",
    "../desktop/src/components/**/*.{ts,tsx}",
    "../desktop/src/theme/**/*.{ts,tsx}",
  ],
} satisfies Config;
