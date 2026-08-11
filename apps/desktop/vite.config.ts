import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    // Component tests (jsdom + @testing-library/svelte) live next to the
    // components; pure-function tests keep the default node environment.
    environment: "jsdom",
  },
});
