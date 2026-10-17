import { useEffect, useRef, useState, ReactNode } from "react";
import { BrowserRouter, NavLink, useLocation, useNavigate } from "react-router-dom";
import {
  KeyRound, Copy, RefreshCw, Plus, LogOut, Check,
  LayoutDashboard, Receipt, MessageSquare, Menu, X, SendHorizontal, Boxes,
  Gift, Star, Code2, Clock, CheckCircle2, XCircle, ExternalLink, ImagePlus, Lightbulb, BookOpen,
  Gauge, Zap, Timer, CalendarDays, KeySquare, Eye, EyeOff, ShieldCheck, Sparkles, Square, Trash2,
} from "lucide-react";
import { api, getToken, clearToken, healthz, SITE_PREFIX, KeyInfo, ModelCard, QuotaInfo, chatStream, ChatMsg, RewardInfo, RewardClaim, RewardTask, EvidenceType } from "./api";
import { copyText } from "./clipboard";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";
import EChart, { cssVar } from "@/components/EChart";
import type { EChartsCoreOption } from "echarts/core";
import PaginationBar from "./PaginationBar";
import DocsView from "./ApiDocs";

const GRANT = 10_000_000;

// 子路径反代兼容:部署在 https://host/relay/portal/ 这类前缀下时,路由 basename 探测为 "/relay/portal"。
const APP_BASE = (() => {
  const m = window.location.pathname.match(/^(.*)\/portal(?=\/|$)/);
  return (m?.[1] ?? "") + "/portal";
})();

const fmtDay = (ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; };

/** 复制到剪贴板,并给出 ~1.4s 的“已复制”反馈状态;复制失败(HTTP 环境异常等)不误报。 */
function useCopy(): [boolean, (t: string) => void] {
  const [copied, setCopied] = useState(false);
  const copy = (t: string) => {
    void copyText(t).then((ok) => {
      if (!ok) return;
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    });
  };
  return [copied, copy];
}

/** 值旁边的复制图标按钮:点击后短暂变成品牌色对勾。 */
function CopyButton({ value, label = "复制" }: { value: string; label?: string }) {
  const [copied, copy] = useCopy();
  return (
    <button type="button" onClick={() => copy(value)} title={label} aria-label={label}
      className="shrink-0 rounded-md border border-border bg-background p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
      {copied ? <Check className="h-3.5 w-3.5 text-primary" /> : <Copy className="h-3.5 w-3.5" />}
    </button>
  );
}

/** 模型广场卡片:点击查看详情;悬浮复制图标直接复制模型名。 */
function ModelCardView({ m, onDetail }: { m: ModelCard; onDetail: (m: ModelCard) => void }) {
  const [copied, copy] = useCopy();
  const ctx = m.context_length != null && m.context_length > 0
    ? m.context_length >= 1_000_000 ? `${(m.context_length / 1_000_000).toFixed(m.context_length % 1_000_000 ? 1 : 0)}M` : `${Math.round(m.context_length / 1000)}K`
    : null;
  const price = (m.input_price != null && m.output_price != null)
    ? `$${m.input_price.toFixed(2)} / $${m.output_price.toFixed(2)}`
    : null;
  return (
    <button type="button" onClick={() => onDetail(m)} title="点击查看模型详情"
      className={cn(
        "group flex flex-col gap-2 rounded-xl border p-3.5 text-left transition-all",
        copied
          ? "border-primary/50 bg-primary/5"
          : "border-border bg-card hover:border-primary/40 hover:shadow-sm"
      )}>
      <div className="flex items-center gap-2">
        <span className="mono truncate text-sm font-medium" title={m.name}>{m.name}</span>
        {copied
          ? <span className="ml-auto inline-flex shrink-0 items-center gap-1 text-xs text-primary"><Check className="h-3.5 w-3.5" />已复制</span>
          : <Copy className="ml-auto h-3.5 w-3.5 shrink-0 opacity-0 transition-opacity group-hover:opacity-40" onClick={(e) => { e.stopPropagation(); copy(m.name); }} />}
      </div>
      {(m.labels.length > 0 || m.tags.length > 0) && (
        <div className="flex flex-wrap gap-1">
          {m.labels.map((l) => <Badge key={l}>{l}</Badge>)}
          {m.tags.map((t) => <Badge key={t} variant="muted">{t}</Badge>)}
        </div>
      )}
      <div className="mt-auto grid grid-cols-2 gap-x-3 gap-y-1 border-t pt-2.5 text-xs text-muted-foreground">
        <span className="flex items-center gap-1" title="上下文窗口">上下文<span className="mono text-foreground">{ctx ?? "—"}</span></span>
        <span className="flex items-center gap-1" title="该模型路由的计费倍率,实际单价 = 模型单价 × 倍率">倍率<span className="mono text-foreground">×{m.multiplier}</span></span>
        <span className="flex items-center gap-1" title="每 1M tokens 模型单价(未含倍率)">价格<span className="mono truncate text-foreground" title={price != null ? `${price} $/1M tokens(未含倍率)` : "未定价"}>{price ?? "未定价"}</span></span>
        <span className="flex items-center gap-1" title="该模型聚合的上游部署数">上游<span className="mono text-foreground">{m.upstreams}</span></span>
        {m.cache && <span className="col-span-2 inline-flex items-center gap-1 text-success" title="支持提示词缓存,命中按折扣计费"><Star className="h-3 w-3" />支持上下文缓存</span>}
      </div>
    </button>
  );
}

