import { useState } from "react";
import { BookOpen, Check, Copy, KeyRound, ShieldCheck, TerminalSquare } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

const BASE = "http://localhost:8080";

const OPENAI_CURL = `curl ${BASE}/v1/chat/completions \\
  -H "Authorization: Bearer rk_live_xxx" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "你好，介绍一下你自己"}]
  }'`;

const OPENAI_PY = `from openai import OpenAI

client = OpenAI(
    base_url="${BASE}/v1",
    api_key="rk_live_xxx",
)

resp = client.chat.completions.create(
    model="chat",
    messages=[{"role": "user", "content": "你好，介绍一下你自己"}],
)
print(resp.choices[0].message.content)`;

const OPENAI_NODE = `import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "${BASE}/v1",
  apiKey: "rk_live_xxx",
});

const resp = await client.chat.completions.create({
  model: "chat",
  messages: [{ role: "user", content: "你好，介绍一下你自己" }],
});
console.log(resp.choices[0].message.content);`;

const ANTHROPIC_CURL = `curl ${BASE}/v1/messages \\
  -H "x-api-key: rk_live_xxx" \\
  -H "anthropic-version: 2023-06-01" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "chat",
    "max_tokens": 256,
    "messages": [{"role": "user", "content": "你好，介绍一下你自己"}]
  }'`;

const ANTHROPIC_PY = `import anthropic

client = anthropic.Anthropic(
    base_url="${BASE}",
    api_key="rk_live_xxx",
)

msg = client.messages.create(
    model="chat",
    max_tokens=256,
    messages=[{"role": "user", "content": "你好，介绍一下你自己"}],
)
print(msg.content[0].text)`;

const CHAT_RESP = `{
  "id": "chatcmpl-a1b2c3",
  "object": "chat.completion",
  "model": "chat",
  "choices": [{
    "index": 0,
    "message": { "role": "assistant", "content": "……" },
    "finish_reason": "stop"
  }],
  "usage": { "prompt_tokens": 12, "completion_tokens": 98, "total_tokens": 110 }
}`;

const MODELS_RESP = `{
  "object": "list",
  "data": [
    { "id": "chat", "object": "model", "owned_by": "relay" },
    { "id": "economy", "object": "model", "owned_by": "relay" },
    { "id": "reasoning", "object": "model", "owned_by": "relay" }
  ]
}`;

const STREAM_PY = `stream = client.chat.completions.create(
    model="chat",
    messages=[{"role": "user", "content": "写一首七言绝句"}],
    stream=True,
)
for chunk in stream:
    if chunk.choices and chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)`;

const STREAM_NODE = `const stream = await client.chat.completions.create({
  model: "chat",
  messages: [{ role: "user", content: "写一首七言绝句" }],
  stream: true,
});
for await (const chunk of stream) {
  const delta = chunk.choices[0]?.delta?.content;
  if (delta) process.stdout.write(delta);
}`;

const STREAM_SSE = `# OpenAI 协议(SSE):每行 data: <json>,以 [DONE] 结束
data: {"choices":[{"delta":{"content":"好"}}]}

data: {"choices":[{"delta":{"content":"的"}}]}

data: [DONE]

# Anthropic 协议(SSE):event 声明类型,配合 message_start / content_block_delta /
# message_stop 等事件,与官方语义一致
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"好的"}}`;

const ERR_OPENAI = `HTTP/1.1 401 Unauthorized

{
  "error": {
    "message": "api key is invalid or revoked",
    "type": "authentication_error"
  }
}`;

const ERR_ANTHROPIC = `HTTP/1.1 404 Not Found

{
  "type": "error",
  "error": {
    "type": "not_found_error",
    "message": "model \`foo\` not found in routing table"
  }
}`;

const PROM_CONFIG = `# prometheus.yml
scrape_configs:
  - job_name: "relay"
    metrics_path: "/metrics"
    scrape_interval: 15s
    scheme: http
    static_configs:
      - targets: ["relay.example.com:8080"]
    authorization:
      type: Bearer
      credentials: "<metrics.export_token>"`;

