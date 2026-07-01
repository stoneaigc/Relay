import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// 刷新 /admin(无结尾斜杠)时 Vite 会 404 并提示 base —— 重定向到 /admin/。
const baseSlashRedirect = (base: string): Plugin => ({
  name: "base-slash-redirect",
  configureServer(server) {
    const noSlash = base.replace(/\/$/, "");
    server.middlewares.use((req, res, next) => {
      const path = (req.url || "").split("?")[0];
      if (path === noSlash) {
        res.writeHead(301, { Location: base });
        res.end();
        return;
      }
      next();
    });
  },
});

export default defineConfig({
  base: "/admin/",
  plugins: [baseSlashRedirect("/admin/"), react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    port: 5174,
    proxy: { "/admin/api": "http://localhost:8080" },
  },
});
