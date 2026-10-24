import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The UI lives in ui/; Tauri loads the dev server on port 1420 and the build from dist/ui.
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist/ui", emptyOutDir: true, target: "es2022" },
});
