# Relay Wiki · 使用与部署手册

> **LLM API 转发网关**：单进程 Rust 二进制，同时兼容 OpenAI 与 Anthropic 协议，内置多供应商智能路由、故障转移、计费管控、用户门户与运营后台。
>
> 本 Wiki 与仓库 `docs/wiki/` 目录同源。若在 GitHub 上启用了 Wiki，可用一条命令将本目录整体导入：
>
> ```bash
> git clone https://github.com/stoneaigc/Relay.wiki.git relay-wiki \
>   && cp docs/wiki/*.md relay-wiki/ \
>   && cd relay-wiki && git add . && git commit -m "docs: import wiki" && git push
> ```

## 文档目录

| 章节 | 内容 |
|---|---|
| **[安装与升级](https://github.com/stoneaigc/Relay/wiki/安装与升级)** | 在线一键安装（1Panel 风格）、离线安装、Docker Compose、升级与回滚、卸载 |
| **[Nginx 反向代理](https://github.com/stoneaigc/Relay/wiki/Nginx-反向代理)** | 子路径(`/relay/`)对外暴露完整配置、兜底规则、流式(SSE)与 WebSocket 注意事项 |

## 快速导航

- 我还没装，想最快跑起来 → [安装与升级 · 在线安装](安装与升级.md#在线安装推荐)
- 我的服务器不能上外网 → [安装与升级 · 离线安装](安装与升级.md#离线安装)
- 装好了，怎么升级到新版本 → [安装与升级 · 升级](安装与升级.md#升级)
- 想通过 `https://域名/relay/` 这种带前缀的地址对外提供 → [Nginx 反向代理](Nginx-反向代理.md)
- 门户 / 管理后台 / API 分别是什么地址 → [安装与升级 · 安装后你可以访问](安装与升级.md#安装后你可以访问)

## 一图看懂

```
客户端 SDK ──▶ Relay 网关 ──▶ OpenAI / Anthropic / DeepSeek / 通义 / 智谱 / 私有部署 …
门户用户  ──▶ (鉴权 → 路由 → 协议互转 → 故障转移 → 计费 → 链路追踪)
运营后台 ──▶
```

## 相关入口

- 主仓库与 README：<https://github.com/stoneaigc/Relay>
- 版本发布（Release Notes）：<https://github.com/stoneaigc/Relay/releases>
- 问题反馈：<https://github.com/stoneaigc/Relay/issues>
