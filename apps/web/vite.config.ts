import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // `cargo run -- serve` listens on 8080.
  server: { proxy: { "/api": "http://localhost:8080" } },
});
