/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Hanken Grotesk"', "Inter", "system-ui", "sans-serif"],
        display: ['"Hanken Grotesk"', "Inter", "system-ui", "sans-serif"],
        mono: ['"JetBrains Mono"', "Consolas", "ui-monospace", "monospace"],
      },
      colors: {
        "tg-bg": "rgb(var(--tg-surface) / <alpha-value>)",
        "tg-lowest": "rgb(var(--tg-surface-lowest) / <alpha-value>)",
        "tg-low": "rgb(var(--tg-surface-low) / <alpha-value>)",
        "tg-card": "rgb(var(--tg-surface-container) / <alpha-value>)",
        "tg-high": "rgb(var(--tg-surface-high) / <alpha-value>)",
        "tg-highest": "rgb(var(--tg-surface-highest) / <alpha-value>)",
        "tg-ink": "rgb(var(--tg-on-surface) / <alpha-value>)",
        "tg-muted": "rgb(var(--tg-on-surface-variant) / <alpha-value>)",
        "tg-outline": "rgb(var(--tg-outline-variant) / <alpha-value>)",
        "tg-primary": "rgb(var(--tg-primary) / <alpha-value>)",
        "tg-primary-container": "rgb(var(--tg-primary-container) / <alpha-value>)",
        "tg-secondary": "rgb(var(--tg-secondary) / <alpha-value>)",
        "tg-tertiary": "rgb(var(--tg-tertiary) / <alpha-value>)",
        "tg-error": "rgb(var(--tg-error) / <alpha-value>)",
      },
    },
  },
  plugins: [],
};
