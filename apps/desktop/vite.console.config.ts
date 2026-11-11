import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

/**
 * The Madrasa Nexus hub console: a second build of this app's components and translations,
 * without the Tauri shell. The hub serves the result (`dist-console/`) itself, so the API
 * is on the same origin. `npm run build:console`; `npm run dev:console` proxies the API to
 * a hub on this machine.
 */

/** The hub serves `index.html`; the entry here is `console.html` so both builds can coexist. */
function asIndexHtml(): Plugin {
  return {
    name: "console-as-index-html",
    enforce: "post",
    generateBundle(_options, bundle) {
      const page = bundle["console.html"];
      if (page) page.fileName = "index.html";
    },
  };
}

export default defineConfig({
  plugins: [react(), asIndexHtml()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    open: "/console.html",
    proxy: { "/api/nexus": process.env.NEXUS_DEV_HUB ?? "http://127.0.0.1:4780" },
  },
  build: {
    outDir: "dist-console",
    emptyOutDir: true,
    target: "es2020",
    rollupOptions: { input: "console.html" },
  },
});
