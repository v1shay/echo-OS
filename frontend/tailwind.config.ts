import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        canvas: "#020202",
        ink: "#f6f5f0",
        muted: "#6f6f67",
        pulse: "#9df7cf",
        accent: "#f2b36d",
      },
      fontFamily: {
        display: ['"IBM Plex Mono"', "monospace"],
        body: ['"Space Grotesk"', "sans-serif"],
      },
      boxShadow: {
        ambient: "0 0 80px rgba(157, 247, 207, 0.12)",
      },
    },
  },
  plugins: [],
} satisfies Config;
