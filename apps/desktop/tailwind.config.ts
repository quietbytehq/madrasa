import type { Config } from "tailwindcss";

/**
 * Theme tokens are defined once as CSS variables in src/styles/tokens.css (light + dark)
 * and exposed here so Tailwind utilities (bg-card, text-muted, border-border, ...) use them.
 */
const token = (name: string) => `rgb(var(--${name}) / <alpha-value>)`;

export default {
  darkMode: ["selector", '[data-theme="dark"]'],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: token("bg"),
        card: token("card"),
        navy: token("navy"),
        blue: token("blue"),
        selection: token("selection"),
        success: { DEFAULT: token("success"), bg: token("success-bg") },
        warning: { DEFAULT: token("warning"), bg: token("warning-bg") },
        danger: { DEFAULT: token("danger"), bg: token("danger-bg") },
        lavender: token("lavender"),
        text: token("text"),
        muted: token("muted"),
        border: token("border"),
        sidebar: token("sidebar"),
        hover: token("hover"),
      },
      fontFamily: {
        sans: ["Inter", "ui-sans-serif", "system-ui", "Segoe UI", "Roboto", "sans-serif"],
        urdu: ["Noto Nastaliq Urdu", "Inter", "system-ui", "sans-serif"],
      },
      fontSize: {
        "2xs": ["0.6875rem", { lineHeight: "1rem" }],
      },
      borderRadius: {
        DEFAULT: "6px",
        md: "6px",
        lg: "8px",
        xl: "10px",
      },
      boxShadow: {
        card: "0 1px 2px rgb(20 47 80 / 0.06), 0 1px 1px rgb(20 47 80 / 0.04)",
        pop: "0 8px 24px rgb(20 47 80 / 0.14), 0 2px 6px rgb(20 47 80 / 0.08)",
      },
      spacing: {
        topbar: "var(--topbar-height)",
        sidebar: "var(--sidebar-width)",
      },
    },
  },
  plugins: [],
} satisfies Config;
