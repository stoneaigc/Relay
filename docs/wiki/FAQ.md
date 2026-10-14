# FAQ

## 常见问题

**如何新增 / 更换供应商？**
管理后台「上游模型」页添加上游（自带连接测试），在「模型组」里调整路由即可，调用方代码零改动。

**语义缓存为什么默认关？**
相似度命中存在轻微误差风险，因此按路由 opt-in，且仅缓存确定性请求（temperature=0 或带 seed）；确认业务可接受后再开启。

**需要多实例 / 更高并发怎么办？**
多个 Relay 实例共享 PostgreSQL（数据）与 Redis（缓存）即可横向扩展；链路日志建议切换 Elasticsearch 后端。

**用户的可见模型是怎么隔离的？**
用户在后台绑定模型组，`/v1/models` 只返回该组内的对外模型名；组外模型对用户完全不可见（调用会被拒）。

**通过 `https://域名/relay/` 这种子路径访问出现 404？**
前端资源走相对路径，配合 Nginx 去前缀转发即可，服务端已做兜底重定向。完整配置见 [Nginx 反向代理](https://github.com/stoneaigc/Relay/wiki/Nginx-反向代理)。

**离线安装后管理员密码是多少？**
默认 `admin / Relay@123`（JWT 密钥也是开发默认值）——生产环境务必在启动前编辑 `/opt/relay/relay.env` 覆盖；在线安装会自动生成强密码。

**升级会丢数据吗？**
不会。`install.sh` / `upgrade.sh` 对已存在的 `config/default.toml`、`relay.env` 与数据库文件原样保留；升级前还会自动备份二进制，可随时回滚。

**怎么接入 Prometheus？**
`GET /metrics` 输出 Prometheus 文本格式，用管理端 JWT 或 `metrics.export_token` 鉴权，直接配到 Prometheus scrape 即可。

**请求失败了怎么排查？**
管理后台「请求链路」展开该请求：候选顺序、实际尝试链（每一跳的状态/延迟/错误、熔断跳过标记）全部留痕，失败请求标红。

## 日志查看与端口修改（运维速查）

**看日志**（systemd 部署）：

```bash
sudo systemctl status relay                    # 状态 + 最后几行
sudo journalctl -u relay -n 100 --no-pager     # 最近 100 行
sudo journalctl -u relay -f                    # 实时滚动
sudo journalctl -u relay --since "10 min ago"  # 按时间窗
```

服务配了 `Restart=always, RestartSec=3`：进程挂掉会每 3 秒自动重启，日志会刷屏。排查启动失败时先 `sudo systemctl stop relay` 止血，看清报错改完再 `start`。**成功启动的标志**是最后一行 `Relay vX.Y.Z listening on <地址:端口>`，其前有 routing / loaded users 等自检日志。

**改端口等配置**：配置在启动时读取（`config/default.toml` + `RELAY_*` 环境变量覆盖），改完必须 `sudo systemctl restart relay`：

```bash
# 推荐:写 relay.env(变量名双层下划线,写单下划线不生效)
echo 'RELAY_SERVER__BIND=0.0.0.0:9000' | sudo tee -a /opt/relay/relay.env
sudo systemctl restart relay
```

**确认生效**：启动日志最后一行 `listening on` 打的就是实际生效的绑定地址；再用 `curl http://127.0.0.1:9000/healthz` 与 `ss -tlnp | grep relay` 复核，防火墙/安全组同步放行。

## 不适合的场景（诚实边界）

- 需要**多租户 / 组织架构 / RBAC / SSO** 的平台型 SaaS——产品形态固定为「一个运营者 + N 个用户」两级，刻意不做团队体系。
- **超大规模流量**——单实例定位；多实例可共享 PostgreSQL / Redis，但未做大规模并发压测与专项调优。
- 需要**Key 级模型白名单**的精细化管控——「用户绑定模型组」已覆盖隔离需求，Key 级粒度判定不做。

## 安全披露

发现安全漏洞请勿公开 Issue，优先通过 GitHub 私密安全通告（Security Advisories）或私信联系维护者报告，确认后尽快修复并在 Release 中致谢。
