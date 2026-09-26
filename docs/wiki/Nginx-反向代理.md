# Nginx 反向代理

对外部署推荐 **TLS 终结在 Nginx + 子路径暴露**，例如：

- 管理后台：`https://sh.example.com:8443/relay/admin/`
- 用户门户：`https://sh.example.com:8443/relay/portal/`
- 数据面 API：`https://sh.example.com:8443/relay/v1/...`

Relay ≥ v0.2.1 对「剥前缀 / 不剥前缀」两种代理写法均兼容；无尾斜杠入口会自动 301 补齐。

## 完整配置（带兜底规则）

以下 `location` 块放入你的 `server { listen 8443 ssl; ... }` 内（同款文件在仓库 `deploy/nginx/relay.example.conf`）：

```nginx
# ---- ① 入口兜底:裸 /relay 与 /relay/ 统一落到管理后台 --------------------
location = /relay  { return 301 /relay/admin/; }
location = /relay/ { return 301 /relay/admin/; }

# ---- ② 主代理:/relay/ 下所有请求转发网关 ---------------------------------
location /relay/ {
    # 尾部带 "/" = 剥离 /relay/ 前缀后转发(当前模式,需 Relay >= v0.2.1)。
    # 备选:去掉尾部 "/" 改为不剥前缀模式(proxy_pass http://127.0.0.1:8081;),二选一。
    proxy_pass http://127.0.0.1:8081/;

    proxy_set_header Host              $http_host;
    proxy_set_header X-Real-IP         $remote_addr;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Port  $server_port;
    proxy_set_header REMOTE-HOST       $remote_addr;

    # ---- WebSocket / 流式(SSE)必需:逐字透传,不缓冲 ----
    proxy_http_version 1.1;
    proxy_set_header Upgrade    $http_upgrade;
    proxy_set_header Connection $http_connection;
    proxy_buffering             off;
    proxy_cache                 off;

    # ---- 长对话兜底:推理模型/长流式响应不被 nginx 掐断(默认 60s 太短) ----
    proxy_connect_timeout 10s;
    proxy_send_timeout    600s;
    proxy_read_timeout    600s;

    # ---- 请求体兜底:多模态图片 base64 动辄数 MB(nginx 默认 1m 会 413) ----
    client_max_body_size 50m;

    proxy_ssl_server_name off;
    proxy_ssl_name $proxy_host;
    add_header X-Cache $upstream_cache_status;
}

# ---- ③ 站点根路径兜底(可选):访问 https://host:8443/ 也进管理后台 --------
# location / { return 301 /relay/admin/; }
```

生效：`nginx -t && nginx -s reload`。

## 两种 proxy_pass 写法的区别

| 写法 | 行为 | 说明 |
|---|---|---|
| `proxy_pass http://127.0.0.1:8081/;`（尾斜杠） | **剥前缀**：`/relay/admin` → 后端收到 `/admin` | 与 `install.sh` 的相对资源引用天然匹配 |
| `proxy_pass http://127.0.0.1:8081;`（无尾斜杠） | **不剥前缀**：`/relay/admin` 原样到后端 | Relay 内置前缀剥离中间件，行为一致 |

两种写法 Relay ≥ v0.2.1 均完整支持（无尾斜杠入口统一 301 补齐）；**任选其一**，不要为同一路径混用多条 location。

## 为什么需要这些兜底

| 规则 | 防什么 |
|---|---|
| `location = /relay`、`= /relay/` | `location /relay/` 匹配不到裸 `/relay`,裸入口会 404 |
| `proxy_buffering off` | SSE 流式对话默认被 nginx 攒缓冲,回复一坨一坨出来甚至长时间空白 |
| `proxy_read/send_timeout 600s` | 默认 60s,推理模型长思考、长流式会被中途掐断(504) |
| `client_max_body_size 50m` | 默认 1m,多模态图片 base64 请求体直接 413 |
| ③ 根路径兜底 | 整站只跑 Relay 时,访问根地址也能进后台 |

## 常见问题

**Q：访问 `/relay/admin`（无尾斜杠）报 `/relay/assets/...` 404？**
A：Relay < v0.2.1 的已知问题——剥前缀代理下入口无重定向，相对资源错位。升级到 v0.2.1 即可（会自动 301 到 `/relay/admin/`）。

**Q：流式对话没有逐字输出？**
A：检查是否漏了 `proxy_buffering off`；另外若中间还有 CDN，需在 CDN 侧同样关闭响应缓冲。

**Q：大图片（视觉模型）请求 413？**
A：`client_max_body_size` 调大（示例 50m），同时注意 nginx `client_body_buffer_size` 保持默认即可。

**Q：想用根路径直接对外（不带 /relay 前缀）？**
A：`location / { proxy_pass http://127.0.0.1:8081; }` 即可，Relay 根路径部署天然支持。
