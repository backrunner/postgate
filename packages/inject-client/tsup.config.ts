import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["iife", "esm"],
  dts: true,
  clean: true,
  minify: true,
  sourcemap: true,
  globalName: "PostGateClient",
  outExtension({ format }) {
    return {
      js: format === "iife" ? ".global.js" : ".js",
    };
  },
});