function CopyBtn({ code }: { code: string }) {
  const [ok, setOk] = useState(false);
  return (
    <button
      title="复制"
      onClick={async () => {
        try { await navigator.clipboard.writeText(code); } catch { /* 忽略 */ }
        setOk(true);
        setTimeout(() => setOk(false), 1500);
      }}
      className="rounded-md p-1.5 text-zinc-400 transition-colors hover:bg-white/10 hover:text-white"
    >
      {ok ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
    </button>
  );
}

function CodeBlock({ title, code }: { title: string; code: string }) {
  return (
    <div className="overflow-hidden rounded-lg border border-zinc-800 bg-zinc-950">
      <div className="flex items-center justify-between border-b border-white/10 px-3 py-1.5">
        <span className="font-mono text-[11px] uppercase tracking-wide text-zinc-400">{title}</span>
        <CopyBtn code={code} />
      </div>
      <pre className="overflow-x-auto p-4 font-mono text-[12.5px] leading-relaxed text-zinc-100">{code}</pre>
    </div>
  );
}

function LangTabs({ tabs }: { tabs: { label: string; code: string }[] }) {
  const [active, setActive] = useState(0);
  const cur = tabs[Math.min(active, tabs.length - 1)];
  return (
    <div className="overflow-hidden rounded-lg border border-zinc-800 bg-zinc-950">
      <div className="flex items-center justify-between border-b border-white/10 px-2 py-1.5">
        <div className="flex gap-1">
          {tabs.map((t, i) => (
            <button
              key={t.label}
              onClick={() => setActive(i)}
              className={cn(
                "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                i === active ? "bg-white/15 text-white" : "text-zinc-400 hover:text-zinc-200"
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
        <CopyBtn code={cur.code} />
      </div>
      <pre className="overflow-x-auto p-4 font-mono text-[12.5px] leading-relaxed text-zinc-100">{cur.code}</pre>
    </div>
  );
}

function K({ children }: { children: React.ReactNode }) {
  return <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[12px] text-foreground">{children}</code>;
}

function MethodBadge({ m }: { m: string }) {
  return (
    <Badge variant={m === "GET" ? "muted" : "default"} className="font-mono text-[11px]">
      {m}
    </Badge>
  );
}

const NAV_SECTIONS = [
  { id: "quickstart", label: "快速开始" },
  { id: "auth", label: "鉴权" },
  { id: "protocols", label: "协议端点" },
  { id: "streaming", label: "流式" },
  { id: "errors", label: "错误码" },
  { id: "metrics", label: "Prometheus 指标" },
];

function Section({ id, title, desc, children }: { id: string; title: string; desc: string; children: React.ReactNode }) {
  return (
    <section id={id} className="scroll-mt-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{title}</CardTitle>
          <CardDescription>{desc}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">{children}</CardContent>
      </Card>
    </section>
  );
}

export default function ApiDocsPanel() {
  return (
    <div className="mx-auto max-w-4xl space-y-5">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="flex items-center gap-2 text-lg font-bold">
          <BookOpen className="h-5 w-5 text-primary" /> API 文档
        </h1>
        <nav className="flex flex-wrap gap-1.5">
          {NAV_SECTIONS.map((s) => (
            <a key={s.id} href={`#${s.id}`}
              className="rounded-full border bg-background px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
              {s.label}
            </a>
          ))}
        </nav>
      </div>

      <Section id="quickstart" title="快速开始" desc="三步接入:门户创建 API Key → 把 Base URL 指向网关 → 像调用官方 API 一样发请求。">
        <div className="flex flex-wrap items-center gap-2 rounded-lg border bg-muted/40 px-3 py-2 text-sm">
          <TerminalSquare className="h-4 w-4 text-muted-foreground" />
          <span className="text-muted-foreground">Base URL</span>
          <K>{BASE}</K>
          <span className="text-xs text-muted-foreground">（OpenAI SDK 追加 <K>/v1</K>，Anthropic SDK 直接使用根地址）</span>
        </div>
        <LangTabs tabs={[
          { label: "curl", code: OPENAI_CURL },
          { label: "Python", code: OPENAI_PY },
          { label: "Node", code: OPENAI_NODE },
        ]} />
        <div className="text-sm font-medium text-muted-foreground">Anthropic 协议同样可用:</div>
        <LangTabs tabs={[
          { label: "curl", code: ANTHROPIC_CURL },
          { label: "Python", code: ANTHROPIC_PY },
        ]} />
        <p className="text-sm text-muted-foreground">
          <K>model</K> 填管理后台「模型组」里的<b>对外模型名</b>;同名多条路由按权重 / 优先级自动分流,上游是 OpenAI 还是 Anthropic 由网关透明互转。
        </p>
      </Section>

      <Section id="auth" title="鉴权" desc="数据面 Key 双协议兼容:任选其一,网关自动识别。">
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-lg border p-3">
            <div className="flex items-center gap-2 text-sm font-medium"><KeyRound className="h-4 w-4 text-primary" /> OpenAI 客户端(默认)</div>
            <div className="mt-2"><K>Authorization: Bearer rk_live_xxx</K></div>
          </div>
          <div className="rounded-lg border p-3">
            <div className="flex items-center gap-2 text-sm font-medium"><KeyRound className="h-4 w-4 text-primary" /> Anthropic 客户端(默认)</div>
            <div className="mt-2"><K>x-api-key: rk_live_xxx</K></div>
          </div>
        </div>
        <ul className="list-disc space-y-1 pl-5 text-sm text-muted-foreground">
          <li>Key 在门户「API Keys」创建,明文<b>仅创建时展示一次</b>,请立即保存。</li>
          <li>Key 校验走内存热路径(哈希直查,不查库),吊销 / 刷新即时生效。</li>
          <li>鉴权失败返回 <K>401 authentication_error</K>;账号被禁用返回 <K>403 permission_error</K>。</li>
        </ul>
        <CodeBlock title="401 示例" code={ERR_OPENAI} />
      </Section>

      <Section id="protocols" title="协议端点" desc="OpenAI 与 Anthropic 双协议入站,流式 / 非流式、工具调用、多模态图片均支持。">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-20">方法</TableHead>
              <TableHead>路径</TableHead>
              <TableHead>说明</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TableCell><MethodBadge m="GET" /></TableCell>
              <TableCell><K>/v1/models</K></TableCell>
              <TableCell className="text-muted-foreground">当前 Key 可用的对外模型名列表</TableCell>
            </TableRow>
            <TableRow>
              <TableCell><MethodBadge m="POST" /></TableCell>
              <TableCell><K>/v1/chat/completions</K></TableCell>
              <TableCell className="text-muted-foreground">OpenAI 协议对话补全(流式传 <K>"stream": true</K>)</TableCell>
            </TableRow>
            <TableRow>
              <TableCell><MethodBadge m="POST" /></TableCell>
              <TableCell><K>/v1/messages</K></TableCell>
              <TableCell className="text-muted-foreground">Anthropic 协议消息(<K>max_tokens</K> 必填;SDK 自动带 <K>anthropic-version</K>)</TableCell>
            </TableRow>
            <TableRow>
              <TableCell><MethodBadge m="GET" /></TableCell>
              <TableCell><K>/healthz</K></TableCell>
              <TableCell className="text-muted-foreground">健康检查(无需鉴权)</TableCell>
            </TableRow>
          </TableBody>
        </Table>
        <CodeBlock title="GET /v1/models 响应" code={MODELS_RESP} />
        <CodeBlock title="POST /v1/chat/completions 响应(非流式)" code={CHAT_RESP} />
        <p className="text-sm text-muted-foreground">
          计费按上游实际返回的 token 用量结算,叠加模型级与用户级倍率;用量明细可在门户「用量明细」实时查看。
        </p>
      </Section>

      <Section id="streaming" title="流式" desc="标准 SSE:请求体加 stream: true,SDK 原生流式接口直接可用。">
        <LangTabs tabs={[
          { label: "Python", code: STREAM_PY },
          { label: "Node", code: STREAM_NODE },
        ]} />
        <CodeBlock title="SSE 线格式" code={STREAM_SSE} />
        <p className="text-sm text-muted-foreground">
          网关在上游与客户端协议不一致时(如 OpenAI 客户端 → Anthropic 上游)逐 chunk 回译,流式语义与官方一致,无需感知上游差异。
        </p>
      </Section>

      <Section id="errors" title="错误码" desc="错误体遵循客户端协议:OpenAI 入口返回 OpenAI 格式,Anthropic 入口返回 Anthropic 格式。">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-16">状态码</TableHead>
              <TableHead>type</TableHead>
              <TableHead>含义</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {[
              ["401", "authentication_error", "缺少鉴权头 / Key 无效或已吊销"],
              ["403", "permission_error", "账号已被禁用"],
              ["402", "insufficient_quota", "token 余额不足"],
              ["404", "not_found_error", "模型不在路由表,或对当前 Key 不可用"],
              ["429", "rate_limit_error", "并发超限"],
              ["429", "insufficient_quota", "日 / 月 token 预算耗尽"],
              ["502", "upstream_error", "无健康上游目标,或上游返回错误(返回前已自动尝试全部候选)"],
              ["503", "upstream_error", "上游暂时不可用(超时 / 连接失败 / 5xx,failover 后仍失败)"],
              ["400", "invalid_request_error", "请求体不合法"],
              ["500", "internal_error", "网关内部错误"],
            ].map(([code, type, desc]) => (
              <TableRow key={code + type}>
                <TableCell className="font-mono text-xs font-semibold">{code}</TableCell>
                <TableCell><K>{type}</K></TableCell>
                <TableCell className="text-muted-foreground">{desc}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <div className="grid gap-3 md:grid-cols-2">
          <CodeBlock title="OpenAI 格式" code={ERR_OPENAI} />
          <CodeBlock title="Anthropic 格式" code={ERR_ANTHROPIC} />
        </div>
        <p className="flex items-start gap-2 text-sm text-muted-foreground">
          <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
          <span>
            <K>502 / 503</K> 返回前网关已按负载策略完成 failover(连接失败 / 超时 / 429 / 5xx 自动切换下一候选,连续失败触发熔断跳过);完整尝试链可在管理后台「请求链路」按 request_id 查看。
          </span>
        </p>
      </Section>

      <Section id="metrics" title="Prometheus 指标" desc="GET /metrics 输出标准 exposition 文本,窗口为内存滚动聚合(默认 48h,进程重启清零)。">
        <div className="space-y-1 text-sm">
          <div className="flex flex-wrap items-center gap-2">
            <MethodBadge m="GET" /><K>/metrics</K>
            <span className="text-muted-foreground">响应 <K>text/plain; version=0.0.4</K></span>
          </div>
          <p className="text-muted-foreground">
            鉴权:管理端 JWT,或静态令牌 <K>Authorization: Bearer &lt;metrics.export_token&gt;</K>。令牌在 <K>config/default.toml</K> 的 <K>[metrics] export_token</K> 配置(或环境变量 <K>RELAY_METRICS__EXPORT_TOKEN</K>);留空时仅管理端 JWT 可访问。
          </p>
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>指标</TableHead>
              <TableHead>类型</TableHead>
              <TableHead>说明</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {[
              ["relay_upstream_requests_total", "counter", "数据面请求数;label scope=global/provider、status=success|fail_unavailable|fail_other"],
              ["relay_upstream_request_duration_ms", "summary", "上游成功请求延迟(ms);quantile 0.95 / 0.99 + _sum/_count"],
              ["relay_upstream_success_ratio", "gauge", "上游窗口内成功率"],
              ["relay_upstream_breaker_state", "gauge", "熔断状态:1=熔断中,0=正常"],
              ["relay_upstream_breaker_fail_count", "gauge", "熔断器当前窗口失败计数"],
              ["relay_tokens_input_total", "counter", "输入 token 总量(含失败请求已消耗部分)"],
              ["relay_tokens_output_total", "counter", "输出 token 总量"],
              ["relay_cache_requests_total", "counter", "语义缓存请求;label result=hit|miss"],
              ["relay_users_total", "gauge", "内存活跃用户会话数"],
              ["relay_metrics_window_secs", "gauge", "聚合窗口长度(秒)"],
            ].map(([name, type, desc]) => (
              <TableRow key={name}>
                <TableCell><K>{name}</K></TableCell>
                <TableCell className="text-xs text-muted-foreground">{type}</TableCell>
                <TableCell className="text-muted-foreground">{desc}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <CodeBlock title="Prometheus 抓取配置" code={PROM_CONFIG} />
      </Section>
    </div>
  );
}
