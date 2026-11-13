import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss()],
  oxc: {
    jsx: {
      runtime: "automatic",
      importSource: "preact",
    },
  },
  resolve: {
    alias: {
      react: fileURLToPath(new URL("./node_modules/preact/compat/", import.meta.url)),
      "react-dom": fileURLToPath(new URL("./node_modules/preact/compat/", import.meta.url)),
      "react/jsx-runtime": fileURLToPath(
        new URL("./node_modules/preact/jsx-runtime", import.meta.url),
      ),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:7000",
        changeOrigin: true,
      },
    },
  },
});
