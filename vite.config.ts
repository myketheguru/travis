import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteStaticCopy } from "vite-plugin-static-copy";
import { resolve } from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [
    react(),
    tailwindcss(),
    // Bundle Pyodide assets locally so Travis works offline.
    // vite-plugin-static-copy v4 preserves the source's leading path
    // regardless of dest/rename, so files land at
    // dist/pyodide-bundle/node_modules/pyodide/ — indexURL matches that.
    viteStaticCopy({
      targets: [
        // Core Pyodide runtime + lock manifest + every wheel we
        // pre-fetched via scripts/fetch-pyodide-wheels.mjs (both the
        // in-lock packages and the pure-Python PyPI extras like
        // reportlab/pypdf/python-docx). All wheels land in one dir
        // so `loadPackagesFromImports` and `micropip.install` both
        // resolve to local URLs — no jsdelivr CDN round trip.
        // pyodide-extras.json is the manifest the interpreter reads
        // at runtime to map extra-package names → wheel filenames.
        {
          src: "node_modules/pyodide/*.{wasm,asm.js,zip,json,mjs,whl}",
          dest: "pyodide-bundle",
        },
      ],
    }),
  ],
  clearScreen: false,
  optimizeDeps: {
    exclude: ["pyodide"],
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
    // Allow CORS headers needed by Pyodide's WASM loading
    headers: {
      "Cross-Origin-Embedder-Policy": "credentialless",
      "Cross-Origin-Opener-Policy": "same-origin",
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        interpreter: resolve(__dirname, "interpreter.html"),
      },
    },
    // Increase chunk size warning — interpreter bundle is naturally larger
    chunkSizeWarningLimit: 1500,
  },
}));
