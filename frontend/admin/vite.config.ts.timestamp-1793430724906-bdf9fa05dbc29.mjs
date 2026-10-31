// vite.config.ts
import { defineConfig } from "file:///H:/AI%20%E5%88%9B%E9%80%A0%E8%90%A5/AI%20%E7%BC%96%E7%A8%8B/runapi/frontend/admin/node_modules/vite/dist/node/index.js";
import react from "file:///H:/AI%20%E5%88%9B%E9%80%A0%E8%90%A5/AI%20%E7%BC%96%E7%A8%8B/runapi/frontend/admin/node_modules/@vitejs/plugin-react/dist/index.js";
import tailwindcss from "file:///H:/AI%20%E5%88%9B%E9%80%A0%E8%90%A5/AI%20%E7%BC%96%E7%A8%8B/runapi/frontend/admin/node_modules/@tailwindcss/vite/dist/index.mjs";
import { fileURLToPath, URL } from "node:url";
var __vite_injected_original_import_meta_url = "file:///H:/AI%20%E5%88%9B%E9%80%A0%E8%90%A5/AI%20%E7%BC%96%E7%A8%8B/runapi/frontend/admin/vite.config.ts";
var baseSlashRedirect = (base) => ({
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
  }
});
var vite_config_default = defineConfig(({ command }) => ({
  base: command === "serve" ? "/admin/" : "./",
  plugins: [baseSlashRedirect("/admin/"), react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", __vite_injected_original_import_meta_url)) }
  },
  server: {
    port: 5174,
    proxy: {
      "/admin/api": "http://localhost:9000",
      "/login": "http://localhost:9000",
      "/api/auth": "http://localhost:9000"
    }
  }
}));
export {
  vite_config_default as default
};
//# sourceMappingURL=data:application/json;base64,ewogICJ2ZXJzaW9uIjogMywKICAic291cmNlcyI6IFsidml0ZS5jb25maWcudHMiXSwKICAic291cmNlc0NvbnRlbnQiOiBbImNvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9kaXJuYW1lID0gXCJIOlxcXFxBSSBcdTUyMUJcdTkwMjBcdTg0MjVcXFxcQUkgXHU3RjE2XHU3QTBCXFxcXHJ1bmFwaVxcXFxmcm9udGVuZFxcXFxhZG1pblwiO2NvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9maWxlbmFtZSA9IFwiSDpcXFxcQUkgXHU1MjFCXHU5MDIwXHU4NDI1XFxcXEFJIFx1N0YxNlx1N0EwQlxcXFxydW5hcGlcXFxcZnJvbnRlbmRcXFxcYWRtaW5cXFxcdml0ZS5jb25maWcudHNcIjtjb25zdCBfX3ZpdGVfaW5qZWN0ZWRfb3JpZ2luYWxfaW1wb3J0X21ldGFfdXJsID0gXCJmaWxlOi8vL0g6L0FJJTIwJUU1JTg4JTlCJUU5JTgwJUEwJUU4JTkwJUE1L0FJJTIwJUU3JUJDJTk2JUU3JUE4JThCL3J1bmFwaS9mcm9udGVuZC9hZG1pbi92aXRlLmNvbmZpZy50c1wiO2ltcG9ydCB7IGRlZmluZUNvbmZpZywgdHlwZSBQbHVnaW4gfSBmcm9tIFwidml0ZVwiO1xyXG5pbXBvcnQgcmVhY3QgZnJvbSBcIkB2aXRlanMvcGx1Z2luLXJlYWN0XCI7XHJcbmltcG9ydCB0YWlsd2luZGNzcyBmcm9tIFwiQHRhaWx3aW5kY3NzL3ZpdGVcIjtcclxuaW1wb3J0IHsgZmlsZVVSTFRvUGF0aCwgVVJMIH0gZnJvbSBcIm5vZGU6dXJsXCI7XHJcblxyXG4vLyBcdTUyMzdcdTY1QjAgL2FkbWluKFx1NjVFMFx1N0VEM1x1NUMzRVx1NjU5Q1x1Njc2MClcdTY1RjYgVml0ZSBcdTRGMUEgNDA0IFx1NUU3Nlx1NjNEMFx1NzkzQSBiYXNlIFx1MjAxNFx1MjAxNCBcdTkxQ0RcdTVCOUFcdTU0MTFcdTUyMzAgL2FkbWluL1x1MzAwMlxyXG5jb25zdCBiYXNlU2xhc2hSZWRpcmVjdCA9IChiYXNlOiBzdHJpbmcpOiBQbHVnaW4gPT4gKHtcclxuICBuYW1lOiBcImJhc2Utc2xhc2gtcmVkaXJlY3RcIixcclxuICBjb25maWd1cmVTZXJ2ZXIoc2VydmVyKSB7XHJcbiAgICBjb25zdCBub1NsYXNoID0gYmFzZS5yZXBsYWNlKC9cXC8kLywgXCJcIik7XHJcbiAgICBzZXJ2ZXIubWlkZGxld2FyZXMudXNlKChyZXEsIHJlcywgbmV4dCkgPT4ge1xyXG4gICAgICBjb25zdCBwYXRoID0gKHJlcS51cmwgfHwgXCJcIikuc3BsaXQoXCI/XCIpWzBdO1xyXG4gICAgICBpZiAocGF0aCA9PT0gbm9TbGFzaCkge1xyXG4gICAgICAgIHJlcy53cml0ZUhlYWQoMzAxLCB7IExvY2F0aW9uOiBiYXNlIH0pO1xyXG4gICAgICAgIHJlcy5lbmQoKTtcclxuICAgICAgICByZXR1cm47XHJcbiAgICAgIH1cclxuICAgICAgbmV4dCgpO1xyXG4gICAgfSk7XHJcbiAgfSxcclxufSk7XHJcblxyXG4vLyBkZXYgXHU1NkZBXHU1QjlBIC9hZG1pbi8oXHU2NzJDXHU1NzMwXHU3NkY0XHU1RjAwKTtidWlsZCBcdTc1MjhcdTc2RjhcdTVCRjlcdThERUZcdTVGODQgXHUyMDE0XHUyMDE0IFx1NEVBN1x1NzI2OVx1NTNFRlx1NjMwMlx1NTcyOFx1NEVGQlx1NjEwRlx1NTNDRFx1NEVFM1x1NUI1MFx1OERFRlx1NUY4NFx1NEUwQihcdTU5ODIgaHR0cHM6Ly9ob3N0L3JlbGF5LylcdTMwMDJcclxuZXhwb3J0IGRlZmF1bHQgZGVmaW5lQ29uZmlnKCh7IGNvbW1hbmQgfSkgPT4gKHtcclxuICBiYXNlOiBjb21tYW5kID09PSBcInNlcnZlXCIgPyBcIi9hZG1pbi9cIiA6IFwiLi9cIixcclxuICBwbHVnaW5zOiBbYmFzZVNsYXNoUmVkaXJlY3QoXCIvYWRtaW4vXCIpLCByZWFjdCgpLCB0YWlsd2luZGNzcygpXSxcclxuICByZXNvbHZlOiB7XHJcbiAgICBhbGlhczogeyBcIkBcIjogZmlsZVVSTFRvUGF0aChuZXcgVVJMKFwiLi9zcmNcIiwgaW1wb3J0Lm1ldGEudXJsKSkgfSxcclxuICB9LFxyXG4gIHNlcnZlcjoge1xyXG4gICAgcG9ydDogNTE3NCxcclxuICAgIHByb3h5OiB7XG4gICAgICBcIi9hZG1pbi9hcGlcIjogXCJodHRwOi8vbG9jYWxob3N0OjkwMDBcIixcbiAgICAgIFwiL2xvZ2luXCI6IFwiaHR0cDovL2xvY2FsaG9zdDo5MDAwXCIsXG4gICAgICBcIi9hcGkvYXV0aFwiOiBcImh0dHA6Ly9sb2NhbGhvc3Q6OTAwMFwiLFxuICAgIH0sXHJcbiAgfSxcclxufSkpO1xyXG4iXSwKICAibWFwcGluZ3MiOiAiO0FBQXlWLFNBQVMsb0JBQWlDO0FBQ25ZLE9BQU8sV0FBVztBQUNsQixPQUFPLGlCQUFpQjtBQUN4QixTQUFTLGVBQWUsV0FBVztBQUgwSixJQUFNLDJDQUEyQztBQU05TyxJQUFNLG9CQUFvQixDQUFDLFVBQTBCO0FBQUEsRUFDbkQsTUFBTTtBQUFBLEVBQ04sZ0JBQWdCLFFBQVE7QUFDdEIsVUFBTSxVQUFVLEtBQUssUUFBUSxPQUFPLEVBQUU7QUFDdEMsV0FBTyxZQUFZLElBQUksQ0FBQyxLQUFLLEtBQUssU0FBUztBQUN6QyxZQUFNLFFBQVEsSUFBSSxPQUFPLElBQUksTUFBTSxHQUFHLEVBQUUsQ0FBQztBQUN6QyxVQUFJLFNBQVMsU0FBUztBQUNwQixZQUFJLFVBQVUsS0FBSyxFQUFFLFVBQVUsS0FBSyxDQUFDO0FBQ3JDLFlBQUksSUFBSTtBQUNSO0FBQUEsTUFDRjtBQUNBLFdBQUs7QUFBQSxJQUNQLENBQUM7QUFBQSxFQUNIO0FBQ0Y7QUFHQSxJQUFPLHNCQUFRLGFBQWEsQ0FBQyxFQUFFLFFBQVEsT0FBTztBQUFBLEVBQzVDLE1BQU0sWUFBWSxVQUFVLFlBQVk7QUFBQSxFQUN4QyxTQUFTLENBQUMsa0JBQWtCLFNBQVMsR0FBRyxNQUFNLEdBQUcsWUFBWSxDQUFDO0FBQUEsRUFDOUQsU0FBUztBQUFBLElBQ1AsT0FBTyxFQUFFLEtBQUssY0FBYyxJQUFJLElBQUksU0FBUyx3Q0FBZSxDQUFDLEVBQUU7QUFBQSxFQUNqRTtBQUFBLEVBQ0EsUUFBUTtBQUFBLElBQ04sTUFBTTtBQUFBLElBQ04sT0FBTztBQUFBLE1BQ0wsY0FBYztBQUFBLE1BQ2QsVUFBVTtBQUFBLE1BQ1YsYUFBYTtBQUFBLElBQ2Y7QUFBQSxFQUNGO0FBQ0YsRUFBRTsiLAogICJuYW1lcyI6IFtdCn0K
