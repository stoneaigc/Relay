# Relay 多阶段构建:前端(node) → Rust(release) → 运行镜像(bookworm-slim)
# 构建上下文必须为仓库根目录:docker build -t relay .

# ---------- 阶段 1:双前端构建 ----------
FROM node:20-alpine AS web
WORKDIR /web
# 先拷 manifest 单独装依赖,源码变更不触发重装
COPY frontend/admin/package.json frontend/admin/package-lock.json admin/
COPY frontend/portal/package.json frontend/portal/package-lock.json portal/
RUN cd admin && npm ci --no-audit --no-fund \
    && cd ../portal && npm ci --no-audit --no-fund
COPY frontend/admin/ admin/
COPY frontend/portal/ portal/
RUN cd admin && npm run build \
    && cd ../portal && npm run build

# ---------- 阶段 2:Rust release 构建 ----------
FROM rust:1-bookworm AS build
WORKDIR /build
# 依赖缓存层:dummy 源码先把依赖编译进 target,业务代码变更不重编依赖
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "pub fn placeholder() {}" > src/lib.rs \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release --bin relay \
    && rm src/lib.rs src/main.rs
# migrations 必须随源码一起进编译期:storage.rs 用 include_str! 嵌入迁移 SQL
COPY migrations/ migrations/
COPY src/ src/
RUN touch src/lib.rs src/main.rs && cargo build --release --bin relay

# ---------- 阶段 3:运行镜像 ----------
FROM debian:bookworm-slim
# curl 供 HEALTHCHECK 使用;ca-certificates 供出站 HTTPS(供应商上游/邮件)兜底
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /build/target/release/relay /app/relay
COPY config/ /app/config/
COPY --from=web /web/admin/dist /app/frontend/admin/dist
COPY --from=web /web/portal/dist /app/frontend/portal/dist
# 数据目录:默认空库落这里(可用 RELAY_DATABASE__URL 指向他处);以 nobody 运行
RUN mkdir -p /app/data && chown -R nobody:nogroup /app/data
VOLUME /app/data
USER nobody:nogroup
ENV RUST_LOG=info
# 默认库落数据卷:/app 以 root 属主只读,nobody 只能写 /app/data;裸 docker run 不传参也可用
ENV RELAY_DATABASE__URL=sqlite:///app/data/relay.db
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/app/relay"]
