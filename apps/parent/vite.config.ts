/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

const host = process.env.TAURI_DEV_HOST;

/**
 * The Madrasa parent app. It is built from the desktop app's design system: `@/…` is the
 * desktop app's source (components, tokens, translations), `@parent/…` is this app's own.
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@parent": fileURLToPath(new URL("./src", import.meta.url)),
      "@": fileURLToPath(new URL("../desktop/src", import.meta.url)),
    },
    // One copy of React, whichever app's folder a file lives in.
    dedupe: ["react", "react-dom", "react-router-dom", "lucide-react"],
  },
  clearScreen: false,
  server: {
    port: 1440,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1441 } : undefined,
    fs: { allow: [".."] },
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2020",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // One file, shipped inside the app: both languages' strings make it large, and
    // nothing is downloaded when the app opens.
    chunkSizeWarningLimit: 900,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
  },
});
