import { useEffect, useRef, useState, ReactNode } from "react";
import { BrowserRouter, NavLink, useLocation } from "react-router-dom";
import {
  Wallet, KeyRound, Copy, RefreshCw, Plus, LogOut, Check,
  LayoutDashboard, Receipt, MessageSquare, Menu, X, SendHorizontal, Boxes,
  Gift, Star, Code2, Clock, CheckCircle2, XCircle, ExternalLink, ImagePlus, Lightbulb, BookOpen, AlertTriangle,
} from "lucide-react";
import { api, getToken, setToken, clearToken, healthz, KeyInfo, ModelCard, chatStream, ChatMsg, RewardInfo, RewardClaim, RewardTask, EvidenceType } from "./api";
import { copyText } from "./clipboard";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/ui/password-input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
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

/** 值旁边的复制图标按钮:点击后短暂变成绿色对勾。 */
function CopyButton({ value, label = "复制" }: { value: string; label?: string }) {
  const [copied, copy] = useCopy();
  return (
    <button type="button" onClick={() => copy(value)} title={label} aria-label={label}
      className="shrink-0 rounded-md border border-border bg-background p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
      {copied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
    </button>
  );
}

/** 模型广场卡片:展示名称(点击复制)、能力标签、上下文长度、倍率、缓存与单价等信息。 */
function ModelCardView({ m }: { m: ModelCard }) {
  const [copied, copy] = useCopy();
  const ctx = m.context_length != null && m.context_length > 0
    ? m.context_length >= 1_000_000 ? `${(m.context_length / 1_000_000).toFixed(m.context_length % 1_000_000 ? 1 : 0)}M` : `${Math.round(m.context_length / 1000)}K`
    : null;
  const price = (m.input_price != null && m.output_price != null)
    ? `$${m.input_price.toFixed(2)} / $${m.output_price.toFixed(2)}`
    : null;
  return (
    <button type="button" onClick={() => copy(m.name)} title="点击复制模型名"
      className={cn(
        "group flex flex-col gap-2 rounded-xl border p-3.5 text-left transition-all",
        copied
          ? "border-emerald-500/50 bg-emerald-500/5"
          : "border-border bg-card hover:border-primary/40 hover:shadow-sm"
      )}>
      <div className="flex items-center gap-2">
        <span className="mono truncate text-sm font-medium" title={m.name}>{m.name}</span>
        {copied
          ? <span className="ml-auto inline-flex shrink-0 items-center gap-1 text-xs text-emerald-600"><Check className="h-3.5 w-3.5" />已复制</span>
          : <Copy className="ml-auto h-3.5 w-3.5 shrink-0 opacity-0 transition-opacity group-hover:opacity-40" />}
      </div>
      {(m.labels.length > 0 || m.tags.length > 0) && (
        <div className="flex flex-wrap gap-1">
          {m.labels.map((l) => <Badge key={l}>{l}</Badge>)}
          {m.tags.map((t) => <Badge key={t} variant="muted">{t}</Badge>)}
        </div>
      )}
      <div className="mt-auto grid grid-cols-2 gap-x-3 gap-y-1 border-t pt-2.5 text-xs text-muted-foreground">
        <span className="flex items-center gap-1" title="上下文窗口">上下文<span className="mono text-foreground">{ctx ?? "—"}</span></span>
        <span className="flex items-center gap-1" title="计费倍率">倍率<span className="mono text-foreground">×{m.multiplier}</span></span>
        <span className="flex items-center gap-1" title="每 1M tokens 输入/输出单价">价格<span className="mono truncate text-foreground" title={price ?? "未定价"}>{price ?? "未定价"}</span></span>
        <span className="flex items-center gap-1" title="该模型聚合的上游部署数">上游<span className="mono text-foreground">{m.upstreams}</span></span>
        {m.cache && <span className="col-span-2 inline-flex items-center gap-1 text-success" title="支持提示词缓存,命中按折扣计费"><Star className="h-3 w-3" />支持上下文缓存</span>}
      </div>
    </button>
  );
}

/** 模型广场独立页:顶部概要与能力标签筛选,下方为可复制模型卡片网格。 */
function ModelsView({ models }: { models: ModelCard[] }) {
  const [tag, setTag] = useState<string | null>(null);
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
          {shown.map((m) => <ModelCardView key={m.name} m={m} />)}
        </div>
      )}
      <p className="text-xs text-muted-foreground">点击卡片即可复制模型名,在 API 调用的 model 字段中使用。</p>
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

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  // 全局 401:api 层广播 relay:auth-expired,这里统一清 token 回登录页。
  useEffect(() => {
    const onExpired = () => { clearToken(); setAuthed(false); };
    window.addEventListener("relay:auth-expired", onExpired);
    return () => window.removeEventListener("relay:auth-expired", onExpired);
  }, []);
  if (!authed) return <Login onSuccess={() => setAuthed(true)} />;
  return (
    <BrowserRouter basename={APP_BASE}>
      <Dashboard onLogout={() => { clearToken(); setAuthed(false); }} />
    </BrowserRouter>
  );
}

