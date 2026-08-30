import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives this dev server; the fixed port and strict mode keep the
// shell's `devUrl` honest instead of silently moving to another port.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021", sourcemap: true },
});