/// 模型详情弹窗:上下文/倍率/单价/上游部署数/缓存 + 协议兼容入口 + 复制模型名。
function ModelDetailDialog({ m, onClose }: { m: ModelCard | null; onClose: () => void }) {
  const navigate = useNavigate();
  const ctx = m?.context_length != null && m.context_length > 0
    ? (m.context_length >= 1_000_000
        ? `${(m.context_length / 1_000_000).toFixed(m.context_length % 1_000_000 ? 1 : 0)}M`
        : `${Math.round(m.context_length / 1000)}K`) + " tokens"
    : "—";
  // 网关内置协议互转:按模型能力提供入口——对话/多模态模型走三大对话端点,向量模型走 Embeddings。
  const isEmbedding = m ? m.tags.some((t) => t.toLowerCase().includes("向量") || t.toLowerCase().includes("embedding")) : false;
  const protos: [string, string][] = isEmbedding
    ? [["/v1/embeddings", "向量 Embeddings(输入文本,返回向量)"]]
    : [
        ["/v1/chat/completions", "OpenAI 对话补全"],
        ["/v1/responses", "OpenAI Responses"],
        ["/v1/messages", "Anthropic Messages"],
      ];
  // 实际计费单价 = 模型单价 × 路由倍率。
  const effIn = m?.input_price != null ? m.input_price * m.multiplier : null;
  const effOut = m?.output_price != null ? m.output_price * m.multiplier : null;
  return (
    <Dialog open={!!m} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span className="mono">{m?.name}</span>
            {m && <CopyButton value={m.name} label="复制模型名" />}
          </DialogTitle>
          <DialogDescription>按模型能力提供兼容的调用入口，网关自动完成协议互转；实际计费以下方「实际计费单价」为准。</DialogDescription>
        </DialogHeader>
        {m && (
          <div className="space-y-4">
            {(m.labels.length > 0 || m.tags.length > 0) && (
              <div className="flex flex-wrap gap-1">
                {m.labels.map((l) => <Badge key={l}>{l}</Badge>)}
                {m.tags.map((t) => <Badge key={t} variant="muted">{t}</Badge>)}
              </div>
            )}
            <div className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
              <div><div className="text-xs text-muted-foreground">上下文窗口</div><div className="mono">{ctx}</div></div>
              <div><div className="text-xs text-muted-foreground">计费倍率</div><div className="mono">×{m.multiplier}</div></div>
              <div><div className="text-xs text-muted-foreground">模型单价(输入/输出)</div><div className="mono">{m.input_price != null && m.output_price != null ? `$${m.input_price.toFixed(2)} / $${m.output_price.toFixed(2)} · 1M tokens` : "未定价"}</div></div>
              <div><div className="text-xs text-muted-foreground">上游部署数</div><div className="mono">{m.upstreams}</div></div>
              <div><div className="text-xs text-muted-foreground">上下文缓存</div><div>{m.cache ? <span className="text-success">支持(命中按折扣)</span> : <span className="text-muted-foreground">不支持</span>}</div></div>
            </div>
            <div className="rounded-lg bg-muted/40 px-3 py-2 text-xs leading-5">
              {effIn != null && effOut != null ? (
                <>实际计费单价（= 模型单价 × 倍率）：输入 <b className="mono">${effIn.toFixed(2)}</b> / 输出 <b className="mono">${effOut.toFixed(2)}</b> 每 1M tokens</>
              ) : (
                <>该模型未定价，调用按 $0 计。</>
              )}
            </div>
            <div>
              <div className="mb-1 text-xs font-medium text-muted-foreground">兼容调用入口</div>
              <div className="space-y-1">
                {protos.map(([p, d]) => (
                  <div key={p} className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1 text-xs">
                    <span className="mono">{p}</span>
                    <span className="text-muted-foreground">{d}</span>
                  </div>
                ))}
              </div>
            </div>
            <Button variant="outline" className="w-full" onClick={() => { onClose(); navigate("/docs"); }}>
              <BookOpen className="h-4 w-4" /> 查看调用示例(API 文档)
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

/** 模型广场独立页:顶部概要与能力标签筛选,下方为模型卡片网格(点击查看详情)。 */
function ModelsView({ models }: { models: ModelCard[] }) {
  const [tag, setTag] = useState<string | null>(null);
  const [detail, setDetail] = useState<ModelCard | null>(null);
  const allTags: string[] = [];
  for (const m of models) {
    for (const t of m.tags) {
      if (!allTags.some((x) => x.toLowerCase() === t.toLowerCase())) allTags.push(t);
    }
  }
  const shown = tag ? models.filter((m) => m.tags.some((t) => t.toLowerCase() === tag.toLowerCase())) : models;
  const cacheCount = models.filter((m) => m.cache).length;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-muted-foreground">
          共 <span className="mono font-medium text-foreground">{models.length}</span> 个可用模型
          {cacheCount > 0 && <> · <span className="mono font-medium text-foreground">{cacheCount}</span> 个支持缓存</>}
        </span>
        {allTags.length > 0 && (
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <button onClick={() => setTag(null)}
              className={cn("rounded-full border px-3 py-1 text-xs transition-colors",
                tag === null ? "border-primary/40 bg-primary/15 text-primary" : "border-border text-muted-foreground hover:bg-accent")}>
              全部
            </button>
            {allTags.map((t) => (
              <button key={t} onClick={() => setTag(tag === t ? null : t)}
                className={cn("rounded-full border px-3 py-1 text-xs transition-colors",
                  tag === t ? "border-primary/40 bg-primary/15 text-primary" : "border-border text-muted-foreground hover:bg-accent")}>
                {t}
              </button>
            ))}
          </div>
        )}
      </div>
      {models.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center text-sm text-muted-foreground">
            暂无可用模型,请联系管理员为你分配模型组。
          </CardContent>
        </Card>
      ) : shown.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center text-sm text-muted-foreground">
            没有「{tag}」标签的模型。
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">
          {shown.map((m) => <ModelCardView key={m.name} m={m} onDetail={setDetail} />)}
        </div>
      )}
      <p className="text-xs text-muted-foreground">点击卡片查看模型详情与兼容的调用入口;悬浮复制图标可直接复制模型名。</p>
      <ModelDetailDialog m={detail} onClose={() => setDetail(null)} />
    </div>
  );
}

function LineChart({ points }: { points: { ts: number; tokens: number; calls: number }[] }) {
  const border = cssVar("--color-border", "#e4e4e7");
  const muted = cssVar("--color-muted-foreground", "#71717a");
  const fmtDay2 = (ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; };
  const compact = (v: number) => (v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1_000 ? `${(v / 1_000).toFixed(0)}K` : String(v));
  const option: EChartsCoreOption = {
    grid: { left: 8, right: 8, top: 30, bottom: 0, containLabel: true },
    tooltip: {
      trigger: "axis",
      confine: true,
      formatter: (params: any) => {
        const list = Array.isArray(params) ? params : [params];
        const rows = list.map((p: any) => `${p.marker} ${p.seriesName} <b>${Number(p.value).toLocaleString()}</b>`);
        return `${list[0]?.axisValue ?? ""}<br/>${rows.join("<br/>")}`;
      },
    },
    xAxis: {
      type: "category", data: points.map((p) => fmtDay2(p.ts)), boundaryGap: true,
      axisTick: { show: false }, axisLine: { lineStyle: { color: border } },
      axisLabel: { color: muted, fontSize: 11 },
    },
    yAxis: [
      { type: "value", splitLine: { lineStyle: { type: "dashed", color: border } }, axisLabel: { color: muted, fontSize: 11, formatter: compact } },
      { type: "value", show: false },
    ],
    series: [
      // 柱色用靛紫(数据可视化强调色),不用近黑的主色,避免整页灰黑观感。
      { name: "tokens", type: "bar", data: points.map((p) => p.tokens), barMaxWidth: 18, itemStyle: { color: "#6366f1", borderRadius: [3, 3, 0, 0] } },
      { name: "调用次数", type: "line", yAxisIndex: 1, data: points.map((p) => p.calls), smooth: true, symbol: "circle", symbolSize: 5, itemStyle: { color: "#f59e0b" }, lineStyle: { width: 2, color: "#f59e0b" } },
    ],
  };
  return <EChart option={option} height={190} />;
}

// token 数紧凑格式(预算条用):1.2M / 45K / 800。
const compactTokens = (n: number) =>
  n >= 1_000_000 ? `${(n / 1_000_000).toFixed(n % 1_000_000 ? 1 : 0)}M` : n >= 1_000 ? `${(n / 1_000).toFixed(0)}K` : String(n);

/** 我的套餐卡:品牌渐变底,展示套餐名 + 余额 + 消耗进度。 */
function PlanHero({ quota, summary }: { quota: QuotaInfo | null; summary: { granted: number; used: number; balance: number } | null }) {
  const pct = summary && summary.granted > 0 ? Math.max(0, Math.min(100, Math.round((summary.used / summary.granted) * 100))) : 0;
  const lim = (v: number | null | undefined) => (v == null ? "不限" : v.toLocaleString());
  return (
    <div className="brand-gradient relative overflow-hidden rounded-2xl p-5 text-white shadow-sm md:p-6">
      <div className="pointer-events-none absolute -right-10 -top-14 h-44 w-44 rounded-full bg-white/10" />
      <div className="pointer-events-none absolute -bottom-16 right-24 h-36 w-36 rounded-full bg-white/10" />
      <div className="relative flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-1.5 text-xs font-medium text-white/80">
            <Sparkles className="h-3.5 w-3.5" /> 当前套餐
          </div>
          <div className="mt-1 text-2xl font-bold tracking-tight md:text-3xl">{quota?.plan?.name ?? "默认套餐"}</div>
          <p className="mt-1.5 text-xs text-white/75">
            并发 {lim(quota?.concurrency.limit)} · RPM {lim(quota?.rpm.limit)} · TPM {lim(quota?.tpm.limit)}
          </p>
        </div>
        <div className="text-right">
          <div className="text-xs text-white/80">余额(tokens)</div>
          <div className="mono text-3xl font-bold tracking-tight md:text-4xl">{summary ? summary.balance.toLocaleString() : "…"}</div>
        </div>
      </div>
      <div className="relative mt-4">
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/25">
          <div className="h-full rounded-full bg-white/90 transition-all" style={{ width: `${pct}%` }} />
        </div>
        <p className="mt-1.5 text-xs text-white/75">
          已用 {summary ? summary.used.toLocaleString() : "…"} / 总量 {summary ? summary.granted.toLocaleString() : "…"}({pct}%)
        </p>
      </div>
    </div>
  );
}

/** 限额格:图标 + 名称 + 已用/上限 + 进度条(limit=null 视为不限,不画进度)。 */
function QuotaCell({ icon: Icon, label, limit, used, fmt }: {
  icon: any; label: string; limit: number | null; used: number; fmt?: (n: number) => string;
}) {
  const unlimited = limit == null;
  const pct = !unlimited && limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;
  const danger = !unlimited && pct >= 90;
  const f = fmt ?? ((n: number) => n.toLocaleString());
  return (
    <div className="rounded-xl border bg-card p-3.5">
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground"><Icon className="h-3.5 w-3.5" />{label}</div>
      <div className="mono mt-1.5 truncate text-lg font-bold tracking-tight" title={`${f(used)} / ${unlimited ? "不限" : f(limit)}`}>
        {f(used)}<span className="text-xs font-normal text-muted-foreground"> / {unlimited ? "不限" : f(limit)}</span>
      </div>
      <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-muted">
        <div className={cn("h-full rounded-full transition-all", danger ? "bg-destructive" : "bg-primary")}
          style={{ width: `${unlimited ? 0 : pct}%` }} />
      </div>
    </div>
  );
}

/** 密码输入框(带小眼睛切换)。 */
function PwInput({ value, onChange, placeholder, autoFocus }: {
  value: string; onChange: (v: string) => void; placeholder: string; autoFocus?: boolean;
}) {
  const [show, setShow] = useState(false);
  return (
    <div className="relative">
      <Input type={show ? "text" : "password"} placeholder={placeholder} value={value}
        onChange={(e) => onChange(e.target.value)} autoFocus={autoFocus} className="pr-9" />
      <button type="button" onClick={() => setShow(!show)} title={show ? "隐藏密码" : "显示密码"}
        className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground">
        {show ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
      </button>
    </div>
  );
}

/** 修改密码弹窗:旧密码校验 + 新密码两遍输入,成功后自动关闭。 */
function PasswordDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [oldPw, setOldPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [ok, setOk] = useState(false);
  useEffect(() => {
    if (open) { setOldPw(""); setNewPw(""); setConfirmPw(""); setErr(""); setOk(false); }
  }, [open]);
  const submit = async () => {
    setErr("");
    if (newPw.length < 8) { setErr("新密码至少 8 位"); return; }
    if (newPw !== confirmPw) { setErr("两次输入的新密码不一致"); return; }
    setBusy(true);
    try {
      await api.changePassword(oldPw, newPw);
      setOk(true);
      window.setTimeout(onClose, 1200);
    } catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>修改密码</DialogTitle>
          <DialogDescription>修改成功后请用新密码重新登录(其他已登录会话不受影响)。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <PwInput value={oldPw} onChange={setOldPw} placeholder="当前密码" autoFocus />
          <PwInput value={newPw} onChange={setNewPw} placeholder="新密码(至少 8 位)" />
          <PwInput value={confirmPw} onChange={setConfirmPw} placeholder="再输一遍新密码" />
          {ok && <p className="flex items-center gap-1 text-sm text-success"><CheckCircle2 className="h-4 w-4" />密码已修改</p>}
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose} disabled={busy}>取消</Button>
          <Button onClick={submit} disabled={busy || !oldPw || !newPw || !confirmPw}>
            <ShieldCheck className="h-4 w-4" />{ok ? "已完成" : busy ? "提交中…" : "确认修改"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  // 全局 401:api 层广播 relay:auth-expired,这里清 token 并跳回统一登录页。
  useEffect(() => {
    const onExpired = () => { clearToken(); setAuthed(false); };
    window.addEventListener("relay:auth-expired", onExpired);
    return () => window.removeEventListener("relay:auth-expired", onExpired);
  }, []);
  // 无登录态:一律跳转统一登录页(登录/注册/重置都集中在那里,本应用不再内置认证页)。
  useEffect(() => {
    if (!authed) window.location.replace(SITE_PREFIX + "/login");
  }, [authed]);
  if (!authed) return null;
  return (
    <BrowserRouter basename={APP_BASE}>
      <Dashboard onLogout={() => { clearToken(); setAuthed(false); }} />
    </BrowserRouter>
  );
}

type Section = "overview" | "models" | "chat" | "rewards" | "usage" | "docs";
const NAV_ALL: { path: string; label: string; icon: any }[] = [
  { path: "/", label: "概览", icon: LayoutDashboard },
  { path: "/models", label: "模型广场", icon: Boxes },
  { path: "/chat", label: "对话", icon: MessageSquare },
  { path: "/rewards", label: "奖励", icon: Gift },
  { path: "/usage", label: "对话日志", icon: Receipt },
  { path: "/docs", label: "API 文档", icon: BookOpen },
];

function Dashboard({ onLogout }: { onLogout: () => void }) {
  const loc = useLocation();
  const navigate = useNavigate();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [summary, setSummary] = useState<{ granted: number; used: number; balance: number } | null>(null);
  const [quota, setQuota] = useState<QuotaInfo | null>(null);
  const [phone, setPhone] = useState("");
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [usage, setUsage] = useState<any[]>([]);
  const [usageTotal, setUsageTotal] = useState(0);
  const [usagePage, setUsagePage] = useState(1);
  const [usagePageSize, setUsagePageSize] = useState(20);
  const [models, setModels] = useState<ModelCard[]>([]);
  const [series, setSeries] = useState<{ ts: number; tokens: number; calls: number }[]>([]);
  const [seriesDays, setSeriesDays] = useState(30);
  const [reveal, setReveal] = useState<{ kind: string; key: string; pending_approval?: boolean } | null>(null);
  const [rewardsEnabled, setRewardsEnabled] = useState(false);
  const [version, setVersion] = useState("");
  const [pwOpen, setPwOpen] = useState(false);
  useEffect(() => { healthz().then((h) => setVersion(h.version)).catch(() => {}); }, []);
  const nav = NAV_ALL.filter((n) => n.path !== "/rewards" || rewardsEnabled);
  const section: Section = loc.pathname.startsWith("/chat") ? "chat"
    : loc.pathname.startsWith("/models") ? "models"
    : loc.pathname.startsWith("/rewards") ? "rewards"
    : loc.pathname.startsWith("/usage") ? "usage"
    : loc.pathname.startsWith("/docs") ? "docs" : "overview";
  // 奖励功能关闭时,强制回退到概览页
  const effectiveSection = section === "rewards" && !rewardsEnabled ? "overview" as Section : section;
  // 标题与内容一致(按 effectiveSection 而非原始路径判断),避免奖励关闭时标题/内容不一致。
  const title = effectiveSection === "chat" ? "对话" : effectiveSection === "models" ? "模型广场" : effectiveSection === "rewards" ? "奖励"
    : effectiveSection === "usage" ? "对话日志" : effectiveSection === "docs" ? "API 文档" : "概览";

  const loadUsage = async () => {
    try {
      const u = await api.usage(usagePage, usagePageSize);
      setUsage(u.data); setUsageTotal(u.total);
    } catch (e: any) { if (String(e.message).includes("auth")) onLogout(); }
  };
  useEffect(() => { loadUsage(); }, [usagePage, usagePageSize]);

  const [loadErr, setLoadErr] = useState("");
  const refresh = async () => {
    // 并行拉取但各自独立结算:单个接口失败不应让整页数据空白。
    const [me, k, m, s, q] = await Promise.allSettled([api.me(), api.keys(), api.models(), api.summary(), api.quota()]);
    const all = [me, k, m, s, q];
    // 任一请求因鉴权失败 → 统一登出。
    if (all.some((r) => r.status === "rejected" && String((r as PromiseRejectedResult).reason?.message || "").includes("auth"))) { onLogout(); return; }
    if (me.status === "fulfilled") setPhone(me.value.phone || "—");
    if (k.status === "fulfilled") setKeys(k.value.data);
    if (m.status === "fulfilled") setModels(m.value.data);
    if (s.status === "fulfilled") setSummary(s.value);
    if (q.status === "fulfilled") setQuota(q.value);
    const failed = all.filter((r) => r.status === "rejected").length;
    setLoadErr(failed ? `有 ${failed} 项数据加载失败,其余内容已正常显示。` : "");
  };
  useEffect(() => { refresh(); }, []);

  // 套餐限额的实时用量(RPM/TPM/并发)每 20s 轻量轮询。
  useEffect(() => {
    const t = window.setInterval(() => { api.quota().then(setQuota).catch(() => {}); }, 20_000);
    return () => window.clearInterval(t);
  }, []);

  // 配置未就绪前不重定向,避免深链 /rewards 在配置返回前被误判为「奖励已关闭」而跳回首页。
  const [configLoaded, setConfigLoaded] = useState(false);
  useEffect(() => {
    api.config().then((c) => setRewardsEnabled(c.rewards_enabled)).catch(() => {}).finally(() => setConfigLoaded(true));
  }, []);
  // 奖励关闭时直接访问 /rewards:把地址栏也纠正回首页,与导航/标题/内容保持一致。
  useEffect(() => {
    if (configLoaded && section === "rewards" && !rewardsEnabled) navigate("/", { replace: true });
  }, [configLoaded, section, rewardsEnabled, navigate]);

  useEffect(() => {
    api.series(seriesDays).then((ser) => setSeries(ser.data)).catch(() => {});
  }, [seriesDays]);

  const sidebar = (
    <>
      <div className="flex items-center justify-between px-4 py-4">
        <div className="flex items-center gap-2 text-lg font-bold">
          <svg viewBox="0 0 64 64" className="h-7 w-7 shrink-0">
            <defs><linearGradient id="pg" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style={{stopColor:'#6366f1'}}/><stop offset="100%" style={{stopColor:'#8b5cf6'}}/></linearGradient></defs>
            <rect width="64" height="64" rx="14" fill="url(#pg)"/>
            <path d="M10 32 L24 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
            <path d="M18 25 L10 32 L18 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
            <polygon points="32,16 44,24 44,40 32,48 20,40 20,24" fill="none" stroke="white" strokeWidth="3" strokeLinejoin="round"/>
            <path d="M35 22 L28 33 L34 33 L29 44" stroke="white" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
            <path d="M40 32 L54 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
            <path d="M46 25 L54 32 L46 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
          </svg>
          <span>Relay</span>
        </div>
        <button className="md:hidden" onClick={() => setMobileOpen(false)}><X className="h-5 w-5" /></button>
      </div>
      <nav className="flex-1 space-y-1 px-2">
        {nav.map((n) => (
          <NavLink key={n.path} to={n.path} end={n.path === "/"} onClick={() => setMobileOpen(false)}
            className={({ isActive }) => cn("flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
              isActive ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent")}>
            <n.icon className="h-4 w-4 shrink-0" /> {n.label}
          </NavLink>
        ))}
      </nav>
      <div className="px-2 py-3">
        <div className="px-3 pb-2 text-xs text-muted-foreground">{phone}</div>
        <Button variant="ghost" size="sm" className="w-full justify-start gap-3 text-muted-foreground" onClick={() => setPwOpen(true)}>
          <ShieldCheck className="h-4 w-4" />修改密码
        </Button>
        <Button variant="ghost" size="sm" className="w-full justify-start gap-3 text-muted-foreground" onClick={onLogout}>
          <LogOut className="h-4 w-4" />退出登录
        </Button>
        {version && <div className="px-3 pt-2 text-[10px] text-muted-foreground/60">Relay v{version}</div>}
      </div>
    </>
  );

  return (
    <div className="flex h-screen gap-2 overflow-hidden bg-background p-2 md:gap-3 md:p-3">
      <aside className="hidden w-56 shrink-0 flex-col overflow-hidden rounded-2xl border bg-card shadow-sm md:flex">{sidebar}</aside>
      {mobileOpen && (
        <div className="fixed inset-0 z-40 md:hidden">
          <div className="absolute inset-0 bg-black/30" onClick={() => setMobileOpen(false)} />
          <aside className="absolute inset-y-0 left-0 flex w-64 flex-col bg-card shadow-xl">{sidebar}</aside>
        </div>
      )}

      <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border bg-card shadow-sm">
        <header className="flex h-14 shrink-0 items-center gap-3 border-b px-4 md:px-6">
          <button className="md:hidden" onClick={() => setMobileOpen(true)}><Menu className="h-5 w-5" /></button>
          <span className="text-sm font-medium">{title}</span>
        </header>

        {loadErr && (
          <div className="flex shrink-0 flex-wrap items-center gap-3 border-b bg-destructive/5 px-4 py-2 text-xs text-destructive md:px-6">
            <span>{loadErr}</span>
            <Button variant="outline" size="sm" onClick={refresh}>重试</Button>
          </div>
        )}

        {effectiveSection === "chat" ? (
          <ChatView models={models.map((m) => m.name)} onSent={refresh} />
        ) : effectiveSection === "models" ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              <ModelsView models={models} />
            </div>
          </div>
        ) : effectiveSection === "docs" ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              <DocsView />
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              {effectiveSection === "overview" ? (
                <div className="space-y-5">
                  <PlanHero quota={quota} summary={summary} />
                  <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-5">
                    <QuotaCell icon={Gauge} label="并发" limit={quota?.concurrency.limit ?? null} used={quota?.concurrency.used ?? 0} />
                    <QuotaCell icon={Zap} label="RPM · 次/分" limit={quota?.rpm.limit ?? null} used={quota?.rpm.used ?? 0} />
                    <QuotaCell icon={Timer} label="TPM · tokens/分" limit={quota?.tpm.limit ?? null} used={quota?.tpm.used ?? 0} />
                    <QuotaCell icon={CalendarDays} label="日预算" limit={quota?.budget.daily.limit ?? null} used={quota?.budget.daily.used ?? 0} fmt={compactTokens} />
                    <QuotaCell icon={KeySquare} label="月预算" limit={quota?.budget.monthly.limit ?? null} used={quota?.budget.monthly.used ?? 0} fmt={compactTokens} />
                  </div>
                  <div className="grid gap-5 md:grid-cols-2">
                    <InterfaceCard kind="openai" label="OpenAI 接口" baseUrl={`${window.location.origin}${SITE_PREFIX}/v1`} keys={keys} onReveal={setReveal} onChanged={refresh} />
                    <InterfaceCard kind="anthropic" label="Anthropic 接口" baseUrl={`${window.location.origin}${SITE_PREFIX}/v1`} keys={keys} onReveal={setReveal} onChanged={refresh} />
                  </div>
                  <Card>
                    <CardHeader>
                      <div className="flex items-center justify-between gap-3">
                        <CardTitle className="text-base">用量趋势</CardTitle>
                        <div className="flex gap-1">
                          {[7, 30, 90].map((d) => (
                            <button key={d} onClick={() => setSeriesDays(d)}
                              className={cn("rounded-md px-2 py-0.5 text-xs transition-colors",
                                d === seriesDays ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground hover:text-foreground")}>
                              {d}天
                            </button>
                          ))}
                        </div>
                      </div>
                    </CardHeader>
                    <CardContent>
                      {series.length === 0 ? <p className="text-sm text-muted-foreground">暂无数据</p> : (
                        <>
                          <LineChart points={series} />
                          <div className="mt-1 flex justify-between text-xs text-muted-foreground">
                            <span>{fmtDay(series[0].ts)}</span>
                            <span>峰值 {Math.max(0, ...series.map((d) => d.tokens)).toLocaleString()} tokens / 天</span>
                            <span>{fmtDay(series[series.length - 1].ts)}</span>
                          </div>
                        </>
                      )}
                    </CardContent>
                  </Card>
                </div>
              ) : effectiveSection === "rewards" ? (
                <RewardsView />
              ) : (
                <Card>
                  <CardHeader><CardTitle className="text-base">对话日志({usageTotal})</CardTitle></CardHeader>
                  <CardContent>
                    {usage.length === 0 ? <p className="text-sm text-muted-foreground">暂无调用记录</p> : (
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>时间</TableHead><TableHead>模型</TableHead>
                            <TableHead className="text-right">输入 (tokens)</TableHead>
                            <TableHead className="text-right">输出 (tokens)</TableHead>
                            <TableHead className="text-right">计费 (tokens)</TableHead>
                            <TableHead className="text-right" title="最终生效计费倍率 = 模型倍率 × 时段倍率 × 用户倍率">倍率</TableHead>
                            <TableHead className="text-right">耗时</TableHead>
                            <TableHead className="text-right">费用</TableHead>
                            <TableHead>缓存 / 状态</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {usage.map((r, i) => {
                            // created_at 存储约定为 "@unix秒";兼容纯数字与 ISO 字符串。
                            const raw = String(r.created_at ?? "").replace(/^@/, "");
                            const tsMs = /^\d+$/.test(raw) ? Number(raw) * 1000 : NaN;
                            const d = isFinite(tsMs) ? new Date(tsMs) : (r.created_at ? new Date(r.created_at) : null);
                            const time = d && !isNaN(d.getTime())
                              ? `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`
                              : "—";
                            const ms = r.latency_ms;
                            const latency = ms != null ? (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`) : "—";
                            const cost = r.cost_usd != null ? `$${r.cost_usd.toFixed(4)}` : "—";
                            const bm = r.bill_multiplier;
                            const mult = bm != null ? `×${Number.isInteger(bm) ? bm : bm.toFixed(2)}` : "—";
                            const ok = r.status != null && r.status >= 200 && r.status < 400;
                            return (
                              <TableRow key={i}>
                                <TableCell className="mono whitespace-nowrap text-xs text-muted-foreground" title={r.created_at}>{time}</TableCell>
                                <TableCell>
                                  <div className="mono text-xs">{r.model || "—"}</div>
                                  {r.provider && <div className="text-[11px] text-muted-foreground">{r.provider}</div>}
                                </TableCell>
                                <TableCell className="mono text-right tabular-nums">{(r.input_tokens ?? 0).toLocaleString()}</TableCell>
                                <TableCell className="mono text-right tabular-nums">{(r.output_tokens ?? 0).toLocaleString()}</TableCell>
                                <TableCell className="mono text-right tabular-nums">{(r.charged_tokens ?? 0).toLocaleString()}</TableCell>
                                <TableCell className="mono text-right tabular-nums" title="模型倍率 × 时段倍率 × 用户倍率的最终值">{mult}</TableCell>
                                <TableCell className="mono text-right tabular-nums">{latency}</TableCell>
                                <TableCell className="mono text-right tabular-nums">{cost}</TableCell>
                                <TableCell>
                                  <div className="flex items-center gap-1.5">
                                    {r.kind === "cache" && <Badge variant="success" title="精确命中缓存,按折扣率计费">精确</Badge>}
                                    {r.kind === "semantic" && <Badge variant="default" title="语义相似命中缓存,按折扣率计费">语义</Badge>}
                                    {r.status != null && (ok
                                      ? <Badge variant="muted">{r.status}</Badge>
                                      : <Badge variant="default" className="border-destructive/30 bg-destructive/15 text-destructive">{r.status || "失败"}</Badge>)}
                                  </div>
                                </TableCell>
                              </TableRow>
                            );
                          })}
                        </TableBody>
                      </Table>
                    )}
                    <PaginationBar page={usagePage} pageSize={usagePageSize} total={usageTotal} onPageChange={setUsagePage} onPageSizeChange={(s) => { setUsagePage(1); setUsagePageSize(s); }} />
                  </CardContent>
                </Card>
              )}
            </div>
          </div>
        )}
      </main>

      <Dialog open={!!reveal} onOpenChange={(o) => !o && setReveal(null)}>
        {reveal && <RevealContent kind={reveal.kind} apiKey={reveal.key} pendingApproval={reveal.pending_approval} />}
      </Dialog>

      <PasswordDialog open={pwOpen} onClose={() => setPwOpen(false)} />
    </div>
  );
}

function ChatView({ models, onSent }: { models: string[]; onSent: () => void }) {
  const [model, setModel] = useState("");
  // 会话持久化到 sessionStorage:门户内切页(组件卸载)或刷新后回到对话仍保留上下文。
  const [msgs, setMsgs] = useState<ChatMsg[]>(() => {
    try { const raw = sessionStorage.getItem("relay_portal_chat"); return raw ? JSON.parse(raw) : []; } catch { return []; }
  });
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // 是否吸底:用户滚到接近底部时为 true,往上滚则停止自动吸底,滚回底部恢复。
  const stick = useRef(true);

  const onScroll = () => {
    const el = scrollRef.current;
    if (el) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  useEffect(() => { if (!model && models.length) setModel(models[0]); }, [models]);
  useEffect(() => { if (stick.current) scrollRef.current?.scrollTo(0, scrollRef.current.scrollHeight); }, [msgs]);
  useEffect(() => {
    try { sessionStorage.setItem("relay_portal_chat", JSON.stringify(msgs)); } catch { /* 配额/隐私模式错误可忽略 */ }
  }, [msgs]);
  // 卸载时中断进行中的流式请求,避免组件已卸载仍在回调更新状态。
  useEffect(() => () => { abortRef.current?.abort(); }, []);

  const clearChat = () => { setMsgs([]); try { sessionStorage.removeItem("relay_portal_chat"); } catch { /* 忽略 */ } };

  const send = async () => {
    const text = input.trim();
    if (!text || busy || !model) return;
    stick.current = true; // 发送时强制回到底部
    const next = [...msgs, { role: "user", content: text }];
    setMsgs([...next, { role: "assistant", content: "" }]);
    setInput(""); setBusy(true);
    const ac = new AbortController();
    abortRef.current = ac;
    // 超时保护:流式长时间无进展时中断,避免请求永久挂起。
    const timer = window.setTimeout(() => ac.abort(), 120_000);
    try {
      await chatStream(model, next, (d) => {
        setMsgs((cur) => {
          const copy = [...cur];
          const last = copy[copy.length - 1];
          copy[copy.length - 1] = {
            role: "assistant",
            content: last.content + (d.content || ""),
            reasoning: (last.reasoning || "") + (d.reasoning || ""),
          };
          return copy;
        });
      }, ac.signal);
    } catch (e: any) {
      const aborted = ac.signal.aborted;
      setMsgs((cur) => {
        const copy = [...cur];
        const last = copy[copy.length - 1];
        // 主动停止/超时:保留已生成内容并标注;其它错误:展示原因。
        copy[copy.length - 1] = aborted
          ? { ...last, content: (last.content || "") + (last.content ? "\n\n" : "") + "⏹ 已停止生成" }
          : { role: "assistant", content: "⚠️ " + e.message };
        return copy;
      });
    } finally { window.clearTimeout(timer); abortRef.current = null; setBusy(false); onSent(); }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-4 py-2 md:px-6">
        <span className="text-xs text-muted-foreground">模型</span>
        <select value={model} onChange={(e) => setModel(e.target.value)}
          className="h-8 rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring">
          {models.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
        {msgs.length > 0 && (
          <Button variant="ghost" size="sm" className="ml-auto gap-1.5 text-muted-foreground" onClick={clearChat} disabled={busy}>
            <Trash2 className="h-3.5 w-3.5" />清空对话
          </Button>
        )}
      </div>

      <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-auto">
        <div className="mx-auto max-w-3xl space-y-5 p-4 md:p-6">
          {msgs.length === 0 ? (
            <div className="pt-20 text-center text-sm text-muted-foreground">
              <MessageSquare className="mx-auto mb-3 h-8 w-8 opacity-40" />
              开始和模型对话吧 — 对话经网关真实计费,按 tokens 记入「对话日志」。
            </div>
          ) : msgs.map((m, i) => (
            <div key={i} className={cn("flex", m.role === "user" ? "justify-end" : "justify-start")}>
              <div className={cn("max-w-[85%] rounded-2xl px-4 py-2.5 text-sm leading-relaxed",
                m.role === "user" ? "bg-primary text-primary-foreground" : "border bg-card")}>
                {m.reasoning && (
                  <details open={!m.content} className="mb-2 rounded-lg bg-muted/60 px-3 py-2 text-xs text-muted-foreground">
                    <summary className="cursor-pointer select-none font-medium">💭 思考过程{m.content && "(已完成,点击展开)"}</summary>
                    <div className="mt-1.5 whitespace-pre-wrap leading-relaxed">{m.reasoning}</div>
                  </details>
                )}
                <div className="whitespace-pre-wrap">
                  {m.content || (!m.reasoning && <span className="opacity-50">…</span>)}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="shrink-0 border-t bg-card/50 p-3 md:p-4">
        <div className="mx-auto flex max-w-3xl items-end gap-2">
          <textarea
            value={input} onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }}
            rows={1} placeholder="输入消息,Enter 发送,Shift+Enter 换行"
            className="max-h-40 min-h-[40px] flex-1 resize-none rounded-xl border border-input bg-card px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring" />
          {busy ? (
            <Button size="icon" variant="outline" className="h-10 w-10 shrink-0" onClick={() => abortRef.current?.abort()} title="停止生成">
              <Square className="h-4 w-4" />
            </Button>
          ) : (
            <Button size="icon" className="h-10 w-10 shrink-0" disabled={!input.trim()} onClick={send}>
              <SendHorizontal className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

const STATUS_META: Record<number, { label: string; cls: string; icon: any }> = {
  0: { label: "审核中", cls: "text-amber-600", icon: Clock },
  1: { label: "已通过", cls: "text-emerald-600", icon: CheckCircle2 },
  2: { label: "已驳回", cls: "text-destructive", icon: XCircle },
};

const EVIDENCE_ICON: Record<EvidenceType, any> = {
  screenshot: Star, link: Code2, text: Lightbulb, none: Gift,
};

/** 读取图片文件,等比缩放到最长边 maxDim 后导出为 JPEG data URL(压缩,减小上传体积)。 */
function fileToDataUrl(file: File, maxDim = 1280, quality = 0.82): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    const url = URL.createObjectURL(file);
    img.onload = () => {
      URL.revokeObjectURL(url);
      const scale = Math.min(1, maxDim / Math.max(img.width, img.height));
      const w = Math.round(img.width * scale), h = Math.round(img.height * scale);
      const canvas = document.createElement("canvas");
      canvas.width = w; canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) { reject(new Error("无法处理图片")); return; }
      ctx.drawImage(img, 0, 0, w, h);
      resolve(canvas.toDataURL("image/jpeg", quality));
    };
    img.onerror = () => { URL.revokeObjectURL(url); reject(new Error("图片读取失败")); };
    img.src = url;
  });
}

function RewardsView() {
  const [info, setInfo] = useState<RewardInfo | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const [err, setErr] = useState("");
  const [done, setDone] = useState("");

  const load = async () => {
    try { setInfo(await api.rewards()); } catch (e: any) { setErr(e.message); }
  };
  useEffect(() => { load(); }, []);

  const claimOf = (taskId: number) => info?.claims.find((c) => c.task_id === taskId);

  const submit = async (taskId: number, evidence?: string) => {
    setBusy(taskId); setErr(""); setDone("");
    try {
      await api.claimReward(taskId, evidence);
      setDone("申领已提交,等待后台人工审核");
      await load();
    } catch (e: any) { setErr(e.message); } finally { setBusy(null); }
  };

  if (!info) return <p className="text-sm text-muted-foreground">{err || "加载中…"}</p>;

  const fmt = (n?: number) => (n ?? 0).toLocaleString();

  return (
    <div className="space-y-5">
      <Card>
        <CardContent className="pt-5">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Gift className="h-4 w-4" /> 参与开源活动,领取 token 奖励
          </div>
          <p className="mt-2 text-xs text-muted-foreground">
            完成下列任务后提交申领,后台人工审核通过后 token 自动入账到你的余额。每项奖励每个账号仅可领取一次;被驳回可重新提交。
          </p>
        </CardContent>
      </Card>

      {info.tasks.length === 0 ? (
        <Card><CardContent className="pt-5"><p className="text-sm text-muted-foreground">暂无进行中的奖励活动。</p></CardContent></Card>
      ) : (
        <div className="grid gap-5 md:grid-cols-2">
          {info.tasks.map((t) => (
            <RewardTaskCard key={t.id} task={t} claim={claimOf(t.id)} busy={busy === t.id} onSubmit={(ev) => submit(t.id, ev)} />
          ))}
        </div>
      )}

      {done && <p className="text-sm text-emerald-600">{done}</p>}
      {err && <p className="text-sm text-destructive">{err}</p>}

      <Card>
        <CardHeader><CardTitle className="text-base">我的申领记录</CardTitle></CardHeader>
        <CardContent>
          {info.claims.length === 0 ? (
            <p className="text-sm text-muted-foreground">还没有申领记录</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>奖励</TableHead><TableHead>额度</TableHead>
                  <TableHead>状态</TableHead><TableHead>备注</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {info.claims.map((c) => {
                  const m = STATUS_META[c.status] ?? STATUS_META[0];
                  const pending = c.reward_tokens === 0 && c.status !== 1;
                  return (
                    <TableRow key={c.id}>
                      <TableCell>{c.title ?? "已下线任务"}</TableCell>
                      <TableCell className="mono">
                        {pending
                          ? <span className="text-muted-foreground">待评定</span>
                          : <>{c.status === 1 ? "+" : ""}{fmt(c.reward_tokens)}</>}
                      </TableCell>
                      <TableCell>
                        <span className={cn("inline-flex items-center gap-1 text-xs font-medium", m.cls)}>
                          <m.icon className="h-3.5 w-3.5" />{m.label}
                        </span>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">{c.review_note || "—"}</TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

/** 单个奖励任务卡片:按 evidence_type 渲染对应的证明输入。 */
function RewardTaskCard({ task, claim, busy, onSubmit }: {
  task: RewardTask; claim?: RewardClaim; busy: boolean; onSubmit: (evidence?: string) => void;
}) {
  const [val, setVal] = useState("");   // link / text 输入
  const [shot, setShot] = useState(""); // 截图(压缩后的 data URL)
  const [err, setErr] = useState("");
  // 仅待审(0)/ 已通过(1)锁定;已驳回(2)可重新申领。
  const blocked = !!claim && claim.status !== 2;
  const m = claim ? (STATUS_META[claim.status] ?? STATUS_META[0]) : null;
  const fmt = (n: number) => n.toLocaleString();
  const et = task.evidence_type;

  const pickShot = async (file?: File) => {
    if (!file) return;
    setErr("");
    if (!file.type.startsWith("image/")) { setErr("请选择图片文件"); return; }
    try { setShot(await fileToDataUrl(file)); } catch (e: any) { setErr(e.message); }
  };

  const ready =
    et === "screenshot" ? !!shot :
    et === "link" ? !!val.trim() :
    et === "text" ? val.trim().length >= 10 :
    true; // none

  const evidence = et === "screenshot" ? shot : et === "none" ? undefined : val.trim();
  const doSubmit = () => { onSubmit(evidence); setVal(""); setShot(""); };

  const tokensLabel = task.variable
    ? <span>{fmt(task.reward_min)} ~ {fmt(task.reward_max)} <span className="text-sm font-normal text-muted-foreground">tokens · 由管理员评定</span></span>
    : <>+{fmt(task.reward_tokens)} <span className="text-sm font-normal text-muted-foreground">tokens</span></>;

  const Icon = EVIDENCE_ICON[et] ?? Gift;
  const btnLabel = busy ? "提交中…" : blocked ? "已申领"
    : task.variable
      ? `${claim?.status === 2 ? "重新" : ""}提交申领`
      : `${claim?.status === 2 ? "重新" : ""}申领 ${fmt(task.reward_tokens)} tokens`;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between gap-2 text-base">
          <span className="flex items-center gap-2"><Icon className="h-4 w-4 text-amber-500" />{task.title}</span>
          {m && <span className={cn("inline-flex items-center gap-1 text-xs font-medium", m.cls)}>
            <m.icon className="h-3.5 w-3.5" />{m.label}
          </span>}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-2xl font-bold tracking-tight text-primary">{tokensLabel}</div>
        {task.description && <p className="text-xs text-muted-foreground">{task.description}</p>}

        <div className="space-y-2">
          {task.link_url && (
            <a href={task.link_url} target="_blank" rel="noreferrer"
              className="flex w-full items-center justify-center gap-2 rounded-lg border bg-muted/40 px-3 py-2 text-sm hover:bg-accent">
              <ExternalLink className="h-3.5 w-3.5 opacity-60" /> 打开链接
            </a>
          )}

          {!blocked && et === "screenshot" && (
            shot ? (
              <div className="relative">
                <img src={shot} alt="截图" className="max-h-40 w-full rounded-lg border object-contain bg-muted/40" />
                <button onClick={() => setShot("")} title="移除"
                  className="absolute right-1.5 top-1.5 rounded-full bg-background/90 p-1 text-muted-foreground shadow hover:text-destructive">
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            ) : (
              <label className="flex w-full cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground hover:bg-accent">
                <ImagePlus className="h-4 w-4" /> 上传截图
                <input type="file" accept="image/*" className="hidden"
                  onChange={(e) => { pickShot(e.target.files?.[0]); e.target.value = ""; }} />
              </label>
            )
          )}

          {et === "link" && (
            <Input placeholder="填写链接,如 https://…" value={val} onChange={(e) => setVal(e.target.value)} disabled={blocked} />
          )}

          {et === "text" && (
            <>
              <textarea
                value={val} onChange={(e) => setVal(e.target.value)} disabled={blocked}
                rows={4} maxLength={5000} placeholder="请输入内容(10–5000 字)…"
                className="w-full resize-y rounded-lg border border-input bg-card px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60" />
              <div className="text-right text-xs text-muted-foreground">{val.trim().length} / 5000</div>
            </>
          )}

          <Button className="w-full" disabled={blocked || !ready || busy} onClick={doSubmit}>{btnLabel}</Button>
        </div>

        {err && <p className="text-xs text-destructive">{err}</p>}
        {claim?.status === 2 && claim.review_note && <p className="text-xs text-destructive">驳回原因:{claim.review_note}</p>}
      </CardContent>
    </Card>
  );
}

function InterfaceCard({ kind, label, baseUrl, keys, onReveal, onChanged }: {
  kind: string; label: string; baseUrl: string; keys: KeyInfo[];
  onReveal: (r: { kind: string; key: string; pending_approval?: boolean }) => void; onChanged: () => void;
}) {
  const active = keys.find((k) => k.interface_kind === kind && !k.revoked);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const create = async () => {
    setBusy(true); setErr("");
    try { const r = await api.createKey(kind); onReveal({ kind, key: r.key, pending_approval: r.pending_approval }); onChanged(); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  const doRotate = async () => {
    setBusy(true); setErr("");
    try { const r = await api.rotateKey(kind); setConfirmOpen(false); onReveal({ kind, key: r.key, pending_approval: r.pending_approval }); onChanged(); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <KeyRound className="h-4 w-4 text-muted-foreground" /> {label}
          {active
            ? (active.pending_approval
              ? <Badge variant="muted" className="border-amber-500/30 bg-amber-500/10 text-amber-600">待审批</Badge>
              : <Badge variant="success">已启用</Badge>)
            : <Badge variant="muted">未创建</Badge>}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div>
          <div className="mb-1 text-xs text-muted-foreground">Base URL</div>
          <div className="flex items-center gap-2">
            <div className="mono flex-1 rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{baseUrl}</div>
            <CopyButton value={baseUrl} label="复制 Base URL" />
          </div>
        </div>
        <div>
          <div className="mb-1 text-xs text-muted-foreground">可用端点(同一把 Key 通用)</div>
          <div className="space-y-1">
            {(kind === "openai"
              ? [["/v1/chat/completions", "对话补全(流式/非流式)"], ["/v1/responses", "OpenAI Responses"], ["/v1/embeddings", "向量 Embeddings"], ["/v1/models", "模型列表"]]
              : [["/v1/messages", "Anthropic 对话(流式/非流式)"], ["/v1/models", "模型列表"]]
            ).map(([p, d]) => (
              <div key={p} className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-2 py-1 text-xs">
                <span className="mono">{p}</span>
                <span className="text-muted-foreground">{d}</span>
              </div>
            ))}
          </div>
          <p className="mt-1 text-[11px] text-muted-foreground">完整请求示例见「API 文档」。</p>
        </div>
        <div>
          <div className="mb-1 text-xs text-muted-foreground">API Key</div>
          {active?.key_plain ? (
            <div className="flex items-center gap-2">
              <div className="mono flex-1 rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{active.key_prefix}</div>
              <CopyButton value={active.key_plain} label="复制 API Key" />
            </div>
          ) : (
            <div className="mono rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{active ? active.key_prefix : "—"}</div>
          )}
          {active?.pending_approval && (
            <p className="text-xs text-amber-600">该 Key 正在等待管理员批准,批准后才能调用接口</p>
          )}
          <p className="mt-1 text-xs text-muted-foreground">
            {active?.key_plain
              ? "Key 以掩码显示、加密存储且仅你可见,点右侧按钮随时复制;泄露请「刷新 Key」重新生成(旧 Key 立即失效)。"
              : "旧版本创建的 Key 无法直接复制。点击下方「刷新 Key」重新生成一把:新 Key 可随时复制,旧 Key 立即失效。"}
          </p>
        </div>
        {active
          ? <Button variant="outline" disabled={busy} onClick={() => setConfirmOpen(true)}><RefreshCw className="h-4 w-4" />刷新 Key</Button>
          : <Button disabled={busy} onClick={create}><Plus className="h-4 w-4" />创建 Key</Button>}
        {err && <p className="text-sm text-destructive">{err}</p>}
      </CardContent>

      <Dialog open={confirmOpen} onOpenChange={(o) => !busy && setConfirmOpen(o)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>刷新 {label} 的 Key?</DialogTitle>
            <DialogDescription className="text-destructive">
              ⚠️ 刷新会立即吊销当前 Key,正在使用旧 Key 的应用将中断。此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          {err && <p className="text-sm text-destructive">{err}</p>}
          <div className="flex justify-end gap-2">
            <Button variant="outline" disabled={busy} onClick={() => setConfirmOpen(false)}>取消</Button>
            <Button variant="destructive" disabled={busy} onClick={doRotate}>
              <RefreshCw className="h-4 w-4" />确认刷新
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}

function RevealContent({ kind, apiKey, pendingApproval }: { kind: string; apiKey: string; pendingApproval?: boolean }) {
  const [state, setState] = useState<"idle" | "ok" | "sel">("idle");
  const keyRef = useRef<HTMLDivElement>(null);
  const copy = async () => {
    if (await copyText(apiKey)) {
      setState("ok");
      window.setTimeout(() => setState("idle"), 2500);
      return;
    }
    // 复制命令失败(HTTP 环境被浏览器策略拦截):自动全选 Key 文本,用户 Ctrl+C 手动完成。
    if (keyRef.current) {
      const sel = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(keyRef.current);
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
    setState("sel");
  };
  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>你的新 {kind === "openai" ? "OpenAI" : "Anthropic"} Key</DialogTitle>
        <DialogDescription>请立即复制保存;关闭后将无法再次查看完整 Key(库里只存掩码与哈希),如丢失请「刷新 Key」重新生成。{pendingApproval ? "(该 Key 需管理员批准后生效)" : ""}</DialogDescription>
      </DialogHeader>
      <div ref={keyRef} className="mono select-all rounded-lg border bg-muted/50 px-3 py-3 text-sm break-all" title="点击可全选">{apiKey}</div>
      <Button onClick={copy} variant={state === "ok" ? "secondary" : "default"}>
        {state === "ok" && <><Check className="h-4 w-4" />已复制</>}
        {state === "sel" && <><Check className="h-4 w-4" />已全选,请按 Ctrl+C 复制</>}
        {state === "idle" && <><Copy className="h-4 w-4" />复制</>}
      </Button>
      {state === "sel" && <p className="text-xs text-muted-foreground">若 Ctrl+C 无效,请长按(手机)或双击 Key 文本手动选中复制;关闭后只能「刷新 Key」重新生成。</p>}
    </DialogContent>
  );
}