function Login({ onSuccess }: { onSuccess: () => void }) {
  const [mode, setMode] = useState<"login" | "register" | "reset">("login");
  const [account, setAccount] = useState("");
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [err, setErr] = useState("");
  const [loading, setLoading] = useState(false);
  const [cooldown, setCooldown] = useState(0);

  useEffect(() => {
    if (cooldown <= 0) return;
    const t = setTimeout(() => setCooldown((c) => c - 1), 1000);
    return () => clearTimeout(t);
  }, [cooldown]);

  const needCode = mode === "register" || mode === "reset";
  const switchMode = (m: typeof mode) => { setMode(m); setErr(""); setCode(""); setPassword(""); setConfirm(""); };

  const sendCode = async () => {
    if (!account.trim() || cooldown > 0) return;
    setErr("");
    try {
      const r = await api.sendEmailCode(account.trim());
      setCooldown(60);
      if (r.dev_code) { setCode(r.dev_code); setErr("开发模式:验证码已自动填入"); }
    } catch (e: any) { setErr(e.message); }
  };

  const canSubmit = mode === "login"
    ? account.trim() && password
    : account.trim() && code.trim() && password && confirm;
  const submit = async () => {
    if (!canSubmit) return;
    if (needCode && password !== confirm) { setErr("两次输入的密码不一致"); return; }
    setErr(""); setLoading(true);
    try {
      const r = mode === "login" ? await api.login(account.trim(), password)
        : mode === "register" ? await api.register(account.trim(), code.trim(), password)
        : await api.resetPassword(account.trim(), code.trim(), password);
      setToken(r.token); onSuccess();
    } catch (e: any) {
      // 登录模式:401 显示通用文案;429 等其它错误透出后端提示(如防爆破锁定)。
      setErr(mode === "login" && e?.status !== 429 ? "用户名 / 邮箱或密码错误" : e.message);
    } finally { setLoading(false); }
  };

  const subtitle = mode === "login" ? "账号登录" : mode === "register" ? "邮箱注册 · 注册即送额度" : "重置密码 · 邮箱验证";
  const submitLabel = mode === "login" ? "登录" : mode === "register" ? "注册" : "重置密码";
  const pwPlaceholder = mode === "login" ? "密码" : mode === "register" ? "设置密码(至少 6 位)" : "设置新密码(至少 6 位)";

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-xl">
            <svg viewBox="0 0 64 64" className="h-7 w-7 shrink-0">
              <defs><linearGradient id="lg" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style={{stopColor:'#6366f1'}}/><stop offset="100%" style={{stopColor:'#8b5cf6'}}/></linearGradient></defs>
              <rect width="64" height="64" rx="14" fill="url(#lg)"/>
              <path d="M10 32 L24 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
              <path d="M18 25 L10 32 L18 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
              <polygon points="32,16 44,24 44,40 32,48 20,40 20,24" fill="none" stroke="white" strokeWidth="3" strokeLinejoin="round"/>
              <path d="M35 22 L28 33 L34 33 L29 44" stroke="white" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
              <path d="M40 32 L54 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
              <path d="M46 25 L54 32 L46 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
            </svg>
            <span>Rel<span className="text-primary">ay</span> 控制台</span>
          </CardTitle>
          <p className="text-sm text-muted-foreground">{subtitle}</p>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input
            type={needCode ? "email" : "text"}
            placeholder={mode === "login" ? "用户名 / 邮箱" : "邮箱"}
            value={account}
            onChange={(e) => setAccount(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
          {needCode && (
            <div className="flex gap-2">
              <Input placeholder="邮箱验证码" value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))} maxLength={6} />
              <Button variant="outline" className="shrink-0" disabled={!account.trim() || cooldown > 0} onClick={sendCode}>
                {cooldown > 0 ? `${cooldown}s` : "发送验证码"}
              </Button>
            </div>
          )}
          <PasswordInput placeholder={pwPlaceholder} value={password}
            onChange={(e) => setPassword(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
          {needCode && (
            <Input type="password" placeholder="确认密码" value={confirm}
              onChange={(e) => setConfirm(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
          )}
          <Button className="w-full" disabled={loading || !canSubmit} onClick={submit}>
            {loading ? "请稍候…" : submitLabel}
          </Button>
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            {mode === "login" ? (
              <>
                <button className="hover:text-foreground" onClick={() => switchMode("register")}>没有账号?邮箱注册</button>
                <button className="hover:text-foreground" onClick={() => switchMode("reset")}>忘记密码?</button>
              </>
            ) : (
              <button className="hover:text-foreground" onClick={() => switchMode("login")}>← 返回登录</button>
            )}
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </CardContent>
      </Card>
    </div>
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
  const [mobileOpen, setMobileOpen] = useState(false);
  const [summary, setSummary] = useState<{ granted: number; used: number; balance: number } | null>(null);
  const [phone, setPhone] = useState("");
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [usage, setUsage] = useState<any[]>([]);
  const [usageTotal, setUsageTotal] = useState(0);
  const [usagePage, setUsagePage] = useState(1);
  const [usagePageSize, setUsagePageSize] = useState(20);
  const [models, setModels] = useState<ModelCard[]>([]);
  const [series, setSeries] = useState<{ ts: number; tokens: number; calls: number }[]>([]);
  const [seriesDays, setSeriesDays] = useState(30);
  const [reveal, setReveal] = useState<{ kind: string; key: string } | null>(null);
  const [rewardsEnabled, setRewardsEnabled] = useState(false);
  const [version, setVersion] = useState("");
  useEffect(() => { healthz().then((h) => setVersion(h.version)).catch(() => {}); }, []);
  const nav = NAV_ALL.filter((n) => n.path !== "/rewards" || rewardsEnabled);
  const section: Section = loc.pathname.startsWith("/chat") ? "chat"
    : loc.pathname.startsWith("/models") ? "models"
    : loc.pathname.startsWith("/rewards") ? "rewards"
    : loc.pathname.startsWith("/usage") ? "usage"
    : loc.pathname.startsWith("/docs") ? "docs" : "overview";
  // 奖励功能关闭时,强制回退到概览页
  const effectiveSection = section === "rewards" && !rewardsEnabled ? "overview" as Section : section;
  const title = section === "chat" ? "对话" : section === "models" ? "模型广场" : section === "rewards" ? "奖励"
    : section === "usage" ? "对话日志" : section === "docs" ? "API 文档" : "概览";

  const loadUsage = async () => {
    try {
      const u = await api.usage(usagePage, usagePageSize);
      setUsage(u.data); setUsageTotal(u.total);
    } catch (e: any) { if (String(e.message).includes("auth")) onLogout(); }
  };
  useEffect(() => { loadUsage(); }, [usagePage, usagePageSize]);

  const refresh = async () => {
    try {
      const [me, k, m, s] = await Promise.all([api.me(), api.keys(), api.models(), api.summary()]);
      setPhone(me.phone || "—"); setKeys(k.data); setModels(m.data); setSummary(s);
    } catch (e: any) { if (String(e.message).includes("auth")) onLogout(); }
  };
  useEffect(() => { refresh(); }, []);

  useEffect(() => {
    api.config().then((c) => setRewardsEnabled(c.rewards_enabled)).catch(() => {});
  }, []);

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
          <span>Rel<span className="text-primary">ay</span></span>
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
                  <Card>
                    <CardContent className="pt-5">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Wallet className="h-4 w-4" /> 我的额度(Tokens)
                      </div>
                      <div className="mt-3 grid grid-cols-3 gap-3">
                        <div>
                          <div className="text-xs text-muted-foreground">总量</div>
                          <div className="mono text-xl font-bold tracking-tight md:text-2xl">{summary ? summary.granted.toLocaleString() : "…"}</div>
                        </div>
                        <div>
                          <div className="text-xs text-muted-foreground">已用</div>
                          <div className="mono text-xl font-bold tracking-tight md:text-2xl">{summary ? summary.used.toLocaleString() : "…"}</div>
                        </div>
                        <div>
                          <div className="text-xs text-muted-foreground">余额</div>
                          <div className="mono text-xl font-bold tracking-tight text-primary md:text-2xl">{summary ? summary.balance.toLocaleString() : "…"}</div>
                        </div>
                      </div>
                      <div className="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-muted">
                        <div className="h-full rounded-full bg-primary"
                          style={{ width: `${summary && summary.granted > 0 ? Math.max(0, Math.min(100, (summary.used / summary.granted) * 100)) : 0}%` }} />
                      </div>
                      <p className="mt-2 text-xs text-muted-foreground">
                        已用 {summary && summary.granted > 0 ? Math.round((summary.used / summary.granted) * 100) : 0}% · 两个接口共用
                      </p>
                    </CardContent>
                  </Card>
                  <div className="grid gap-5 md:grid-cols-2">
                    <InterfaceCard kind="openai" label="OpenAI 接口" baseUrl={`${window.location.origin}/v1`} keys={keys} onReveal={setReveal} onChanged={refresh} />
                    <InterfaceCard kind="anthropic" label="Anthropic 接口" baseUrl={`${window.location.origin}/v1`} keys={keys} onReveal={setReveal} onChanged={refresh} />
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
        {reveal && <RevealContent kind={reveal.kind} apiKey={reveal.key} />}
      </Dialog>
    </div>
  );
}

function ChatView({ models, onSent }: { models: string[]; onSent: () => void }) {
  const [model, setModel] = useState("");
  const [msgs, setMsgs] = useState<ChatMsg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  // 是否吸底:用户滚到接近底部时为 true,往上滚则停止自动吸底,滚回底部恢复。
  const stick = useRef(true);

  const onScroll = () => {
    const el = scrollRef.current;
    if (el) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  useEffect(() => { if (!model && models.length) setModel(models[0]); }, [models]);
  useEffect(() => { if (stick.current) scrollRef.current?.scrollTo(0, scrollRef.current.scrollHeight); }, [msgs]);

  const send = async () => {
    const text = input.trim();
    if (!text || busy || !model) return;
    stick.current = true; // 发送时强制回到底部
    const next = [...msgs, { role: "user", content: text }];
    setMsgs([...next, { role: "assistant", content: "" }]);
    setInput(""); setBusy(true);
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
      });
    } catch (e: any) {
      setMsgs((cur) => {
        const copy = [...cur];
        copy[copy.length - 1] = { role: "assistant", content: "⚠️ " + e.message };
        return copy;
      });
    } finally { setBusy(false); onSent(); }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-4 py-2 md:px-6">
        <span className="text-xs text-muted-foreground">模型</span>
        <select value={model} onChange={(e) => setModel(e.target.value)}
          className="h-8 rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring">
          {models.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
      </div>

      <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-auto">
        <div className="mx-auto max-w-3xl space-y-5 p-4 md:p-6">
          {msgs.length === 0 ? (
            <div className="pt-20 text-center text-sm text-muted-foreground">
              <MessageSquare className="mx-auto mb-3 h-8 w-8 opacity-40" />
              开始和模型对话吧 — 调用会按你的账户余额计费
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
          <Button size="icon" className="h-10 w-10 shrink-0" disabled={busy || !input.trim()} onClick={send}>
            <SendHorizontal className="h-4 w-4" />
          </Button>
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
  onReveal: (r: { kind: string; key: string }) => void; onChanged: () => void;
}) {
  const active = keys.find((k) => k.interface_kind === kind && !k.revoked);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const create = async () => {
    setBusy(true); setErr("");
    try { const r = await api.createKey(kind); onReveal({ kind, key: r.key }); onChanged(); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  const doRotate = async () => {
    setBusy(true); setErr("");
    try { const r = await api.rotateKey(kind); setConfirmOpen(false); onReveal({ kind, key: r.key }); onChanged(); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <KeyRound className="h-4 w-4 text-muted-foreground" /> {label}
          {active ? <Badge variant="success">已启用</Badge> : <Badge variant="muted">未创建</Badge>}
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
          <div className="mb-1 text-xs text-muted-foreground">API Key</div>
          {active?.key_plain ? (
            <div className="flex items-center gap-2">
              <div className="mono flex-1 rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{active.key_prefix}</div>
              <CopyButton value={active.key_plain} label="复制 API Key" />
            </div>
          ) : (
            <div className="space-y-1">
              <div className="mono rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{active ? active.key_prefix : "—"}</div>
              {active && (
                <p className="flex items-start gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-xs leading-5 text-amber-700 dark:text-amber-400">
                  <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span>
                    旧版本创建的 Key 不保存明文，无法直接复制。点击下方「刷新 Key」重新生成一把：新 Key 立即可复制，旧 Key 随即失效。
                  </span>
                </p>
              )}
            </div>
          )}
          {active && active.key_plain && <p className="mt-1 text-xs text-muted-foreground">Key 以掩码显示,点右侧按钮复制完整值;「刷新 Key」可重新生成(旧 Key 立即失效)。</p>}
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

function RevealContent({ kind, apiKey }: { kind: string; apiKey: string }) {
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
        <DialogDescription>请立即复制保存;之后也可在密钥页随时查看和复制,「刷新 Key」可重新生成。</DialogDescription>
      </DialogHeader>
      <div ref={keyRef} className="mono select-all rounded-lg border bg-muted/50 px-3 py-3 text-sm break-all" title="点击可全选">{apiKey}</div>
      <Button onClick={copy} variant={state === "ok" ? "secondary" : "default"}>
        {state === "ok" && <><Check className="h-4 w-4" />已复制</>}
        {state === "sel" && <><Check className="h-4 w-4" />已全选,请按 Ctrl+C 复制</>}
        {state === "idle" && <><Copy className="h-4 w-4" />复制</>}
      </Button>
      {state === "sel" && <p className="text-xs text-muted-foreground">若 Ctrl+C 无效,请长按(手机)或双击 Key 文本手动选中复制;关闭后也可在密钥页随时查看。</p>}
    </DialogContent>
  );
}
