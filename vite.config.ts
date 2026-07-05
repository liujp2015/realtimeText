import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    // Android 真机调试时，Tauri 会把 devUrl host 改成局域网 IP，
    // vite 必须监听 0.0.0.0 才能让设备访问到 dev server。
    host: process.env.TAURI_ENV_PLATFORM === "android" ? "0.0.0.0" : false,
    port: 1420,
    strictPort: true,
    // HMR websocket 默认连到页面 host（tauri.localhost），手机上连不上。
    // 让 HMR 客户端直连 PC 局域网 IP（Tauri 注入的 TAURI_DEV_HOST）。
    hmr:
      process.env.TAURI_ENV_PLATFORM === "android" && process.env.TAURI_DEV_HOST
        ? { host: process.env.TAURI_DEV_HOST, port: 1420, protocol: "ws" as const }
        : undefined,
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "dashboard.html"),
        subtitle: resolve(__dirname, "subtitle.html"),
      },
    },
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
    },
  },
});
