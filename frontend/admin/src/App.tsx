import React, { useEffect, useRef, useState } from "react";
import { BrowserRouter, Routes, Route, NavLink, Navigate, useLocation, useNavigate } from "react-router-dom";
import { Users as UsersIcon, Boxes, Layers, BarChart3, LayoutDashboard, LogOut, Plus, Power, Menu, X, Trash2, Pencil, TrendingUp, Activity, Star, Gift, Check, ExternalLink, Settings, Send, Zap, RefreshCw, Clock, ShieldAlert, ShieldCheck, Cpu, Search, RotateCcw, AlertTriangle, Link2, GitBranch, DollarSign, Database, Sparkles, Download, Upload, BookOpen } from "lucide-react";
import { api, getToken, setToken, clearToken, healthz, UserRow, ModelRow, ProviderRow, ProviderModelRow, ProviderHealthItem, GroupRow, RouteRow, RewardClaimRow, RewardTaskRow, RewardTaskBody, EvidenceType, EmailSettingsResp, UpstreamRow, UpstreamsResp, FailureRow, AuditFailuresResp, MetricsSeriesPoint, MetricsDashboardResp, MetricsUpstreamRow, RequestLogRow, RequestAttempt, TimeRuleRow, TimeRulePayload, CacheStatsResp, CacheTrendPoint, CacheHitRow, EmbeddingSettingsResp, UsageBreakdownResp, UsageBreakdownRow, ImportGroupPayload, ImportPreviewResp, AuditLogRow } from "./api";
import { Button } from "@/components/ui/button";
import { RowActions } from "@/components/ui/row-actions";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import EChart, { cssVar } from "@/components/EChart";
import type { EChartsCoreOption } from "echarts/core";
import PaginationBar from "./PaginationBar";
import ApiDocsPanel from "./ApiDocs";
import { PasswordInput } from "@/components/ui/password-input";
import { Toaster } from "sonner";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Switch } from "@/components/ui/switch";

// 子路径反代兼容:部署在 https://host/relay/admin/ 这类前缀下时,路由 basename 探测为 "/relay/admin"。
const APP_BASE = (() => {
  const m = window.location.pathname.match(/^(.*)\/admin(?=\/|$)/);
  return (m?.[1] ?? "") + "/admin";
})();

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  // 401 凭证失效统一登出:api 层清除 token 后广播此事件,这里切回登录页。
  useEffect(() => {
    const onExpired = () => setAuthed(false);
    window.addEventListener("relay:auth-expired", onExpired);
    return () => window.removeEventListener("relay:auth-expired", onExpired);
  }, []);
  return (
    <>
      <Toaster position="top-center" richColors />
      {!authed ? (
        <Login onSuccess={() => setAuthed(true)} />
      ) : (
        <BrowserRouter basename={APP_BASE}>
          <Console onLogout={() => { clearToken(); setAuthed(false); }} />
        </BrowserRouter>
      )}
    </>
  );
}

function Login({ onSuccess }: { onSuccess: () => void }) {
  const [u, setU] = useState("admin");
  const [p, setP] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    if (busy) return;
    setErr("");
    setBusy(true);
    try { const r = await api.login(u, p); setToken(r.token); onSuccess(); } catch (e: any) { setErr(e?.status === 429 ? e.message : "用户名或密码错误"); } finally { setBusy(false); }
  };
  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-xl">
            <svg viewBox="0 0 64 64" className="h-7 w-7 shrink-0">
              <defs><linearGradient id="alg" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style={{stopColor:'#6366f1'}}/><stop offset="100%" style={{stopColor:'#8b5cf6'}}/></linearGradient></defs>
              <rect width="64" height="64" rx="14" fill="url(#alg)"/>
              <path d="M10 32 L24 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
              <path d="M18 25 L10 32 L18 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
              <polygon points="32,16 44,24 44,40 32,48 20,40 20,24" fill="none" stroke="white" strokeWidth="3" strokeLinejoin="round"/>
              <path d="M35 22 L28 33 L34 33 L29 44" stroke="white" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
              <path d="M40 32 L54 32" stroke="white" strokeWidth="4" strokeLinecap="round"/>
              <path d="M46 25 L54 32 L46 39" stroke="white" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/>
            </svg>
            <span>Rel<span className="text-primary">ay</span> 管理后台</span>
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input placeholder="用户名" value={u} onChange={(e) => setU(e.target.value)} />
          <PasswordInput placeholder="密码" value={p} onChange={(e) => setP(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
          <Button className="w-full" disabled={busy || !u.trim() || !p} onClick={submit}>登录</Button>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </CardContent>
      </Card>
    </div>
  );
}

function NotFoundPanel() {
  const nav = useNavigate();
  return (
    <Card className="mx-auto mt-12 max-w-md">
      <CardContent className="flex flex-col items-center gap-3 py-10 text-center">
        <AlertTriangle className="h-8 w-8 text-amber-500" />
        <div className="text-lg font-semibold">404 · 页面不存在</div>
        <p className="text-sm text-muted-foreground">该地址没有对应的功能面板,请从左侧导航选择。</p>
        <Button variant="outline" size="sm" onClick={() => nav("/overview")}>返回概览</Button>
      </CardContent>
    </Card>
  );
}

// 侧边栏分组:按管理员心智模型归组——日常运营 / 模型与上游 / 监控排查 / 系统;空标题组(概览)置顶不缩进。
const NAV_GROUPS: { title: string; items: { path: string; label: string; icon: any }[] }[] = [
  {
    title: "",
    items: [{ path: "/overview", label: "概览", icon: LayoutDashboard }],
  },
  {
    title: "运营",
    items: [
      { path: "/users", label: "用户管理", icon: UsersIcon },
      { path: "/rewards", label: "奖励审核", icon: Gift },
      { path: "/reward-tasks", label: "奖励设置", icon: Star },
    ],
  },
  {
    title: "模型与上游",
    items: [
      { path: "/models", label: "上游模型", icon: Boxes },
      { path: "/groups", label: "模型组", icon: Layers },
    ],
  },
  {
    title: "监控",
    items: [
      { path: "/upstreams", label: "上游治理", icon: Zap },
      { path: "/metrics", label: "接口指标", icon: Activity },
      { path: "/request-logs", label: "请求链路", icon: GitBranch },
      { path: "/cache", label: "语义缓存", icon: Database },
      { path: "/usage", label: "全局用量", icon: BarChart3 },
      { path: "/audit", label: "操作审计", icon: ShieldCheck },
    ],
  },
  {
    title: "系统",
    items: [
      { path: "/docs", label: "API 文档", icon: BookOpen },
      { path: "/settings", label: "设置", icon: Settings },
    ],
  },
];
const NAV = NAV_GROUPS.flatMap((g) => g.items);

function Console({ onLogout }: { onLogout: () => void }) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const [version, setVersion] = useState("");
  const loc = useLocation();
  useEffect(() => { healthz().then((h) => setVersion(h.version)).catch(() => {}); }, []);
  const title = NAV.find((n) => loc.pathname.startsWith(n.path))?.label ?? "";
  const isGroups = loc.pathname.startsWith("/groups");

  const sidebar = (
    <>
      <div className="flex items-center justify-between px-4 py-4">
        <div className="flex items-center gap-2 text-lg font-bold">
          <svg viewBox="0 0 64 64" className="h-7 w-7 shrink-0">
            <defs><linearGradient id="ag" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style={{stopColor:'#6366f1'}}/><stop offset="100%" style={{stopColor:'#8b5cf6'}}/></linearGradient></defs>
            <rect width="64" height="64" rx="14" fill="url(#ag)"/>
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
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 py-1">
        {NAV_GROUPS.map((g, gi) => (
          <div key={gi} className={cn(gi > 0 && "mt-3")}>
            {g.title && (
              <div className="px-3 pb-1 pt-1.5 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/60">{g.title}</div>
            )}
            {g.items.map((n) => (
              <NavLink key={n.path} to={n.path} onClick={() => setMobileOpen(false)}
                className={({ isActive }) => cn("flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                  isActive ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent")}>
                <n.icon className="h-4 w-4 shrink-0" /> {n.label}
              </NavLink>
            ))}
          </div>
        ))}
      </nav>
      <div className="px-2 py-3">
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
        <header className="flex h-14 shrink-0 items-center gap-3 border-b px-4 text-sm font-medium md:px-6">
          <button className="md:hidden" onClick={() => setMobileOpen(true)}><Menu className="h-5 w-5" /></button>
          {title}
        </header>
        {isGroups ? (
          <div className="min-h-0 flex-1 p-4 md:p-6">
            <Routes><Route path="/groups" element={<GroupsPanel />} /></Routes>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              <Routes>
                <Route path="/" element={<Navigate to="/overview" replace />} />
                <Route path="/overview" element={<OverviewPanel />} />
                <Route path="/users" element={<UsersPanel onAuthErr={onLogout} />} />
                <Route path="/models" element={<ModelsPanel />} />
                <Route path="/upstreams" element={<UpstreamsPanel />} />
                <Route path="/metrics" element={<MetricsPanel />} />
                <Route path="/request-logs" element={<RequestLogPanel />} />
                <Route path="/audit" element={<AuditPanel />} />
                <Route path="/rewards" element={<RewardsPanel />} />
                <Route path="/reward-tasks" element={<RewardTasksPanel />} />
                <Route path="/usage" element={<UsagePanel />} />
                <Route path="/cache" element={<CachePanel />} />
                <Route path="/docs" element={<ApiDocsPanel />} />
                <Route path="/settings" element={<SettingsPanel />} />
                <Route path="*" element={<NotFoundPanel />} />
              </Routes>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

const SOURCE_LABEL: Record<string, string> = {
  admin: "管理员", phone: "手机号", wechat: "微信", alipay: "支付宝", email: "邮箱",
};

/// 手写 SVG 柱状图(零依赖):hover 高亮 + 顶部 tooltip,供趋势卡与用户弹窗复用。
function LineChart({ points, fmtX }: { points: { ts: number; tokens: number; calls?: number }[]; fmtX?: (ts: number) => string }) {
  const primary = cssVar("--color-primary", "#6366f1");
  const border = cssVar("--color-border", "#e4e4e7");
  const muted = cssVar("--color-muted-foreground", "#71717a");
  const fmt = fmtX ?? ((ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; });
  const compact = (v: number) => (v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1_000 ? `${(v / 1_000).toFixed(0)}K` : String(v));
  const hasCalls = points.some((p) => p.calls != null);
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
      type: "category", data: points.map((p) => fmt(p.ts)), boundaryGap: true,
      axisTick: { show: false }, axisLine: { lineStyle: { color: border } },
      axisLabel: { color: muted, fontSize: 11 },
    },
    yAxis: [
      { type: "value", splitLine: { lineStyle: { type: "dashed", color: border } }, axisLabel: { color: muted, fontSize: 11, formatter: compact } },
      ...(hasCalls ? [{ type: "value", show: false }] : []),
    ],
    series: [
      { name: "tokens", type: "bar", data: points.map((p) => p.tokens), barMaxWidth: 20, itemStyle: { color: primary, borderRadius: [3, 3, 0, 0] } },
      ...(hasCalls ? [{ name: "调用次数", type: "line", yAxisIndex: 1, data: points.map((p) => p.calls), smooth: true, symbol: "circle", symbolSize: 5, itemStyle: { color: "#0ea5e9" }, lineStyle: { width: 2, color: "#0ea5e9" } }] : []),
    ],
  };
  return <EChart option={option} height={220} />;
}

function UserChartDialog({ user, onClose }: { user: UserRow | null; onClose: () => void }) {
  const [data, setData] = useState<{ ts: number; tokens: number; calls: number }[]>([]);
  const [days, setDays] = useState(30);
  useEffect(() => {
    if (user) {
      setData([]);
      api.userSeries(user.id, days).then((r) => setData(r.data));
    }
  }, [user, days]);
  const fmt = (ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; };
  const total = data.reduce((a, p) => a + p.tokens, 0);
  const peak = Math.max(0, ...data.map((d) => d.tokens));
  return (
    <Dialog open={!!user} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{user?.username || user?.phone || "用户"} · 用量趋势</DialogTitle>
          <DialogDescription>共 {total.toLocaleString()} tokens</DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-end gap-1">
          {[7, 30, 90].map((d) => (
            <button key={d} onClick={() => setDays(d)}
              className={cn("rounded-md px-2 py-0.5 text-xs transition-colors",
                d === days ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground hover:text-foreground")}>
              {d}天
            </button>
          ))}
        </div>
        {data.length === 0 ? <p className="text-sm text-muted-foreground">加载中或暂无数据…</p> : (
          <div>
            <LineChart points={data} fmtX={fmt} />
            <div className="mt-1 flex justify-between text-xs text-muted-foreground">
              <span>{fmt(data[0].ts)}</span>
              <span>峰值 {peak.toLocaleString()} / 天</span>
              <span>{fmt(data[data.length - 1].ts)}</span>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

type Confirm = { title: string; desc?: string; action: () => void | Promise<void> };

function ConfirmDialog({ confirm, onClose }: { confirm: Confirm | null; onClose: () => void }) {
  return (
    <Dialog open={!!confirm} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{confirm?.title}</DialogTitle>
          {confirm?.desc && <DialogDescription>{confirm.desc}</DialogDescription>}
        </DialogHeader>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button variant="destructive" onClick={async () => { await confirm?.action(); onClose(); }}>
            <Trash2 className="h-4 w-4" />删除
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Avatar({ name }: { name: string }) {
  const ch = (name || "?").trim().charAt(0).toUpperCase() || "?";
  const colors = ["#6366f1", "#16a34a", "#dc2626", "#d97706", "#0891b2", "#7c3aed", "#db2777", "#2563eb"];
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return (
    <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-medium text-white"
      style={{ background: colors[h % colors.length] }}>{ch}</span>
  );
}

function UserDialog({ dlg, groups, onClose, onSaved }: { dlg: { edit: UserRow | null } | null; groups: GroupRow[]; onClose: () => void; onSaved: () => void }) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [groupId, setGroupId] = useState(0);
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [conc, setConc] = useState("16");
  const [amount, setAmount] = useState("");
  const [budgetDaily, setBudgetDaily] = useState("");
  const [budgetMonthly, setBudgetMonthly] = useState("");
  const [err, setErr] = useState("");
  useEffect(() => {
    if (open) {
      setUsername(edit?.username ?? ""); setPassword(""); setGroupId(edit?.group_id ?? 0);
      setEmail(edit?.email ?? ""); setPhone(edit?.phone ?? "");
      setConc(String(edit?.concurrency_limit ?? 16));
      setAmount(edit ? "0" : "10000000"); setErr("");
      setBudgetDaily(edit?.budget_daily_tokens ? String(edit.budget_daily_tokens) : "");
      setBudgetMonthly(edit?.budget_monthly_tokens ? String(edit.budget_monthly_tokens) : "");
    }
  }, [dlg]);

  const submit = async () => {
    setErr("");
    const bd = budgetDaily.trim() === "" ? 0 : Number(budgetDaily);
    const bm = budgetMonthly.trim() === "" ? 0 : Number(budgetMonthly);
    if (!Number.isFinite(bd) || bd < 0 || !Number.isFinite(bm) || bm < 0) {
      setErr("预算需为非负数字,留空或 0 表示不限");
      return;
    }
    try {
      if (edit) {
        const body: any = {
          email: email.trim(), phone: phone.trim(),
          group_id: groupId, concurrency_limit: Number(conc),
          budget_daily_tokens: bd, budget_monthly_tokens: bm,
        };
        if (password) body.password = password;
        if (Number(amount)) body.add_tokens = Number(amount);
        await api.patchUser(edit.id, body);
      } else {
        await api.createUser({
          username: username.trim(), password,
          email: email.trim() || undefined, phone: phone.trim() || undefined,
          group_id: groupId || undefined, grant_tokens: Number(amount) || 0,
          concurrency_limit: Number(conc),
          budget_daily_tokens: bd || undefined, budget_monthly_tokens: bm || undefined,
        });
      }
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
  };
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{edit ? "编辑用户" : "创建用户"}</DialogTitle>
          {edit && <DialogDescription>当前余额 {edit.balance.toLocaleString()} tokens</DialogDescription>}
        </DialogHeader>
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">用户名{edit ? "(不可改)" : " *"}</label>
              <Input placeholder="登录用户名" value={username} onChange={(e) => setUsername(e.target.value)} disabled={!!edit} autoFocus={!edit} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">{edit ? "密码(留空不改)" : "密码 *"}</label>
              <Input type="password" placeholder={edit ? "留空保持不变" : "登录密码"} value={password} onChange={(e) => setPassword(e.target.value)} /></div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">模型组</label>
              <select value={groupId} onChange={(e) => setGroupId(Number(e.target.value))} className={sel}>
                <option value={0}>未绑定</option>
                {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
              </select></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">并发上限</label>
              <Input value={conc} onChange={(e) => setConc(e.target.value)} /></div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">邮箱(可选)</label>
              <Input placeholder="name@example.com" value={email} onChange={(e) => setEmail(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">手机号(可选)</label>
              <Input placeholder="13800000000" value={phone} onChange={(e) => setPhone(e.target.value)} /></div>
          </div>
          <div><label className="mb-1 block text-xs text-muted-foreground">{edit ? "增减额度(可为负,0 不变)" : "赠送 tokens"}</label>
            <Input value={amount} onChange={(e) => setAmount(e.target.value)} /></div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">日预算 tokens</label>
              <Input placeholder="留空或 0 = 不限" value={budgetDaily} onChange={(e) => setBudgetDaily(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">月预算 tokens</label>
              <Input placeholder="留空或 0 = 不限" value={budgetMonthly} onChange={(e) => setBudgetMonthly(e.target.value)} /></div>
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!username.trim() || (!edit && !password)}><Plus className="h-4 w-4" />{edit ? "保存" : "创建"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function StatCard({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <Card>
      <CardContent className="pt-5">
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className="mono mt-1 text-2xl font-bold tracking-tight">{value}</div>
        {sub && <div className="mt-0.5 text-xs text-muted-foreground">{sub}</div>}
      </CardContent>
    </Card>
  );
}

type Gran = "day" | "week" | "month";
const GRAN_LABEL: Record<Gran, string> = { day: "按天", week: "按周", month: "按月" };

function TrendCard() {
  const [gran, setGran] = useState<Gran>("day");
  const [data, setData] = useState<{ ts: number; tokens: number; calls: number }[] | null>(null);
  useEffect(() => { setData(null); api.overviewSeries(gran).then((r) => setData(r.data)).catch(() => setData([])); }, [gran]);

  const fmtX = (ts: number) => {
    const d = new Date(ts * 1000);
    if (gran === "month") return `${d.getFullYear()}/${d.getMonth() + 1}`;
    return `${d.getMonth() + 1}/${d.getDate()}`;
  };
  const total = (data ?? []).reduce((a, p) => a + p.tokens, 0);
  const peak = Math.max(0, ...(data ?? []).map((p) => p.tokens));
  const peakUnit = gran === "day" ? "天" : gran === "week" ? "周" : "月";

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between space-y-0">
        <CardTitle className="text-base">📈 消耗趋势</CardTitle>
        <div className="flex items-center gap-1 rounded-lg bg-muted p-0.5">
          {(Object.keys(GRAN_LABEL) as Gran[]).map((g) => (
            <button key={g} onClick={() => setGran(g)}
              className={cn("rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                gran === g ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}>
              {GRAN_LABEL[g]}
            </button>
          ))}
        </div>
      </CardHeader>
      <CardContent>
        {data === null ? (
          <p className="py-10 text-center text-sm text-muted-foreground">加载中…</p>
        ) : total === 0 ? (
          <p className="py-10 text-center text-sm text-muted-foreground">该时间范围暂无消耗</p>
        ) : (
          <>
            <LineChart points={data} fmtX={fmtX} />
            <div className="mt-1 flex justify-between text-xs text-muted-foreground">
              <span>{fmtX(data[0].ts)}</span>
              <span>共 {total.toLocaleString()} tokens · 峰值 {peak.toLocaleString()} / {peakUnit}</span>
              <span>{fmtX(data[data.length - 1].ts)}</span>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function OverviewPanel() {
  const [d, setD] = useState<any | null>(null);
  useEffect(() => { api.overview().then(setD); }, []);
  const fmt = (n: number) => (n ?? 0).toLocaleString();

  if (!d) return <p className="text-sm text-muted-foreground">加载中…</p>;
  return (
    <div className="space-y-5">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard label="用户数" value={fmt(d.total_users)} sub={`活跃 ${fmt(d.active_users)}`} />
        <StatCard label="全站总消耗 (tokens)" value={fmt(d.total_used)} />
        <StatCard label="全站总余额 (tokens)" value={fmt(d.total_balance)} />
        <StatCard label="总调用数" value={fmt(d.total_requests)} />
      </div>

      <div className="grid gap-5 lg:grid-cols-2">
        <Card>
          <CardHeader><CardTitle className="text-base">用户消耗排行</CardTitle></CardHeader>
          <CardContent>
            {d.top_users.length === 0 ? <p className="text-sm text-muted-foreground">暂无消耗</p> : (
              <Table>
                <TableHeader><TableRow><TableHead>用户</TableHead><TableHead>已用 tokens</TableHead><TableHead>调用数</TableHead></TableRow></TableHeader>
                <TableBody>
                  {d.top_users.map((u: any) => (
                    <TableRow key={u.user_id}>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Avatar name={u.name || u.user_id} />
                          <span>{u.name || <span className="mono text-xs text-muted-foreground">{String(u.user_id).slice(0, 8)}</span>}</span>
                        </div>
                      </TableCell>
                      <TableCell className="mono">{fmt(u.used)}</TableCell>
                      <TableCell>{fmt(u.calls)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle className="text-base">按模型统计</CardTitle></CardHeader>
          <CardContent>
            {d.by_model.length === 0 ? <p className="text-sm text-muted-foreground">暂无消耗</p> : (
              <Table>
                <TableHeader><TableRow><TableHead>模型</TableHead><TableHead>已用 tokens</TableHead><TableHead>调用数</TableHead></TableRow></TableHeader>
                <TableBody>
                  {d.by_model.map((m: any, i: number) => (
                    <TableRow key={i}>
                      <TableCell className="mono">{m.model || "—"}</TableCell>
                      <TableCell className="mono">{fmt(m.used)}</TableCell>
                      <TableCell>{fmt(m.calls)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </div>

      <TrendCard />
    </div>
  );
}

// 行内预算进度条:80% 转黄、95% 转红,耗尽显示「已耗尽」pill。
function BudgetBar({ label, used, budget }: { label: string; used: number; budget: number | null }) {
  if (!budget || budget <= 0) {
    return (
      <div className="flex items-center gap-1.5">
        <span className="w-3 text-[10px] leading-none text-muted-foreground">{label}</span>
        <span className="text-xs leading-none text-muted-foreground">不限</span>
      </div>
    );
  }
  const pct = Math.min(100, Math.round((used / budget) * 100));
  const exhausted = used >= budget;
  const danger = exhausted || pct >= 95;
  const warn = pct >= 80;
  return (
    <div className="flex items-center gap-1.5">
      <span className="w-3 text-[10px] leading-none text-muted-foreground">{label}</span>
      <div className="h-1.5 w-20 overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all ${danger ? "bg-destructive" : warn ? "bg-amber-500" : "bg-primary"}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      {exhausted ? (
        <Badge variant="destructive" className="px-1.5 py-0 text-[10px] leading-tight">已耗尽</Badge>
      ) : (
        <span className={`text-[10px] leading-none ${danger ? "text-destructive" : warn ? "text-amber-500" : "text-muted-foreground"}`}>{pct}%</span>
      )}
    </div>
  );
}

function UsersPanel({ onAuthErr }: { onAuthErr: () => void }) {
  const [rows, setRows] = useState<UserRow[]>([]);
  const [groups, setGroups] = useState<GroupRow[]>([]);
  const [q, setQ] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [userDlg, setUserDlg] = useState<{ edit: UserRow | null } | null>(null);
  const [chart, setChart] = useState<UserRow | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const qRef = useRef("");
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = async () => {
    try {
      const [u, g] = await Promise.all([api.users(page, pageSize, qRef.current || undefined), api.groups()]);
      setRows(u.data); setGroups(g.data); setTotal(u.total);
    } catch (e: any) { if (String(e.message).includes("auth")) onAuthErr(); }
  };
  useEffect(() => { load(); }, [page, pageSize]);
  // 卸载时清掉搜索防抖定时器,避免组件销毁后 setState。
  useEffect(() => () => { if (timerRef.current) clearTimeout(timerRef.current); }, []);

  // 服务端搜索:500ms 防抖,搜索范围覆盖全部用户(跨页),而非仅当前页。
  const onSearch = (v: string) => {
    setQ(v);
    qRef.current = v;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      if (page !== 1) setPage(1);
      else load();
    }, 500);
  };
  const toggle = async (u: UserRow) => { await api.patchUser(u.id, { status: u.status === 0 ? 1 : 0 }); load(); };
  const bindGroup = async (u: UserRow, gid: number) => { await api.patchUser(u.id, { group_id: gid }); load(); };
  const del = (u: UserRow) => setConfirm({
    title: `删除用户「${u.username || u.phone || u.id.slice(0, 8)}」?`,
    desc: "该用户及其 API Key 将被删除,无法恢复。",
    action: async () => { await api.deleteUser(u.id); load(); },
  });

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">所有用户({total})</CardTitle>
            <div className="flex items-center gap-2">
              <Input className="w-56" placeholder="搜索用户名 / 手机号 / ID" value={q} onChange={(e) => onSearch(e.target.value)} />
              <Button size="sm" onClick={() => setUserDlg({ edit: null })}><Plus className="h-4 w-4" />创建用户</Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>用户</TableHead><TableHead>来源</TableHead><TableHead>总量</TableHead><TableHead>已用</TableHead>
                <TableHead>今日</TableHead><TableHead>预算</TableHead><TableHead>余额</TableHead><TableHead>并发</TableHead>
                <TableHead>模型组</TableHead><TableHead>状态</TableHead><TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((u) => (
                <TableRow key={u.id}>
                  <TableCell>
                    <div className="flex items-center gap-2.5">
                      <Avatar name={u.username || u.phone || "?"} />
                      <div className="leading-tight">
                        <div>{u.username || <span className="text-muted-foreground">—</span>}</div>
                        {u.phone && <div className="text-xs text-muted-foreground">{u.phone}</div>}
                      </div>
                    </div>
                  </TableCell>
                  <TableCell><Badge variant="muted">{SOURCE_LABEL[u.source ?? ""] ?? "—"}</Badge></TableCell>
                  <TableCell className="mono text-muted-foreground">{u.granted.toLocaleString()}</TableCell>
                  <TableCell className="mono">{u.used.toLocaleString()}</TableCell>
                  <TableCell className="mono">{u.today > 0 ? <span className="text-primary">{u.today.toLocaleString()}</span> : "0"}</TableCell>
                  <TableCell>
                    <div className="space-y-1">
                      <BudgetBar label="日" used={u.today} budget={u.budget_daily_tokens} />
                      <BudgetBar label="月" used={u.used_month} budget={u.budget_monthly_tokens} />
                    </div>
                  </TableCell>
                  <TableCell className="mono">{u.balance.toLocaleString()}</TableCell>
                  <TableCell>{u.concurrency_limit ?? <span className="text-muted-foreground">默认</span>}</TableCell>
                  <TableCell>
                    <select value={u.group_id || 0} onChange={(e) => bindGroup(u, Number(e.target.value))}
                      className="h-8 rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring">
                      <option value={0}>未绑定</option>
                      {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
                    </select>
                  </TableCell>
                  <TableCell>{u.status === 0 ? <Badge variant="success">正常</Badge> : <Badge variant="muted">禁用</Badge>}</TableCell>
                  <TableCell>
                    <RowActions actions={[
                      { label: "趋势", icon: <TrendingUp className="h-4 w-4" />, onClick: () => setChart(u) },
                      { label: "编辑", icon: <Pencil className="h-4 w-4" />, onClick: () => setUserDlg({ edit: u }) },
                      { label: u.status === 0 ? "禁用" : "启用", icon: <Power className="h-4 w-4" />, onClick: () => toggle(u) },
                      { label: "删除", icon: <Trash2 className="h-4 w-4" />, variant: "destructive", onClick: () => del(u) },
                    ]} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
        </CardContent>
      </Card>

      <UserDialog dlg={userDlg} groups={groups} onClose={() => setUserDlg(null)} onSaved={load} />
      <UserChartDialog user={chart} onClose={() => setChart(null)} />
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}


const HEALTH_META: Record<ProviderHealthItem["status"], { label: string; cls: string; dot: string; pulse?: boolean }> = {
  ok:       { label: "健康",   cls: "bg-success/10 text-success", dot: "bg-success" },
  degraded: { label: "有失败", cls: "bg-warning/10 text-warning-foreground ring-1 ring-warning/20", dot: "bg-warning" },
  down:     { label: "异常",   cls: "bg-destructive/10 text-destructive ring-1 ring-destructive/20", dot: "bg-destructive" },
  broken:   { label: "熔断中", cls: "bg-destructive/10 text-destructive ring-1 ring-destructive/20", dot: "bg-destructive", pulse: true },
  idle:     { label: "无请求", cls: "bg-muted text-muted-foreground", dot: "bg-muted-foreground/60" },
};

function HealthBadge({ h, name }: { h?: ProviderHealthItem; name?: string }) {
  if (!h) return null;
  const meta = HEALTH_META[h.status] ?? HEALTH_META.idle;
  const br = h.breaker;
  const display = name || h.provider;
  const stat = "flex items-center justify-between gap-2";
  const k = "text-muted-foreground";
  const v = "mono font-medium";
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`查看 ${display} 健康详情`}
          onClick={(e) => e.stopPropagation()}
          className={cn("inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium transition-opacity hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring", meta.cls)}
        >
          <span className={cn("h-1.5 w-1.5 rounded-full", meta.dot, meta.pulse && "animate-pulse")} />
          {meta.label}
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64">
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2">
            <span className="mono truncate text-sm font-semibold">{display}</span>
            <span className={cn("inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-semibold", meta.cls)}>
              <span className={cn("h-1 w-1 rounded-full", meta.dot, meta.pulse && "animate-pulse")} />
              {meta.label}
            </span>
          </div>
          {h.status === "idle" ? (
            <p className="text-xs text-muted-foreground">统计窗口内无请求。</p>
          ) : (
            <>
              <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                <div className={stat}><span className={k}>请求数</span><span className={v}>{h.requests.toLocaleString()}</span></div>
                <div className={stat}><span className={k}>成功率</span><span className={v}>{(h.success_rate * 100).toFixed(1)}%</span></div>
                <div className={stat}><span className={k}>平均延迟</span><span className={v}>{Math.round(h.avg_ms)}ms</span></div>
                <div className={stat}><span className={k}>P95</span><span className={v}>{h.p95_ms}ms</span></div>
                <div className={stat}><span className={k}>P99</span><span className={v}>{h.p99_ms}ms</span></div>
                <div className={stat}><span className={k}>不可用/其它</span><span className={v}>{h.fail_unavailable}/{h.fail_other}</span></div>
              </div>
              {br && (br.is_broken || br.fail_count > 0) && (
                <div className={cn("rounded-lg px-2.5 py-2 text-xs", br.is_broken ? "bg-destructive/10 text-destructive" : "bg-muted/60 text-muted-foreground")}>
                  {br.is_broken ? (
                    <>
                      <span className="font-semibold">熔断中</span>
                      <span className="ml-1">· 半开恢复探测约 {Math.max(1, Math.ceil(br.recover_remaining_ms / 1000))}s 后放行</span>
                    </>
                  ) : (
                    <>熔断计数 <span className="mono font-semibold">{br.fail_count}/{br.threshold}</span> · 连续失败达阈值即熔断</>
                  )}
                </div>
              )}
            </>
          )}
          <p className="text-[10px] leading-relaxed text-muted-foreground">统计窗口内该上游 key 的聚合指标;熔断后自动半开探测恢复。</p>
        </div>
      </PopoverContent>
    </Popover>
  );
}

function ModelsPanel() {
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [providerModels, setProviderModels] = useState<Record<string, ProviderModelRow[]>>({});
  const [addDlg, setAddDlg] = useState(false);
  const [editProvider, setEditProvider] = useState<ProviderRow | null>(null);
  const [editModel, setEditModel] = useState<ModelRow | null>(null);
  const [priceTarget, setPriceTarget] = useState<{ p: ProviderRow; m: ProviderModelRow } | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [test, setTest] = useState<Record<number, "loading" | { ok: boolean; msg: string }>>({});
  const [page, setPage] = useState(1);
  const [pageMeta, setPageMeta] = useState({ total: 0, total_pages: 1 });
  const [providerSearch, setProviderSearch] = useState("");
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [provSelected, setProvSelected] = useState<Set<string>>(new Set());
  const [health, setHealth] = useState<Record<string, ProviderHealthItem>>({});

  const loadProviders = async (p?: number) => {
    const pg = p ?? page;
    const r = await api.providers(pg);
    setProviders(r.data);
    setPageMeta({ total: r.total, total_pages: r.total_pages });
    setPage(pg);
  };
  useEffect(() => { loadProviders(1); }, []);

  // 供应商健康徽标:30s 轮询;失败不打断主列表
  const loadHealth = async () => {
    try {
      const r = await api.providerHealth();
      setHealth(Object.fromEntries(r.items.map((it) => [it.provider, it])));
    } catch { /* 忽略 */ }
  };
  useEffect(() => {
    loadHealth();
    const t = setInterval(loadHealth, 30000);
    return () => clearInterval(t);
  }, []);

  const refreshProviderModels = async (name: string) => {
    const models = (await api.providerModels(name)).data;
    setProviderModels((prev) => ({ ...prev, [name]: models }));
  };

  const toggleExpand = async (name: string) => {
    const next = new Set(expanded);
    if (next.has(name)) { next.delete(name); }
    else {
      next.add(name);
      if (!providerModels[name]) {
        const models = (await api.providerModels(name)).data;
        setProviderModels((prev) => ({ ...prev, [name]: models }));
      }
    }
    setExpanded(next);
  };

  const delProvider = (p: ProviderRow) => setConfirm({
    title: `删除上游「${providerDisplayName(p)}」?`,
    desc: `将删除该上游下的 ${p.model_count} 个模型及其所有路由，不可恢复。`,
    action: async () => { await api.deleteProvider(p.name); loadProviders(); },
  });

  // 供应商级多选批量删除:卡片头部复选框勾选,头部"删除所选"一键级联删除。
  const toggleProviderSelect = (name: string, checked: boolean) =>
    setProvSelected((s) => { const n = new Set(s); if (checked) n.add(name); else n.delete(name); return n; });

  const delSelectedProviders = () => {
    const ps = providers.filter((p) => provSelected.has(p.name));
    if (ps.length === 0) return;
    const total = ps.reduce((acc, p) => acc + p.model_count, 0);
    const names = ps.map((p) => providerDisplayName(p)).join("、");
    setConfirm({
      title: `批量删除 ${ps.length} 个上游?`,
      desc: `将删除：${names}，共 ${total} 个模型及其所有组内路由，不可恢复。`,
      action: async () => {
        for (const p of ps) await api.deleteProvider(p.name);
        // 同步清理已勾选的模型行选择,避免残留失效 id。
        const ids = ps.flatMap((p) => (providerModels[p.name] || []).map((m) => m.id));
        setSelected((s) => { const n = new Set(s); ids.forEach((id) => n.delete(id)); return n; });
        setProvSelected(new Set());
        await loadProviders();
      },
    });
  };

  const delModel = (provider: string, m: { id: number; upstream_model: string }) => setConfirm({
    title: `删除模型「${m.upstream_model}」?`,
    desc: "引用该模型的组内路由会一并删除。",
    action: async () => {
      await api.deleteModel(m.id);
      setSelected((s) => { const n = new Set(s); n.delete(m.id); return n; });
      await loadProviders();
      if (expanded.has(provider)) await refreshProviderModels(provider);
    },
  });

  const toggleSelect = (id: number, checked: boolean) =>
    setSelected((s) => { const n = new Set(s); if (checked) n.add(id); else n.delete(id); return n; });

  const toggleSelectAll = (name: string, checked: boolean) => {
    const ids = (providerModels[name] || []).map((m) => m.id);
    setSelected((s) => {
      const n = new Set(s);
      ids.forEach((id) => (checked ? n.add(id) : n.delete(id)));
      return n;
    });
  };

  const selectedInProvider = (name: string) =>
    (providerModels[name] || []).filter((m) => selected.has(m.id));

  const delSelected = (name: string, ms: { id: number; upstream_model: string }[]) => {
    const ids = ms.map((m) => m.id);
    setConfirm({
      title: `批量删除 ${ids.length} 个模型?`,
      desc: `将删除：${ms.map((m) => m.upstream_model).join("、")}。引用它们的组内路由会一并删除，若供应商下模型被删空则供应商也会一并删除，不可恢复。`,
      action: async () => {
        await api.deleteModelsBatch(ids);
        setSelected((s) => { const n = new Set(s); ids.forEach((id) => n.delete(id)); return n; });
        await loadProviders();
        if (expanded.has(name)) await refreshProviderModels(name);
      },
    });
  };

  const runTest = async (id: number) => {
    setTest((t) => ({ ...t, [id]: "loading" }));
    try {
      const r = await api.testModel(id);
      setTest((t) => ({ ...t, [id]: { ok: r.ok, msg: r.ok ? `${r.latency_ms}ms` : (r.error || "失败") } }));
    } catch (e: any) { setTest((t) => ({ ...t, [id]: { ok: false, msg: e.message } })); }
  };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">上游供应商({pageMeta.total})</CardTitle>
            <div className="flex items-center gap-2">
              {/* 常显:配合卡片头部复选框做供应商级批量删除,未勾选时置灰。 */}
              <Button variant="destructive" size="sm" disabled={provSelected.size === 0} onClick={delSelectedProviders}>
                <Trash2 className="h-4 w-4" />删除所选({provSelected.size})
              </Button>
              <Button size="sm" onClick={() => setAddDlg(true)}><Plus className="h-4 w-4" />添加上游</Button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <input value={providerSearch} onChange={(e) => setProviderSearch(e.target.value)} placeholder="搜索供应商名称或地址..." className="h-8 w-full rounded-lg border border-input bg-card pl-7 pr-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring" />
          </div>
          {providers.length === 0 && <p className="text-sm text-muted-foreground">暂无上游供应商，点击「添加上游」开始</p>}
          {providers.filter((p) => !providerSearch.trim() || p.name.toLowerCase().includes(providerSearch.toLowerCase()) || p.base_url.toLowerCase().includes(providerSearch.toLowerCase())).map((p) => (
            <div key={p.name} className="rounded-lg border">
              {/* Provider 头部 */}
              <div className="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-muted/50" onClick={() => toggleExpand(p.name)}>
                <div className="flex items-center gap-3">
                  <input type="checkbox" className="h-4 w-4 shrink-0 rounded border-input"
                    title="勾选该上游(配合顶部删除所选批量删除)"
                    aria-label={`选择上游: ${providerDisplayName(p)}`}
                    checked={provSelected.has(p.name)}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => toggleProviderSelect(p.name, e.target.checked)} />
                  <Zap className="h-4 w-4 text-primary" />
                  <div>
                    <div className="flex items-center gap-2 text-sm font-medium">
                      {providerDisplayName(p)}
                      <HealthBadge h={health[p.name]} name={providerDisplayName(p)} />
                    </div>
                    <div className="text-xs text-muted-foreground">{p.base_url}</div>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <Badge variant="muted">{p.kind}</Badge>
                  <span className="text-xs text-muted-foreground">{p.model_count} 个模型</span>
                  <button className="text-xs text-primary hover:underline" onClick={(e) => { e.stopPropagation(); setEditProvider(p); }}>编辑</button>
                  <button className="text-xs text-destructive hover:underline" onClick={(e) => { e.stopPropagation(); delProvider(p); }}>删除</button>
                </div>
              </div>
              {/* 展开的模型列表 */}
              {expanded.has(p.name) && (
                <div className="border-t">
                  {providerModels[p.name]?.length === 0 && (
                    <p className="px-4 py-2 text-xs text-muted-foreground">该供应商下暂无模型</p>
                  )}
                  {(providerModels[p.name]?.length ?? 0) > 0 && (
                    <div className="flex items-center justify-between px-4 py-2 bg-muted/30">
                      <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
                        <input type="checkbox" className="h-4 w-4 rounded border-input"
                          checked={selectedInProvider(p.name).length === (providerModels[p.name]?.length ?? 0)}
                          onChange={(e) => toggleSelectAll(p.name, e.target.checked)} />
                        全选
                      </label>
                      {/* 常显:让"可多选批量删除"一眼可见,未勾选时置灰。 */}
                      <Button variant="destructive" size="sm" className="h-7 text-xs" disabled={selectedInProvider(p.name).length === 0}
                        onClick={() => delSelected(p.name, selectedInProvider(p.name))}>
                        <Trash2 className="h-3 w-3" />删除所选({selectedInProvider(p.name).length})
                      </Button>
                    </div>
                  )}
                  {providerModels[p.name]?.map((m) => (
                    <div key={m.id} className="flex items-center justify-between px-4 py-2 border-t first:border-t-0">
                      <div className="flex items-center gap-3">
                        <input type="checkbox" className="h-4 w-4 rounded border-input"
                          checked={selected.has(m.id)} onChange={(e) => toggleSelect(m.id, e.target.checked)} />
                        <Boxes className="h-3.5 w-3.5 text-muted-foreground" />
                        <span className="text-sm">{m.label || m.upstream_model}</span>
                        {m.label && <span className="text-xs text-muted-foreground">{m.upstream_model}</span>}
                        {m.input_price != null && m.output_price != null ? (
                          <span className="font-mono text-xs text-muted-foreground" title="每 1M tokens 输入/输出单价">${m.input_price.toFixed(2)} / ${m.output_price.toFixed(2)}</span>
                        ) : (
                          <Badge variant="muted" className="border-amber-500/30 bg-amber-500/15 text-amber-600" title="未定价模型按内置默认价表计费,成本统计可能偏低;点击右侧 $ 按钮设置">未定价</Badge>
                        )}
                      </div>
                      <div className="flex items-center gap-2">
                        {(() => {
                          const r = test[m.id];
                          if (r === "loading") return <span className="text-xs text-muted-foreground">校验中…</span>;
                          if (r) return <span className={cn("text-xs", r.ok ? "text-success" : "text-destructive")} title={r.msg}>{r.ok ? `✓ ${r.msg}` : `✗ ${r.msg}`}</span>;
                          return null;
                        })()}
                        <Button variant="ghost" size="sm" className="h-7 px-2" title="测试连通性" onClick={() => runTest(m.id)}><Activity className="h-3 w-3" /></Button>
                        <Button variant="ghost" size="sm" className="h-7 px-2" title="模型定价" onClick={() => setPriceTarget({ p, m })}><DollarSign className="h-3 w-3" /></Button>
                        <Button variant="ghost" size="sm" className="h-7 px-2" title="编辑模型" onClick={() => setEditModel({ id: m.id, label: m.label, kind: p.kind, base_url: p.base_url, upstream_model: m.upstream_model, provider: p.name, input_price: m.input_price, output_price: m.output_price, context_length: m.context_length, tags: m.tags })}><Pencil className="h-3 w-3" /></Button>
                        <Button variant="ghost" size="sm" className="h-7 px-2 text-destructive" title="删除模型" onClick={() => delModel(p.name, m)}><Trash2 className="h-3 w-3" /></Button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
          {pageMeta.total_pages > 1 && (
            <div className="flex items-center justify-center gap-2 pt-3">
              <Button variant="outline" size="sm" disabled={page <= 1} onClick={() => loadProviders(page - 1)}>上一页</Button>
              <span className="text-xs text-muted-foreground">{page}/{pageMeta.total_pages}</span>
              <Button variant="outline" size="sm" disabled={page >= pageMeta.total_pages} onClick={() => loadProviders(page + 1)}>下一页</Button>
            </div>
          )}
        </CardContent>
      </Card>
      <AddProviderDialog open={addDlg} onClose={() => setAddDlg(false)} onSaved={loadProviders} />
      {editProvider && <EditProviderDialog provider={editProvider} onClose={() => setEditProvider(null)} onSaved={() => { setEditProvider(null); loadProviders(); }} />}
      {editModel && <EditModelDialog model={editModel} onClose={() => setEditModel(null)} onSaved={() => {
        // 保存成功后必须同步刷新展开中的模型列表,否则界面停留在旧数据,看起来像"修改不生效"。
        const prov = editModel.provider;
        setEditModel(null);
        loadProviders();
        if (prov && expanded.has(prov)) refreshProviderModels(prov);
      }} />}
      {priceTarget && <PricingDialog provider={priceTarget.p} model={priceTarget.m} onClose={() => setPriceTarget(null)} onSaved={() => { setPriceTarget(null); if (expanded.has(priceTarget.p.name)) refreshProviderModels(priceTarget.p.name); }} />}
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

// ---- 模型挑选列表(添加上游/编辑上游共用):筛选 + 全选 + 已存在标记 ----
function ModelPickList({ models, selected, setSelected, existing }: {
  models: string[];
  selected: Set<string>;
  setSelected: React.Dispatch<React.SetStateAction<Set<string>>>;
  existing?: Set<string>;
}) {
  const [filter, setFilter] = useState("");
  const shown = models.filter((m) => m.toLowerCase().includes(filter.trim().toLowerCase()));
  const toggle = (m: string) => setSelected((prev) => { const n = new Set(prev); if (n.has(m)) n.delete(m); else n.add(m); return n; });
  const toggleAll = () => {
    const allSel = shown.length > 0 && shown.every((m) => selected.has(m));
    setSelected((prev) => { const n = new Set(prev); for (const m of shown) { if (allSel) n.delete(m); else n.add(m); } return n; });
  };
  return (
    <div className="rounded-lg border bg-card">
      <div className="flex items-center justify-between gap-2 border-b px-3 py-2">
        <span className="shrink-0 text-sm font-medium">勾选模型 ({selected.size}/{models.length})</span>
        <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="筛选模型名…" className="h-7 flex-1 text-xs" />
        <button className="shrink-0 text-xs text-primary hover:underline" onClick={toggleAll}>{shown.length > 0 && shown.every((m) => selected.has(m)) ? "取消全选" : "全选"}</button>
      </div>
      <div className="max-h-72 overflow-y-auto">
        {shown.map((m) => (
          <label key={m} className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm hover:bg-accent">
            <input type="checkbox" checked={selected.has(m)} onChange={() => toggle(m)} className="h-4 w-4 rounded border-input" />
            <span className="flex-1">{m}</span>
            {existing?.has(m) && <span className="shrink-0 text-[10px] text-muted-foreground">已存在</span>}
            {selected.has(m) && <Check className="h-4 w-4 text-primary" />}
          </label>
        ))}
        {shown.length === 0 && <p className="px-3 py-2 text-xs text-muted-foreground">无匹配模型</p>}
      </div>
    </div>
  );
}

// ---- 编辑供应商对话框 ----
function EditProviderDialog({ provider, onClose, onSaved }: { provider: ProviderRow; onClose: () => void; onSaved: () => void }) {
  const [dn, setDn] = useState(provider.display_name || "");
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [apiKey, setApiKey] = useState("");
  const [err, setErr] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [existing, setExisting] = useState<Set<string>>(new Set());
  const [pickSelected, setPickSelected] = useState<Set<string>>(new Set());
  const [fetching, setFetching] = useState(false);
  const [adding, setAdding] = useState(false);
  const [pickMsg, setPickMsg] = useState("");

  const submit = async () => {
    setErr(""); setSubmitting(true);
    try {
      await api.updateProvider(provider.name, { base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, display_name: dn.trim() });
      onSaved();
    } catch (e: any) { setErr(e.message); }
    setSubmitting(false);
  };

  // 从上游拉取最新模型列表;key 留空时后端自动回退用该供应商存储的密钥。
  const doFetchModels = async () => {
    setFetching(true); setPickMsg(""); setPickSelected(new Set());
    try {
      const r = await api.fetchModelList({ kind: provider.kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, provider: provider.name });
      if (r.ok && r.models) {
        setFetchedModels(r.models);
        const cur = await api.providerModels(provider.name);
        setExisting(new Set(cur.data.map((m) => m.upstream_model)));
      } else { setPickMsg(r.error || "获取失败"); setFetchedModels([]); }
    } catch (e: any) { setPickMsg(e.message); setFetchedModels([]); }
    setFetching(false);
  };

  // 把勾选的模型增量添加到本供应商(已存在的自动跳过)。
  const addPicked = async () => {
    if (pickSelected.size === 0) return;
    setAdding(true); setPickMsg("");
    try {
      const items = [...pickSelected].map((m) => ({ upstream_model: m, label: m }));
      const r = await api.addModelsToProvider(provider.name, items);
      setPickMsg(`已添加 ${r.added ?? 0} 个,跳过已存在 ${r.skipped ?? 0} 个`);
      setPickSelected(new Set());
      const cur = await api.providerModels(provider.name);
      setExisting(new Set(cur.data.map((m) => m.upstream_model)));
      onSaved();
    } catch (e: any) { setPickMsg(e.message); }
    setAdding(false);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader><DialogTitle>编辑供应商</DialogTitle><DialogDescription>{provider.name}</DialogDescription></DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">名称 (留空按域名显示)</label>
            <Input value={dn} onChange={(e) => setDn(e.target.value)} placeholder="如: 生产网关 / fit2cloud 主力" /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">Base URL</label>
            <Input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">API Key</label>
            <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="留空保持原密钥" /></div>
          {err && <p className="text-sm text-destructive">{err}</p>}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={onClose}>取消</Button>
            <Button onClick={submit} disabled={submitting}>{submitting ? "保存中..." : "保存"}</Button>
          </div>
          <div className="border-t pt-3">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-sm font-medium">上游模型</span>
              <Button type="button" variant="outline" size="sm" onClick={doFetchModels} disabled={fetching || !baseUrl.trim()}>
                <Search className="h-3.5 w-3.5 mr-1" />{fetching ? "获取中..." : fetchedModels.length > 0 ? "重新获取" : "获取模型列表"}
              </Button>
            </div>
            {fetchedModels.length > 0 && (
              <>
                <ModelPickList models={fetchedModels} selected={pickSelected} setSelected={setPickSelected} existing={existing} />
                <div className="mt-2 flex items-center justify-between gap-2">
                  <span className="text-xs text-muted-foreground">{pickMsg || "已存在的模型会自动跳过"}</span>
                  <Button size="sm" onClick={addPicked} disabled={adding || pickSelected.size === 0}>
                    <Plus className="h-3.5 w-3.5 mr-1" />{adding ? "添加中..." : `添加所选 (${pickSelected.size})`}
                  </Button>
                </div>
              </>
            )}
            {pickMsg && fetchedModels.length === 0 && <p className="text-xs text-destructive">{pickMsg}</p>}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ---- 添加上游对话框 ----
function AddProviderDialog({ open, onClose, onSaved }: { open: boolean; onClose: () => void; onSaved: () => void }) {
  const [kind, setKind] = useState("openai");
  const [provName, setProvName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [selectedModels, setSelectedModels] = useState<Set<string>>(new Set());
  const [fetching, setFetching] = useState(false);
  const [err, setErr] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [inputPrice, setInputPrice] = useState("");
  const [outputPrice, setOutputPrice] = useState("");
  const [tpl, setTpl] = useState("custom");

  // 每次打开都重置为全新状态,避免上一次的填写与探测结果残留。
  useEffect(() => {
    if (!open) return;
    setProvName(""); setKind("openai"); setTpl("custom"); setBaseUrl(""); setApiKey("");
    setFetchedModels([]); setSelectedModels(new Set()); setErr(""); setInputPrice(""); setOutputPrice("");
  }, [open]);

  const TEMPLATES: Record<string, { kind: string; base_url: string }> = {
    custom: { kind: "", base_url: "" },
    anthropic: { kind: "anthropic", base_url: "https://api.anthropic.com" },
    openai: { kind: "openai", base_url: "https://api.openai.com/v1" },
    deepseek: { kind: "openai", base_url: "https://api.deepseek.com/v1" },
    qwen: { kind: "openai", base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
    zhipu: { kind: "openai", base_url: "https://open.bigmodel.cn/api/paas/v4" },
    moonshot: { kind: "openai", base_url: "https://api.moonshot.cn/v1" },
    ollama: { kind: "openai", base_url: "http://localhost:11434/v1" },
  };
  const applyTemplate = (key: string) => {
    setTpl(key);
    if (key !== "custom") { const t = TEMPLATES[key]; setKind(t.kind); setBaseUrl(t.base_url); }
  };

  const doFetch = async () => {
    if (!baseUrl.trim()) { setErr("请先填写 Base URL"); return; }
    setFetching(true); setErr(""); setSelectedModels(new Set());
    try {
      const r = await api.fetchModelList({ kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined });
      if (r.ok && r.models) setFetchedModels(r.models);
      else { setErr(r.error || "获取失败"); setFetchedModels([]); }
    } catch (e: any) { setErr(e.message); setFetchedModels([]); }
    setFetching(false);
  };

  const submit = async () => {
    setErr(""); setSubmitting(true);
    try {
      const ip = inputPrice.trim() === "" ? null : Number(inputPrice);
      const op = outputPrice.trim() === "" ? null : Number(outputPrice);
      if ((ip != null && (!Number.isFinite(ip) || ip < 0)) || (op != null && (!Number.isFinite(op) || op < 0))) {
        setErr("单价必须是非负数字"); setSubmitting(false); return;
      }
      if (selectedModels.size > 0) {
        const items = [...selectedModels].map((m) => ({ upstream_model: m, label: m, input_price: ip, output_price: op }));
        await api.addModelsBatch({ kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, display_name: provName.trim() || undefined, models: items });
      } else {
        await api.addModel({ kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, upstream_model: "custom-model", input_price: ip, output_price: op, display_name: provName.trim() || undefined });
      }
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
    setSubmitting(false);
  };

  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className={fetchedModels.length > 0 ? "max-w-2xl" : ""}>
        <DialogHeader>
          <DialogTitle>添加上游</DialogTitle>
          <DialogDescription>选择供应商后可探测模型列表并批量添加</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">名称 (可选,留空按域名显示)</label>
            <Input placeholder="如: 生产网关 / fit2cloud 主力" value={provName} onChange={(e) => setProvName(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">供应商模板</label>
            <select value={tpl} onChange={(e) => applyTemplate(e.target.value)} className={sel}>
              <option value="custom">自定义</option>
              <option value="deepseek">DeepSeek</option>
              <option value="openai">OpenAI</option>
              <option value="anthropic">Anthropic</option>
              <option value="qwen">通义千问</option>
              <option value="zhipu">智谱 GLM</option>
              <option value="moonshot">Moonshot Kimi</option>
              <option value="ollama">Ollama 本地</option>
            </select></div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">协议</label>
              <select value={kind} onChange={(e) => setKind(e.target.value)} className={sel}>
                <option value="openai">openai 兼容</option><option value="anthropic">anthropic</option>
              </select></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">Base URL</label>
              <Input placeholder="https://api.deepseek.com/v1" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} /></div>
          </div>
          <div><label className="mb-1 block text-xs text-muted-foreground">API Key (可选)</label>
            <Input type="password" placeholder="sk-..." value={apiKey} onChange={(e) => setApiKey(e.target.value)} /></div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">输入单价 $/1M tokens (可选)</label>
              <Input type="number" min="0" step="0.000001" placeholder="留空用内置默认价表" value={inputPrice} onChange={(e) => setInputPrice(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">输出单价 $/1M tokens (可选)</label>
              <Input type="number" min="0" step="0.000001" placeholder="留空用内置默认价表" value={outputPrice} onChange={(e) => setOutputPrice(e.target.value)} /></div>
          </div>
          <Button type="button" variant="outline" size="sm" onClick={doFetch} disabled={fetching || !baseUrl.trim()} className="w-full">
            <Search className="h-4 w-4 mr-1" />{fetching ? "探测中..." : "探测模型列表"}
          </Button>
          {fetchedModels.length > 0 && <ModelPickList models={fetchedModels} selected={selectedModels} setSelected={setSelectedModels} />}
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={submitting || (fetchedModels.length > 0 && selectedModels.size === 0)}>
            {submitting ? "提交中..." : selectedModels.size > 0 ? `添加 (${selectedModels.size} 个模型)` : "添加"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ---- 编辑单个模型对话框 ----
function EditModelDialog({ model, onClose, onSaved }: { model: ModelRow; onClose: () => void; onSaved: () => void }) {
  const [label, setLabel] = useState(model.label ?? "");
  const [upstream, setUpstream] = useState(model.upstream_model);
  const [ctxLen, setCtxLen] = useState(model.context_length != null ? String(model.context_length) : "");
  const [tags, setTags] = useState(model.tags ?? "");
  const [err, setErr] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const submit = async () => {
    setErr("");
    const t = ctxLen.trim();
    if (t && (!isFinite(Number(t)) || Number(t) < 0)) { setErr("上下文长度必须是 ≥ 0 的整数"); return; }
    try {
      await api.updateModel(model.id, {
        kind: model.kind || "openai", base_url: model.base_url || "",
        upstream_model: upstream.trim(), label: label.trim() || undefined,
        context_length: t ? Number(t) : null,
        tags: tags.trim().replace(/,+$/, "") || undefined,
      });
      onSaved();
    } catch (e: any) { setErr(e.message); }
  };

  const doTest = async () => {
    setTesting(true); setTestResult(null);
    try {
      const r = await api.testModel(model.id);
      setTestResult({ ok: r.ok, msg: r.ok ? `${r.latency_ms}ms` : (r.error || "失败") });
    } catch (e: any) { setTestResult({ ok: false, msg: e.message }); }
    setTesting(false);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader><DialogTitle>编辑模型</DialogTitle></DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">上游模型名</label>
            <Input value={upstream} onChange={(e) => setUpstream(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">备注(门户模型广场展示名)</label>
            <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="留空则不显示备注" /></div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">上下文长度 (tokens,可选)</label>
              <Input type="number" min="0" step="1" placeholder="如 128000" value={ctxLen} onChange={(e) => setCtxLen(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">标签 (逗号分隔,可选)</label>
              <Input placeholder="如 视觉,推理" value={tags} onChange={(e) => setTags(e.target.value)} /></div>
          </div>
          <p className="text-xs text-muted-foreground">标签与上下文长度展示在门户「模型广场」；标签留空时按模型名自动推断(视觉/推理/向量/语音/对话)。</p>
          <Button variant="outline" size="sm" onClick={doTest} disabled={testing} className="w-full">
            <Activity className="h-4 w-4 mr-1" />{testing ? "测试中..." : "测试连通性"}
          </Button>
          {testResult && <p className={`text-xs ${testResult.ok ? "text-green-600" : "text-destructive"}`}>{testResult.ok ? `连通正常 (${testResult.msg})` : testResult.msg}</p>}
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit}>保存</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ---- 模型定价对话框 ----
function PricingDialog({ provider, model, onClose, onSaved }: { provider: ProviderRow; model: ProviderModelRow; onClose: () => void; onSaved: () => void }) {
  const [inputPrice, setInputPrice] = useState(model.input_price != null ? String(model.input_price) : "");
  const [outputPrice, setOutputPrice] = useState(model.output_price != null ? String(model.output_price) : "");
  const [err, setErr] = useState("");
  const [submitting, setSubmitting] = useState(false);

  // 空串=清除定价(null);其余解析为数字,非法返回 "invalid"
  const parsePrice = (s: string): number | null | "invalid" => {
    const t = s.trim();
    if (!t) return null;
    const v = Number(t);
    return isFinite(v) && v >= 0 ? v : "invalid";
  };

  const submit = async () => {
    const i = parsePrice(inputPrice);
    const o = parsePrice(outputPrice);
    if (i === "invalid" || o === "invalid") { setErr("单价必须是 ≥ 0 的数字"); return; }
    setErr(""); setSubmitting(true);
    try {
      await api.updateModel(model.id, {
        kind: provider.kind, base_url: provider.base_url,
        upstream_model: model.upstream_model, label: model.label ?? undefined,
        input_price: i, output_price: o,
      });
      onSaved();
    } catch (e: any) { setErr(e.message); setSubmitting(false); }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>模型定价</DialogTitle>
          <DialogDescription>为「{model.upstream_model}」设置单价，按每 1M tokens 计费</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">输入单价 ($/1M tokens)</label>
              <div className="relative">
                <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-muted-foreground">$</span>
                <Input className="pl-6" type="number" min="0" step="0.01" placeholder="未定价" value={inputPrice} onChange={(e) => setInputPrice(e.target.value)} />
              </div>
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">输出单价 ($/1M tokens)</label>
              <div className="relative">
                <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-muted-foreground">$</span>
                <Input className="pl-6" type="number" min="0" step="0.01" placeholder="未定价" value={outputPrice} onChange={(e) => setOutputPrice(e.target.value)} />
              </div>
            </div>
          </div>
          <p className="text-xs text-muted-foreground">留空表示清除定价；未手动定价的模型按内置默认价表计费（仅对已知模型生效）。</p>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={submitting}>{submitting ? "保存中..." : "保存"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function GroupsPanel() {
  const [groups, setGroups] = useState<GroupRow[]>([]);
  const [models, setModels] = useState<ModelRow[]>([]);
  const [sel, setSel] = useState<number | null>(null);
  const [routes, setRoutes] = useState<RouteRow[]>([]);
  const [timeRules, setTimeRules] = useState<TimeRuleRow[]>([]);
  const [gq, setGq] = useState("");
  const [groupOpen, setGroupOpen] = useState(false);
  const [renameDlg, setRenameDlg] = useState<GroupRow | null>(null);
  const [routeDlg, setRouteDlg] = useState<{ edit: RouteRow | null } | null>(null);
  const [timeRuleDlg, setTimeRuleDlg] = useState<{ edit: TimeRuleRow | null } | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [importOpen, setImportOpen] = useState(false);
  const [batchDlg, setBatchDlg] = useState<"multiplier" | "weight" | null>(null);

  const loadGroups = async () => {
    const [g, m] = await Promise.all([api.groups(), api.models()]);
    setGroups(g.data); setModels(m.data);
    if (sel == null && g.data.length) setSel(g.data[0].id);
  };
  const loadRoutes = async (gid: number) => { setRoutes((await api.routes(gid)).data); };
  const loadTimeRules = async (gid: number) => { setTimeRules((await api.timeRules(gid)).data); };
  useEffect(() => { loadGroups(); }, []);
  useEffect(() => { if (sel != null) { loadRoutes(sel); loadTimeRules(sel); } setChecked(new Set()); }, [sel]);

  const createGroup = async (name: string) => { const r = await api.addGroup(name); await loadGroups(); setSel(r.id); };
  const setActive = async (g: GroupRow) => { if (!g.is_active) { await api.activateGroup(g.id); loadGroups(); } };
  const delGroup = (g: GroupRow) => setConfirm({
    title: `删除模型组「${g.name}」?`,
    desc: "组内路由会一并删除,绑定该组的用户将无可用模型。",
    action: async () => { await api.deleteGroup(g.id); setSel(null); loadGroups(); },
  });
  const delRoute = (id: number) => setConfirm({
    title: "删除该路由?",
    action: async () => { await api.deleteRoute(id); if (sel != null) loadRoutes(sel); },
  });
  const downloadJson = (data: unknown, filename: string) => {
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
  };
  const exportAll = async () =>
    downloadJson(await api.groupsExport(), `relay-groups-${new Date().toISOString().slice(0, 10)}.json`);
  const exportGroup = async (gid: number) => {
    const g = groups.find((x) => x.id === gid);
    downloadJson(await api.groupExport(gid), `relay-group-${g?.name ?? gid}.json`);
  };
  const toggleChecked = (id: number) => setChecked((prev) => {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });
  const delRoutesBatch = () => {
    const ids = [...checked];
    if (!ids.length) return;
    setConfirm({
      title: `删除选中的 ${ids.length} 条路由?`,
      desc: "删除后用户请求这些对外模型名将无法路由。",
      action: async () => { await api.batchDeleteRoutes(ids); setChecked(new Set()); if (sel != null) loadRoutes(sel); },
    });
  };
  const applyBatch = async (field: "multiplier" | "weight", val: number) => {
    await api.batchUpdateRoutes([...checked], field === "multiplier" ? { multiplier: val } : { weight: val });
    setChecked(new Set());
    if (sel != null) loadRoutes(sel);
  };
  const setStrategy = async (gid: number, strategy: string) => {
    await api.setGroupStrategy(gid, strategy);
    await loadGroups();
  };
  const delTimeRule = (id: number) => setConfirm({
    title: "删除该时段规则?",
    desc: "删除后该时段不再生效(回到默认倍率与权重)。",
    action: async () => { if (sel != null) { await api.deleteTimeRule(sel, id); loadTimeRules(sel); } },
  });
  const STRATEGY_LABEL: Record<string, string> = {
    weighted_random: "加权随机（默认）",
    weighted_round_robin: "加权轮询",
    round_robin: "简单轮询",
    priority: "优先级（故障转移链）",
    cost_aware: "成本优先",
    latency_aware: "延迟优先",
  };
  const curStrategy = groups.find((g) => g.id === sel)?.strategy ?? "weighted_random";
  const filteredGroups = groups.filter((g) => !gq.trim() || g.name.includes(gq.trim()));

  return (
    <div className="grid h-full min-h-0 gap-5 md:grid-cols-[260px_1fr]">
      <Card className="flex h-full min-h-0 flex-col">
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <CardTitle className="text-base">模型组</CardTitle>
            <div className="flex items-center gap-0.5">
              <button className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="导出全部模型组(JSON)" onClick={exportAll}><Download className="h-4 w-4" /></button>
              <button className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="导入模型组(JSON)" onClick={() => setImportOpen(true)}><Upload className="h-4 w-4" /></button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="min-h-0 flex-1 space-y-3 overflow-auto">
          <div className="flex gap-2">
            <Input placeholder="搜索模型组" value={gq} onChange={(e) => setGq(e.target.value)} />
            <Button size="icon" onClick={() => setGroupOpen(true)}><Plus className="h-4 w-4" /></Button>
          </div>
          <div className="space-y-1">
            {filteredGroups.map((g) => (
              <div key={g.id} className={cn("group flex items-center justify-between gap-2 rounded-lg px-3 py-2 text-sm cursor-pointer",
                sel === g.id ? "bg-accent font-medium" : "hover:bg-accent")} onClick={() => setSel(g.id)}>
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate">{g.name}</span>
                  {g.is_active && <Badge variant="success" className="shrink-0 text-[10px]">默认</Badge>}
                </span>
                <span className="flex shrink-0 items-center gap-1">
                  <button title={g.is_active ? "当前默认组" : "设为默认(新用户注册自动绑定)"} aria-label={`${g.is_active ? "当前默认组" : "设为默认"}: ${g.name}`}
                    className={cn("transition-colors", g.is_active ? "text-amber-500" : "text-muted-foreground hover:text-amber-500")}
                    onClick={(e) => { e.stopPropagation(); setActive(g); }}>
                    <Star className={cn("h-3.5 w-3.5", g.is_active && "fill-current")} />
                  </button>
                  <button title="重命名模型组" aria-label={`重命名模型组: ${g.name}`} className="text-muted-foreground hover:text-foreground"
                    onClick={(e) => { e.stopPropagation(); setRenameDlg(g); }}><Pencil className="h-3.5 w-3.5" /></button>
                  <button title="删除模型组" aria-label={`删除模型组: ${g.name}`} className="text-muted-foreground hover:text-destructive" onClick={(e) => { e.stopPropagation(); delGroup(g); }}><Trash2 className="h-3.5 w-3.5" /></button>
                </span>
              </div>
            ))}
            {filteredGroups.length === 0 && <p className="px-1 text-sm text-muted-foreground">{groups.length === 0 ? "还没有模型组" : "无匹配"}</p>}
          </div>
        </CardContent>
      </Card>

      <Card className="flex h-full min-h-0 flex-col">
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">组内路由 {sel != null && `· ${groups.find((g) => g.id === sel)?.name ?? ""}`}</CardTitle>
            <div className="flex items-center gap-2">
              {sel != null && (
                <div className="flex items-center gap-1.5">
                  <span className="text-xs text-muted-foreground">负载策略</span>
                  <select value={curStrategy} onChange={(e) => setStrategy(sel, e.target.value)} className="h-8 rounded-lg border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring">
                    <option value="weighted_random">加权随机</option>
                    <option value="weighted_round_robin">加权轮询</option>
                    <option value="round_robin">简单轮询</option>
                    <option value="priority">优先级（故障转移链）</option>
                    <option value="cost_aware">成本优先</option>
                    <option value="latency_aware">延迟优先</option>
                  </select>
                </div>
              )}
              {sel != null && (
                <>
                  <Button size="sm" variant="outline" title="导出该组(JSON)" onClick={() => exportGroup(sel)}><Download className="h-4 w-4" /></Button>
                  <Button size="sm" onClick={() => setRouteDlg({ edit: null })}><Plus className="h-4 w-4" />添加路由</Button>
                </>
              )}
            </div>
          </div>
          {curStrategy === "priority" && <p className="mt-1 text-[11px] text-muted-foreground">优先级模式:数字越大越优先;主模型故障时自动切换到下一个。</p>}
          {curStrategy === "cost_aware" && <p className="mt-1 text-[11px] text-muted-foreground">成本优先模式:按 单价×倍率 从低到高选择;未定价模型自动殿后,同价随机分摊。</p>}
          {curStrategy === "latency_aware" && <p className="mt-1 text-[11px] text-muted-foreground">延迟优先模式:按窗口内 P50 延迟从低到高选择;无数据候选自动殿后随机分摊,快照每 15 秒刷新一次。</p>}
        </CardHeader>
        <CardContent className="min-h-0 flex-1 overflow-auto">
          {sel == null ? <p className="text-sm text-muted-foreground">先选择左侧一个模型组</p> : (
            <>
              <div className="mb-4 rounded-lg border border-border/60 bg-card/40 p-3">
                <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Clock className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">高峰/低谷时段策略</span>
                    <span className="hidden text-[11px] text-muted-foreground sm:inline">按 星期+时间段 驱动 计费倍率 与 路由权重</span>
                  </div>
                  <Button size="sm" variant="outline" onClick={() => setTimeRuleDlg({ edit: null })}><Plus className="h-4 w-4" />添加时段</Button>
                </div>
                {timeRules.length === 0 ? (
                  <p className="text-xs text-muted-foreground">尚未配置时段规则。未命中时段时,使用默认倍率 ×1.0 与默认权重。</p>
                ) : (
                  <div className="space-y-1.5">
                    {timeRules.map((tr) => (
                      <div key={tr.id} className="flex items-center gap-2 rounded-md border border-border/60 px-2 py-1.5">
                        <span className={cn("h-2 w-2 shrink-0 rounded-full", tr.active ? "bg-emerald-500" : "bg-muted")} />
                        <span className="min-w-0 flex-1">
                          <span className="flex items-center gap-2">
                            <span className="text-sm font-medium">{tr.name}</span>
                            <Badge variant={tr.multiplier === 1 ? "muted" : "default"}>×{tr.multiplier}</Badge>
                          </span>
                          <span className="block truncate text-[11px] text-muted-foreground">
                            {tr.weekdays === "0-6" ? "每天" : "星期" + tr.weekdays} {tr.start_time}~{tr.end_time}
                            {!tr.active && <span className="ml-1 text-destructive">(停用)</span>}
                          </span>
                        </span>
                        <span className="hidden shrink-0 text-[11px] text-muted-foreground md:inline">
                          {tr.weight_map && tr.weight_map !== "{}" ? "含权重覆盖" : "仅倍率"}
                        </span>
                        <button className="text-muted-foreground transition-colors hover:text-foreground" title="编辑" onClick={() => setTimeRuleDlg({ edit: tr })}><Pencil className="h-3.5 w-3.5" /></button>
                        <button className="text-muted-foreground transition-colors hover:text-destructive" title="删除" onClick={() => delTimeRule(tr.id)}><Trash2 className="h-3.5 w-3.5" /></button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              {checked.size > 0 ? (
                <div className="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-primary/30 bg-primary/5 px-3 py-2">
                  <Badge>已选 {checked.size} 条</Badge>
                  <Button size="sm" variant="outline" onClick={() => setBatchDlg("multiplier")}>批量改倍率</Button>
                  <Button size="sm" variant="outline" onClick={() => setBatchDlg("weight")}>批量改权重</Button>
                  <Button size="sm" variant="destructive" onClick={delRoutesBatch}><Trash2 className="h-4 w-4" />批量删除</Button>
                  <Button size="sm" variant="ghost" onClick={() => setChecked(new Set())}>取消选择</Button>
                </div>
              ) : (
                <p className="mb-3 text-xs text-muted-foreground">用户请求「对外模型名」→ 路由到指定模型;响应里保留用户传的名字。同名多条按权重/优先级分流。</p>
              )}
              <Table>
                <TableHeader><TableRow>
                  <TableHead className="w-9 pr-0">
                    <input type="checkbox" className="h-3.5 w-3.5 accent-primary" title="全选本组路由"
                      checked={routes.length > 0 && checked.size === routes.length}
                      onChange={(e) => setChecked(e.target.checked ? new Set(routes.map((r) => r.id)) : new Set())} />
                  </TableHead>
                  <TableHead>对外模型名</TableHead><TableHead>→ 实际模型</TableHead>
                  <TableHead>{curStrategy === "priority" ? "优先级" : "权重"}</TableHead>
                  <TableHead>倍率</TableHead><TableHead>缓存</TableHead><TableHead className="text-right">操作</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {(() => {
                    // 优先级模式:按权重降序排列,同名路由形成故障转移链
                    const sorted = curStrategy === "priority"
                      ? [...routes].sort((a, b) => b.weight - a.weight || a.id - b.id)
                      : routes;
                    return sorted.map((r, idx) => {
                      // 故障链指示:同名路由之间的连接线
                      const prevSameName = idx > 0 && sorted[idx - 1].public_name === r.public_name;
                      return (
                        <TableRow key={r.id} data-checked={checked.has(r.id) || undefined} className={cn(checked.has(r.id) && "bg-primary/5")}>
                          <TableCell>
                            <input type="checkbox" className="h-3.5 w-3.5 accent-primary" checked={checked.has(r.id)} onChange={() => toggleChecked(r.id)} />
                          </TableCell>
                          <TableCell className="mono">
                            {curStrategy === "priority" && prevSameName && <span className="mr-1 text-muted-foreground">↓</span>}
                            {r.public_name}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground">{(r.label ? r.label + " · " : "") }<span className="mono">{r.provider}/{r.upstream_model}</span></TableCell>
                          <TableCell>
                            {curStrategy === "priority" ? (
                              <Badge variant={idx === 0 || (prevSameName && sorted[idx - 1].public_name !== r.public_name) ? "default" : "muted"}>
                                P{idx + 1}
                              </Badge>
                            ) : r.weight}
                          </TableCell>
                          <TableCell><Badge variant={r.multiplier === 1 ? "muted" : "default"}>×{r.multiplier}</Badge></TableCell>
                          <TableCell>
                            {r.cache_enabled
                              ? <Badge variant="success" title="语义缓存已开启:精确/语义命中时按全局折扣率计费;x-relay-cache-control: no-cache 可跳过">缓存</Badge>
                              : <Badge variant="muted" title="该路由未启用语义缓存(全局缓存开启后仍需路由级 opt-in)">—</Badge>}
                          </TableCell>
                          <TableCell className="text-right">
                            <RowActions actions={[
                              { label: "编辑", icon: <Pencil className="h-4 w-4" />, onClick: () => setRouteDlg({ edit: r }) },
                              { label: "删除", icon: <Trash2 className="h-4 w-4" />, variant: "destructive", onClick: () => delRoute(r.id) },
                            ]} />
                          </TableCell>
                    </TableRow>
                  );
                    });
                  })()}
                  {routes.length === 0 && <TableRow><TableCell colSpan={7} className="text-sm text-muted-foreground">该组暂无路由</TableCell></TableRow>}
                </TableBody>
              </Table>
            </>
          )}
        </CardContent>
      </Card>

      {sel != null && <AddRouteDialog dlg={routeDlg} groupId={sel} models={models} strategy={curStrategy} onClose={() => setRouteDlg(null)} onSaved={() => loadRoutes(sel)} />}
      {sel != null && <TimeRuleDialog dlg={timeRuleDlg} groupId={sel} onClose={() => setTimeRuleDlg(null)} onSaved={() => loadTimeRules(sel)} />}
      <AddGroupDialog open={groupOpen} onClose={() => setGroupOpen(false)} onCreate={createGroup} />
      {renameDlg && <RenameGroupDialog group={renameDlg} onClose={() => setRenameDlg(null)} onSaved={loadGroups} />}
      <ImportGroupsDialog open={importOpen} onClose={() => setImportOpen(false)} onImported={() => { loadGroups(); if (sel != null) loadRoutes(sel); }} />
      {batchDlg && <BatchEditDialog field={batchDlg} count={checked.size} onClose={() => setBatchDlg(null)} onApply={(v) => applyBatch(batchDlg, v)} />}
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

function RenameGroupDialog({ group, onClose, onSaved }: { group: GroupRow; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState(group.name);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    if (!name.trim() || name.trim() === group.name) { onClose(); return; }
    setErr(""); setBusy(true);
    try { await api.renameGroup(group.id, name.trim()); onSaved(); onClose(); } catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>重命名模型组</DialogTitle>
          <DialogDescription>当前组名:{group.name}。组内路由、用户绑定不受影响。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && submit()} />
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={busy || !name.trim()}>{busy ? "保存中..." : "保存"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function AddGroupDialog({ open, onClose, onCreate }: { open: boolean; onClose: () => void; onCreate: (name: string) => Promise<void> }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState("");
  useEffect(() => { if (open) { setName(""); setErr(""); } }, [open]);
  const submit = async () => {
    if (!name.trim()) return;
    setErr("");
    try { await onCreate(name.trim()); onClose(); } catch (e: any) { setErr(e.message); }
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader><DialogTitle>新建模型组</DialogTitle></DialogHeader>
        <div className="space-y-3">
          <Input placeholder="组名,如 默认组 / VIP" value={name} onChange={(e) => setName(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && submit()} />
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!name.trim()}><Plus className="h-4 w-4" />创建</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function BatchEditDialog({ field, count, onClose, onApply }: {
  field: "multiplier" | "weight"; count: number; onClose: () => void; onApply: (v: number) => Promise<void>;
}) {
  const [val, setVal] = useState(field === "multiplier" ? "1.0" : "100");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    const n = Number(val);
    if (!Number.isFinite(n) || n < 0) { setErr("请输入不小于 0 的数字"); return; }
    setErr(""); setBusy(true);
    try { await onApply(n); onClose(); } catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-xs">
        <DialogHeader>
          <DialogTitle>{field === "multiplier" ? "批量改计费倍率" : "批量改权重"}</DialogTitle>
          <DialogDescription>将同时应用到选中的 {count} 条路由。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <Input value={val} onChange={(e) => setVal(e.target.value)} autoFocus
            onKeyDown={(e) => e.key === "Enter" && submit()} />
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={busy}>{busy ? "应用中..." : "应用"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ImportGroupsDialog({ open, onClose, onImported }: { open: boolean; onClose: () => void; onImported: () => void }) {
  const [step, setStep] = useState<1 | 2>(1);
  const [text, setText] = useState("");
  const [parsed, setParsed] = useState<ImportGroupPayload[] | null>(null);
  const [preview, setPreview] = useState<ImportPreviewResp | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  useEffect(() => { if (open) { setStep(1); setText(""); setParsed(null); setPreview(null); setErr(""); } }, [open]);

  const parseJson = async (raw: string) => {
    setErr("");
    let obj: any;
    try { obj = JSON.parse(raw); } catch { setErr("JSON 解析失败,请检查格式"); return; }
    const groups = Array.isArray(obj?.groups) ? obj.groups : Array.isArray(obj) ? obj : null;
    if (!groups || !groups.length) { setErr("未找到模型组数据(需要 groups 数组)"); return; }
    setParsed(groups);
    setBusy(true);
    try { setPreview(await api.importPreview({ groups })); setStep(2); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };
  const pickFile = (f: File | undefined) => {
    if (!f) return;
    const reader = new FileReader();
    reader.onload = () => setText(String(reader.result ?? ""));
    reader.readAsText(f);
  };
  const doImport = async () => {
    if (!parsed) return;
    setBusy(true); setErr("");
    try { await api.groupsImport({ groups: parsed }); onImported(); onClose(); }
    catch (e: any) { setErr(e.message); } finally { setBusy(false); }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>导入模型组</DialogTitle>
          <DialogDescription>按组名匹配:同名组整组覆盖(路由与时段规则重建),新组名则创建。本机缺失的上游模型会跳过并提示。</DialogDescription>
        </DialogHeader>
        {step === 1 ? (
          <div className="space-y-3">
            <textarea
              className="min-h-[220px] w-full rounded-lg border border-input bg-card p-3 font-mono text-xs focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={'粘贴导出的 JSON,例如:\n{\n  "version": 1,\n  "groups": [{ "name": "默认组", "routes": [ ... ] }]\n}'}
              value={text} onChange={(e) => setText(e.target.value)} />
            <div className="flex items-center justify-between gap-2">
              <Button variant="outline" size="sm" onClick={() => fileRef.current?.click()}>选择 JSON 文件…</Button>
              <input ref={fileRef} type="file" accept=".json,application/json" className="hidden" onChange={(e) => pickFile(e.target.files?.[0])} />
              <div className="flex gap-2">
                <Button variant="ghost" onClick={onClose}>取消</Button>
                <Button size="sm" disabled={!text.trim() || busy} onClick={() => parseJson(text)}>{busy ? "解析中..." : "解析并预览"}</Button>
              </div>
            </div>
            {err && <p className="text-sm text-destructive">{err}</p>}
          </div>
        ) : preview && (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge>共 {preview.summary.groups} 组</Badge>
              {preview.summary.create > 0 && <Badge variant="success">新建 {preview.summary.create}</Badge>}
              {preview.summary.overwrite > 0 && <Badge>覆盖 {preview.summary.overwrite}</Badge>}
              <Badge variant="muted">路由 {preview.summary.routes} 条</Badge>
              {preview.summary.time_rules > 0 && <Badge variant="muted">时段规则 {preview.summary.time_rules} 条</Badge>}
              {preview.summary.skipped_routes > 0 && <Badge variant="destructive">跳过 {preview.summary.skipped_routes} 条</Badge>}
            </div>
            <div className="max-h-[260px] overflow-auto rounded-lg border border-border/60">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>模型组</TableHead><TableHead>动作</TableHead>
                  <TableHead>路由</TableHead><TableHead>时段</TableHead><TableHead>缺失上游</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {preview.data.map((row) => (
                    <TableRow key={row.name}>
                      <TableCell className="font-medium">{row.name}</TableCell>
                      <TableCell>
                        {row.action === "create"
                          ? <Badge variant="success">新建</Badge>
                          : <Badge>覆盖 · 替换现有 {row.existing_routes} 条</Badge>}
                      </TableCell>
                      <TableCell>{row.routes}</TableCell>
                      <TableCell>{row.time_rules}</TableCell>
                      <TableCell>
                        {row.missing_models.length === 0 ? <span className="text-xs text-muted-foreground">—</span> : (
                          <span className="text-xs text-destructive"
                            title={row.missing_models.map((m) => `${m.public_name} (${m.kind} ${m.base_url} ${m.upstream_model})`).join("\n")}>
                            {row.missing_models.length} 条
                          </span>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
            {err && <p className="text-sm text-destructive">{err}</p>}
            <div className="flex justify-between gap-2">
              <Button variant="ghost" onClick={() => { setStep(1); setPreview(null); }}>上一步</Button>
              <Button onClick={doImport} disabled={busy}>{busy ? "导入中..." : `确认导入 ${preview.summary.groups} 组`}</Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function AddRouteDialog({ dlg, groupId, models, strategy, onClose, onSaved }: {
  dlg: { edit: RouteRow | null } | null; groupId: number; models: ModelRow[]; strategy: string; onClose: () => void; onSaved: () => void;
}) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const [publicName, setPublicName] = useState("");
  const [modelId, setModelId] = useState<number | "">("");
  const [weight, setWeight] = useState("100");
  const [mult, setMult] = useState("1");
  const [useCache, setUseCache] = useState(false);
  const [batchCache, setBatchCache] = useState(false);
  const [err, setErr] = useState("");
  // 批量模式
  const [selectedModels, setSelectedModels] = useState<Set<number>>(new Set());
  const [routeRows, setRouteRows] = useState<Map<number, { public_name: string; weight: string; multiplier: string }>>(new Map());
  const [submitting, setSubmitting] = useState(false);
  useEffect(() => {
    if (open) {
      if (edit) {
        setPublicName(edit.public_name); setModelId(edit.model_id);
        setWeight(String(edit.weight)); setMult(String(edit.multiplier));
        setUseCache(edit.cache_enabled);
      } else {
        setPublicName(""); setModelId(models[0]?.id ?? "");
        setWeight("100"); setMult("1"); setUseCache(false); setBatchCache(false);
        setSelectedModels(new Set()); setRouteRows(new Map()); setSelectedProvider("");
      }
      setErr("");
    }
  }, [dlg]);

  const toggleModel = (id: number) => {
    setSelectedModels(prev => {
      const next = new Set(prev);
      if (next.has(id)) { next.delete(id); routeRows.delete(id); }
      else {
        next.add(id);
        const m = models.find(x => x.id === id);
        routeRows.set(id, { public_name: m?.upstream_model || "", weight: "100", multiplier: "1" });
      }
      setRouteRows(new Map(routeRows));
      return next;
    });
  };

  const updateRow = (id: number, field: string, value: string) => {
    const row = routeRows.get(id);
    if (row) { routeRows.set(id, { ...row, [field]: value }); setRouteRows(new Map(routeRows)); }
  };

  // 编辑模式:单个提交;批量模式:多条提交
  const submit = async () => {
    setErr(""); setSubmitting(true);
    try {
      if (edit) {
        await api.updateRoute(edit.id, { public_name: publicName.trim(), model_id: Number(modelId), weight: Number(weight) || 100, multiplier: Number(mult) || 1, cache: useCache });
      } else if (selectedModels.size > 0) {
        const routes = [...selectedModels].map(id => {
          const row = routeRows.get(id);
          return { public_name: row?.public_name || "", model_id: id, weight: Number(row?.weight) || 100, multiplier: Number(row?.multiplier) || 1 };
        }).filter(r => r.public_name.trim());
        if (routes.length === 0) { setErr("请至少填写一个对外模型名"); setSubmitting(false); return; }
        await api.addRoutesBatch(groupId, { routes, cache: batchCache });
      } else {
        await api.addRoute(groupId, { public_name: publicName.trim(), model_id: Number(modelId), weight: Number(weight) || 100, multiplier: Number(mult) || 1, cache: useCache });
      }
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
    setSubmitting(false);
  };

  const batchMode = !edit && selectedModels.size > 0;
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  // 按供应商分组的模型列表
  const modelGroups = (() => {
    const seen = new Map<string, { label: string; items: ModelRow[] }>();
    for (const m of models) {
      const key = m.base_url || "unknown";
      if (!seen.has(key)) {
        let name = key.replace(/^https?:\/\//, "").split("/")[0];
        if (name.includes("deepseek")) name = "DeepSeek";
        else if (name.includes("openai")) name = "OpenAI";
        else if (name.includes("anthropic")) name = "Anthropic";
        else if (name.includes("dashscope")) name = "通义千问";
        else if (name.includes("bigmodel")) name = "智谱 GLM";
        else if (name.includes("moonshot")) name = "Moonshot";
        else if (name.includes("localhost")) name = "本地(Ollama等)";
        seen.set(key, { label: name, items: [] });
      }
      seen.get(key)!.items.push(m);
    }
    return [...seen.values()];
  })();

  // 新建模式:当前选中的供应商
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const providerModels = modelGroups.find(g => g.label === selectedProvider)?.items ?? [];

  // 切换供应商时清空已选模型
  const onProviderChange = (label: string) => {
    setSelectedProvider(label);
    setSelectedModels(new Set());
    setRouteRows(new Map());
  };

  // 全选当前供应商下所有模型
  const selectAllProvider = () => {
    setSelectedModels(prev => {
      const next = new Set(prev);
      const newRows = new Map(routeRows);
      for (const m of providerModels) {
        if (!next.has(m.id)) {
          next.add(m.id);
          newRows.set(m.id, { public_name: m.upstream_model, weight: "100", multiplier: "1" });
        }
      }
      setRouteRows(newRows);
      return next;
    });
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className={batchMode ? "max-w-3xl max-h-[85vh]" : ""}>
        <DialogHeader>
          <DialogTitle>{edit ? "编辑路由" : "添加路由"}</DialogTitle>
          <DialogDescription>对外模型名 → 实际模型;可多选模型批量添加。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          {edit ? (
            /* 编辑模式:单个路由 */
            <>
              <div><label className="mb-1 block text-xs text-muted-foreground">对外模型名</label>
                <Input value={publicName} onChange={(e) => setPublicName(e.target.value)} autoFocus /></div>
              <div><label className="mb-1 block text-xs text-muted-foreground">选择模型</label>
                <select value={modelId} onChange={(e) => setModelId(Number(e.target.value))} className={sel}>
                  {modelGroups.map((g) => (
                    <optgroup key={g.label} label={g.label}>
                      {g.items.map((m) => <option key={m.id} value={m.id}>{m.label || m.upstream_model} — {m.upstream_model}</option>)}
                    </optgroup>
                  ))}
                </select></div>
              <div className="grid grid-cols-2 gap-3">
                <div><label className="mb-1 block text-xs text-muted-foreground">权重</label><Input value={weight} onChange={(e) => setWeight(e.target.value)} /></div>
                <div><label className="mb-1 block text-xs text-muted-foreground">倍率</label><Input value={mult} onChange={(e) => setMult(e.target.value)} /></div>
              </div>
              <div className="flex items-center justify-between gap-4 rounded-lg border bg-muted/30 px-3 py-2.5">
                <div className="space-y-0.5">
                  <p className="text-xs font-medium">语义缓存</p>
                  <p className="text-[11px] text-muted-foreground">开启后该路由的确定性请求(temperature≤0.001 或带 seed)可命中缓存,按全局折扣率计费。</p>
                </div>
                <Switch checked={useCache} onCheckedChange={setUseCache} aria-label="切换语义缓存" />
              </div>
            </>
          ) : (
            /* 新建模式:选供应商 → 选模型 → 配置映射 */
            <>
              <div><label className="mb-1 block text-xs text-muted-foreground">选择供应商</label>
                <select value={selectedProvider} onChange={(e) => onProviderChange(e.target.value)} className={sel}>
                  <option value="">-- 请选择供应商 --</option>
                  {modelGroups.map((g) => <option key={g.label} value={g.label}>{g.label} ({g.items.length} 个模型)</option>)}
                </select>
                {models.length === 0 && <p className="mt-1 text-xs text-destructive">请先在「模型」页面添加上游供应商</p>}
              </div>
              {selectedProvider && (
                <div><label className="mb-1 flex items-center justify-between text-xs text-muted-foreground">
                  <span>选择模型(可多选)</span>
                  <button type="button" className="text-primary hover:underline" onClick={selectAllProvider}>全选该供应商</button>
                </label>
                  <div className="max-h-40 overflow-y-auto rounded-lg border bg-card">
                    {providerModels.map((m) => (
                      <label key={m.id} className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm hover:bg-accent">
                        <input type="checkbox" checked={selectedModels.has(m.id)} onChange={() => toggleModel(m.id)} className="h-4 w-4 rounded border-input" />
                        <span className="flex-1">{m.label || m.upstream_model}</span>
                        <span className="text-xs text-muted-foreground">{m.upstream_model}</span>
                      </label>
                    ))}
                  </div>
                </div>
              )}
              {/* 每个选中的模型生成一行配置 */}
              {batchMode && (
                <div className="space-y-2">
                  <label className="block text-xs font-medium text-muted-foreground">配置每条路由</label>
                  <div className="max-h-64 overflow-y-auto rounded-lg border p-3">
                    {/* 表头 */}
                    <div className="mb-1 grid grid-cols-[140px_20px_1fr_70px_70px_36px] items-center gap-2 text-[10px] text-muted-foreground">
                      <span>对外模型名</span><span/>
                      <span>→ 实际模型</span>
                      <span className="text-center">{strategy === "priority" ? "优先级" : "权重"}</span><span className="text-center">倍率</span><span/>
                    </div>
                    {[...selectedModels].map(id => {
                      const m = models.find(x => x.id === id);
                      const row = routeRows.get(id);
                      if (!m || !row) return null;
                      return (
                        <div key={id} className="grid grid-cols-[140px_20px_1fr_70px_70px_36px] items-center gap-2 py-1.5 text-sm">
                          <Input value={row.public_name} onChange={(e) => updateRow(id, "public_name", e.target.value)} placeholder="如 chat" className="h-8 text-xs" />
                          <span className="text-center text-muted-foreground">→</span>
                          <div className="flex items-center gap-1.5 rounded border border-border/50 bg-muted/30 px-2 py-1 text-xs" title={m.upstream_model}>
                            <Boxes className="h-3 w-3 shrink-0 text-muted-foreground" />
                            <span className="truncate">{m.label || m.upstream_model}</span>
                            <span className="shrink-0 text-[10px] text-muted-foreground">/ {m.upstream_model}</span>
                          </div>
                          <Input value={row.weight} onChange={(e) => updateRow(id, "weight", e.target.value)} className="h-8 text-xs text-center" />
                          <Input value={row.multiplier} onChange={(e) => updateRow(id, "multiplier", e.target.value)} className="h-8 text-xs text-center" />
                          <button onClick={() => toggleModel(id)} className="text-xs text-destructive hover:underline">移除</button>
                        </div>
                      );
                    })}
                  </div>
                  <div className="flex items-center justify-between gap-4 rounded-lg border bg-muted/30 px-3 py-2.5">
                    <div className="space-y-0.5">
                      <p className="text-xs font-medium">语义缓存(本批全部路由)</p>
                      <p className="text-[11px] text-muted-foreground">开启后本批路由均启用语义缓存,命中时按全局折扣率计费。</p>
                    </div>
                    <Switch checked={batchCache} onCheckedChange={setBatchCache} aria-label="切换本批语义缓存" />
                  </div>
                </div>
              )}
            </>
          )}
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={submitting || (!edit && !batchMode && modelId === "") || (batchMode && selectedModels.size === 0)}>
            <Plus className="h-4 w-4" />{submitting ? "提交中..." : edit ? "保存" : batchMode ? `批量添加 (${selectedModels.size})` : "添加"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function TimeRuleDialog({ dlg, groupId, onClose, onSaved }: {
  dlg: { edit: TimeRuleRow | null } | null; groupId: number; onClose: () => void; onSaved: () => void;
}) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const WD = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
  const [name, setName] = useState("");
  const [days, setDays] = useState<boolean[]>([true, true, true, true, true, true, true]);
  const [start, setStart] = useState("09:00");
  const [end, setEnd] = useState("17:00");
  const [mult, setMult] = useState("1.0");
  const [weightMap, setWeightMap] = useState("");
  const [active, setActive] = useState(true);
  const [err, setErr] = useState("");
  useEffect(() => {
    if (open) {
      setName(edit?.name ?? "");
      setStart(edit?.start_time ?? "09:00");
      setEnd(edit?.end_time ?? "17:00");
      setMult(String(edit?.multiplier ?? 1));
      setWeightMap(edit?.weight_map ?? "");
      setActive(edit ? edit.active : true);
      // 解析 weekdays:"0-6" 或 "1-5" / "0,6" / "5"
      const bits = [false, false, false, false, false, false, false];
      if (edit) {
        const spec = edit.weekdays || "0-6";
        for (const part of spec.split(",")) {
          if (part.includes("-")) {
            const [a, b] = part.split("-").map((x) => Number(x));
            for (let w = Math.min(a, b); w <= Math.max(a, b); w++) bits[w] = true;
          } else if (part !== "") bits[Number(part)] = true;
        }
      } else {
        bits.fill(true);
      }
      setDays(bits);
      setErr("");
    }
  }, [dlg]);
  const weekdaysFromBits = () => days.map((on, i) => (on ? String(i) : "")).filter(Boolean).join(",");
  const submit = async () => {
    setErr("");
    if (!name.trim()) { setErr("请填写时段名"); return; }
    const daysSpec = weekdaysFromBits();
    if (!daysSpec) { setErr("请至少选择一个星期"); return; }
    if (start === end) { setErr("开始与结束不能相同"); return; }
    try {
      const body: TimeRulePayload = {
        name: name.trim(), weekdays: daysSpec, start_time: start, end_time: end,
        multiplier: Number(mult) || 1, active,
        weight_map: weightMap.trim() === "" ? null : weightMap.trim(),
      };
      if (edit) await api.updateTimeRule(groupId, edit.id, body); else await api.addTimeRule(groupId, body);
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
  };
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";
  const inp = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{edit ? "编辑时段规则" : "添加时段规则"}</DialogTitle>
          <DialogDescription>命中该时间段时:计费倍率按 ×multiplier 计算,并可覆盖各模型的路由权重(高峰/低谷策略)。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="grid grid-cols-3 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">时段名</label>
              <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="高峰" /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">开始 HH:MM</label>
              <input type="time" value={start} onChange={(e) => setStart(e.target.value)} className={inp} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">结束 HH:MM</label>
              <input type="time" value={end} onChange={(e) => setEnd(e.target.value)} className={inp} /></div>
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">生效星期</label>
            <div className="flex flex-wrap gap-1.5">
              {WD.map((w, i) => (
                <button key={w} type="button" onClick={() => setDays((d) => d.map((x, j) => (j === i ? !x : x)))}
                  className={cn("rounded-md border px-2 py-1 text-xs transition-colors", days[i] ? "border-primary bg-primary/10 text-primary" : "border-border text-muted-foreground")}>
                  {w}
                </button>
              ))}
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">计费倍率系数</label>
              <Input value={mult} onChange={(e) => setMult(e.target.value)} placeholder="如高峰 1.3 / 低谷 0.7" /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">启用</label>
              <button type="button" onClick={() => setActive(!active)} className={cn("mt-1 flex h-8 items-center gap-2 rounded-lg border px-3 text-xs", active ? "border-primary bg-primary/10 text-primary" : "border-input text-muted-foreground")}>
                <span className={cn("h-2.5 w-2.5 rounded-full", active ? "bg-emerald-500" : "bg-muted")} />{active ? "启用" : "停用"}
              </button></div>
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">路由权重覆盖(可选,JSON)</label>
            <textarea value={weightMap} onChange={(e) => setWeightMap(e.target.value)} rows={3}
              placeholder={'{"deepseek-chat":{"1":30,"2":70}}\n对外名 -> {模型ID: 权重}。留空 = 仅适用倍率'}
              className="w-full rounded-lg border border-input bg-card p-2 font-mono text-xs focus:outline-none focus:ring-2 focus:ring-ring" />
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!name.trim()}><Plus className="h-4 w-4" />{edit ? "保存" : "添加"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

const REWARD_STATUS: Record<number, { label: string; variant: "muted" | "success" | "default" }> = {
  0: { label: "待审核", variant: "default" },
  1: { label: "已通过", variant: "success" },
  2: { label: "已驳回", variant: "muted" },
};
type RewardFilter = "pending" | "approved" | "rejected" | "all";
const FILTER_LABEL: Record<RewardFilter, string> = { pending: "待审核", approved: "已通过", rejected: "已驳回", all: "全部" };

function RewardsPanel() {
  const [filter, setFilter] = useState<RewardFilter>("pending");
  const [rows, setRows] = useState<RewardClaimRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [reject, setReject] = useState<RewardClaimRow | null>(null);
  const [approveDlg, setApproveDlg] = useState<RewardClaimRow | null>(null);
  const [preview, setPreview] = useState<{ type: "image" | "text"; content: string } | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);

  const load = async () => {
    setLoading(true);
    try {
      const r = await api.rewards(filter === "all" ? undefined : filter, page, pageSize);
      setRows(r.data); setTotal(r.total);
    } finally { setLoading(false); }
  };
  useEffect(() => { load(); }, [filter, page, pageSize]);

  const fmt = (n: number) => n.toLocaleString();
  // 固定额度任务直接入账;区间类任务需管理员评定额度,走对话框。
  const approve = async (c: RewardClaimRow) => {
    if (c.variable) { setApproveDlg(c); return; }
    setBusy(c.id);
    try { await api.reviewReward(c.id, true); await load(); } finally { setBusy(null); }
  };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">奖励申领({total})</CardTitle>
            <div className="flex items-center gap-1 rounded-lg bg-muted p-0.5">
              {(Object.keys(FILTER_LABEL) as RewardFilter[]).map((f) => (
                <button key={f} onClick={() => { setFilter(f); setPage(1); }}
                  className={cn("rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                    filter === f ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}>
                  {FILTER_LABEL[f]}
                </button>
              ))}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? <p className="text-sm text-muted-foreground">加载中…</p>
            : rows.length === 0 ? <p className="text-sm text-muted-foreground">暂无申领</p> : (
            <Table>
              <TableHeader><TableRow>
                <TableHead>用户</TableHead><TableHead>任务</TableHead><TableHead>额度</TableHead>
                <TableHead>证明</TableHead><TableHead>状态</TableHead><TableHead className="text-right">操作</TableHead>
              </TableRow></TableHeader>
              <TableBody>
                {rows.map((c) => {
                  const m = REWARD_STATUS[c.status] ?? REWARD_STATUS[0];
                  const isImage = !!c.evidence && c.evidence.startsWith("data:image");
                  const evidenceLink = !!c.evidence && /^https?:\/\//.test(c.evidence);
                  return (
                    <TableRow key={c.id}>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Avatar name={c.name || c.user_id} />
                          <span>{c.name || <span className="mono text-xs text-muted-foreground">{c.user_id.slice(0, 8)}</span>}</span>
                        </div>
                      </TableCell>
                      <TableCell>{c.title ?? <span className="text-muted-foreground">已删除任务</span>}</TableCell>
                      <TableCell className="mono">
                        {c.variable && c.status !== 1
                          ? <span className="text-muted-foreground">待评定</span> : fmt(c.reward_tokens)}
                      </TableCell>
                      <TableCell className="max-w-[220px]">
                        {!c.evidence ? <span className="text-muted-foreground">—</span>
                          : isImage ? (
                            <button onClick={() => setPreview({ type: "image", content: c.evidence! })} title="查看截图">
                              <img src={c.evidence} alt="截图" className="h-12 w-20 rounded border object-cover hover:opacity-80" />
                            </button>
                          ) : evidenceLink ? (
                            <a href={c.evidence} target="_blank" rel="noreferrer"
                              className="inline-flex items-center gap-1 truncate text-xs text-primary hover:underline" title={c.evidence}>
                              <ExternalLink className="h-3 w-3 shrink-0" /><span className="truncate">{c.evidence}</span>
                            </a>
                          ) : (
                            <button onClick={() => setPreview({ type: "text", content: c.evidence! })}
                              className="line-clamp-2 text-left text-xs text-primary hover:underline" title="查看全文">
                              {c.evidence}
                            </button>
                          )}
                      </TableCell>
                      <TableCell>
                        <Badge variant={m.variant}>{m.label}</Badge>
                        {c.status === 2 && c.review_note && <div className="mt-0.5 text-xs text-muted-foreground" title={c.review_note}>{c.review_note}</div>}
                      </TableCell>
                      <TableCell className="text-right">
                        {c.status === 0 ? (
                          <div className="flex items-center justify-end gap-2">
                            <Button size="sm" variant="outline" disabled={busy === c.id} onClick={() => setReject(c)}>
                              <X className="h-4 w-4" />驳回
                            </Button>
                            <Button size="sm" disabled={busy === c.id} onClick={() => approve(c)}>
                              <Check className="h-4 w-4" />{busy === c.id ? "处理中…" : "通过并入账"}
                            </Button>
                          </div>
                        ) : <span className="text-xs text-muted-foreground">已审核</span>}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
          <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
        </CardContent>
      </Card>
      <RejectRewardDialog claim={reject} onClose={() => setReject(null)} onDone={load} />
      <ApproveSuggestionDialog claim={approveDlg} onClose={() => setApproveDlg(null)} onDone={load} />
      <Dialog open={!!preview} onOpenChange={(o) => !o && setPreview(null)}>
        <DialogContent className="max-w-2xl">
          <DialogHeader><DialogTitle>{preview?.type === "text" ? "建议内容" : "截图预览"}</DialogTitle></DialogHeader>
          {preview?.type === "image" && <img src={preview.content} alt="截图" className="max-h-[70vh] w-full rounded-lg border object-contain" />}
          {preview?.type === "text" && <div className="max-h-[70vh] overflow-auto whitespace-pre-wrap rounded-lg border bg-muted/40 p-3 text-sm leading-relaxed">{preview.content}</div>}
        </DialogContent>
      </Dialog>
    </div>
  );
}

function ApproveSuggestionDialog({ claim, onClose, onDone }: {
  claim: RewardClaimRow | null; onClose: () => void; onDone: () => void;
}) {
  const range = { min: claim?.reward_min ?? 0, max: claim?.reward_max ?? 0 };
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");
  useEffect(() => { if (claim) { setAmount(String(claim.reward_min)); setNote(""); setErr(""); } }, [claim]);
  const n = Number(amount);
  const valid = Number.isFinite(n) && n >= range.min && n <= range.max;
  const submit = async () => {
    setErr("");
    if (!valid) { setErr(`额度需在 ${range.min.toLocaleString()} ~ ${range.max.toLocaleString()} 之间`); return; }
    try { await api.reviewReward(claim!.id, true, note.trim() || undefined, n); onDone(); onClose(); }
    catch (e: any) { setErr(e.message); }
  };
  return (
    <Dialog open={!!claim} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>通过建议申领</DialogTitle>
          <DialogDescription>评定入账额度({range.min.toLocaleString()} ~ {range.max.toLocaleString()} tokens),通过后入账到用户余额。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">入账额度 (tokens)</label>
            <Input type="number" min={range.min} max={range.max} step={100000} value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">备注(可选)</label>
            <Input placeholder="评定说明(可选)" value={note} onChange={(e) => setNote(e.target.value)} />
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!valid}><Check className="h-4 w-4" />通过并入账</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RejectRewardDialog({ claim, onClose, onDone }: { claim: RewardClaimRow | null; onClose: () => void; onDone: () => void }) {
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");
  useEffect(() => { if (claim) { setNote(""); setErr(""); } }, [claim]);
  const submit = async () => {
    setErr("");
    try { await api.reviewReward(claim!.id, false, note.trim() || undefined); onDone(); onClose(); }
    catch (e: any) { setErr(e.message); }
  };
  return (
    <Dialog open={!!claim} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>驳回申领</DialogTitle>
          <DialogDescription>驳回后不会入账,用户可在门户看到驳回原因。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <Input placeholder="驳回原因(可选,如:未找到对应 Star / Issue)" value={note} onChange={(e) => setNote(e.target.value)} autoFocus />
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button variant="destructive" onClick={submit}><X className="h-4 w-4" />确认驳回</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

const EVIDENCE_LABEL: Record<EvidenceType, string> = {
  screenshot: "上传截图", link: "填写链接", text: "填写文本", none: "无需证明",
};

function RewardTasksPanel() {
  const [rows, setRows] = useState<RewardTaskRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [edit, setEdit] = useState<RewardTaskRow | "new" | null>(null);
  const [del, setDel] = useState<RewardTaskRow | null>(null);

  const load = async () => {
    setLoading(true);
    try { setRows((await api.rewardTasks()).data); } finally { setLoading(false); }
  };
  useEffect(() => { load(); }, []);

  const fmt = (n: number) => n.toLocaleString();
  const amountText = (t: RewardTaskRow) =>
    t.variable ? `${fmt(t.reward_min)} ~ ${fmt(t.reward_max)}(评定)` : fmt(t.reward_tokens);

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">奖励任务({rows.length})</CardTitle>
            <Button size="sm" onClick={() => setEdit("new")}><Plus className="h-4 w-4" />新建任务</Button>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? <p className="text-sm text-muted-foreground">加载中…</p>
            : rows.length === 0 ? <p className="text-sm text-muted-foreground">暂无任务,点「新建任务」添加。</p> : (
            <Table>
              <TableHeader><TableRow>
                <TableHead>标题</TableHead><TableHead>证明</TableHead><TableHead>额度</TableHead>
                <TableHead>状态</TableHead><TableHead className="text-right">操作</TableHead>
              </TableRow></TableHeader>
              <TableBody>
                {rows.map((t) => (
                  <TableRow key={t.id}>
                    <TableCell>
                      <div className="font-medium">{t.title}</div>
                      {t.description && <div className="mt-0.5 line-clamp-1 text-xs text-muted-foreground" title={t.description}>{t.description}</div>}
                    </TableCell>
                    <TableCell className="text-xs">{EVIDENCE_LABEL[t.evidence_type]}</TableCell>
                    <TableCell className="mono text-xs">{amountText(t)}</TableCell>
                    <TableCell><Badge variant={t.enabled ? "success" : "muted"}>{t.enabled ? "启用" : "停用"}</Badge></TableCell>
                    <TableCell className="text-right">
                      <div className="flex items-center justify-end gap-2">
                        <Button size="sm" variant="outline" onClick={() => setEdit(t)}><Pencil className="h-4 w-4" />编辑</Button>
                        <Button size="sm" variant="outline" onClick={() => setDel(t)}><Trash2 className="h-4 w-4" /></Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
      <RewardTaskDialog task={edit} onClose={() => setEdit(null)} onDone={load} />
      <Dialog open={!!del} onOpenChange={(o) => !o && setDel(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>删除任务</DialogTitle>
            <DialogDescription>删除「{del?.title}」后门户将不再显示该任务;历史申领记录保留。</DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setDel(null)}>取消</Button>
            <Button variant="destructive" onClick={async () => { await api.deleteRewardTask(del!.id); setDel(null); load(); }}>
              <Trash2 className="h-4 w-4" />确认删除
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function RewardTaskDialog({ task, onClose, onDone }: {
  task: RewardTaskRow | "new" | null; onClose: () => void; onDone: () => void;
}) {
  const isNew = task === "new";
  const cur = task && task !== "new" ? task : null;
  const [form, setForm] = useState<RewardTaskBody>({
    title: "", description: "", evidence_type: "screenshot", variable: false,
    reward_tokens: 5_000_000, reward_min: 1_000_000, reward_max: 10_000_000, link_url: "", enabled: true, sort: 0,
  });
  const [err, setErr] = useState("");
  useEffect(() => {
    setErr("");
    if (cur) {
      setForm({
        title: cur.title, description: cur.description ?? "", evidence_type: cur.evidence_type, variable: cur.variable,
        reward_tokens: cur.reward_tokens, reward_min: cur.reward_min, reward_max: cur.reward_max,
        link_url: cur.link_url ?? "", enabled: cur.enabled, sort: cur.sort,
      });
    } else if (isNew) {
      setForm({
        title: "", description: "", evidence_type: "screenshot", variable: false,
        reward_tokens: 5_000_000, reward_min: 1_000_000, reward_max: 10_000_000, link_url: "", enabled: true, sort: 0,
      });
    }
  }, [task]);

  const set = <K extends keyof RewardTaskBody>(k: K, v: RewardTaskBody[K]) => setForm((f) => ({ ...f, [k]: v }));
  const submit = async () => {
    setErr("");
    if (!form.title.trim()) { setErr("请填写标题"); return; }
    const body: RewardTaskBody = {
      ...form,
      description: form.description?.trim() || undefined,
      link_url: form.link_url?.trim() || undefined,
    };
    try {
      if (cur) await api.updateRewardTask(cur.id, body);
      else await api.createRewardTask(body);
      onDone(); onClose();
    } catch (e: any) { setErr(e.message); }
  };

  return (
    <Dialog open={!!task} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{cur ? "编辑奖励任务" : "新建奖励任务"}</DialogTitle>
          <DialogDescription>配置门户展示的奖励活动;用户申领后需后台审核方可入账。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">标题</label>
            <Input value={form.title} onChange={(e) => set("title", e.target.value)} placeholder="如:给 Relay 点 Star" autoFocus />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">说明(可选)</label>
            <Input value={form.description ?? ""} onChange={(e) => set("description", e.target.value)} placeholder="申领要求 / 提示" />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">相关链接(可选)</label>
            <Input value={form.link_url ?? ""} onChange={(e) => set("link_url", e.target.value)} placeholder="如仓库地址" />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">证明方式</label>
            <div className="flex flex-wrap gap-1 rounded-lg bg-muted p-0.5">
              {(Object.keys(EVIDENCE_LABEL) as EvidenceType[]).map((et) => (
                <button key={et} onClick={() => set("evidence_type", et)}
                  className={cn("rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                    form.evidence_type === et ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}>
                  {EVIDENCE_LABEL[et]}
                </button>
              ))}
            </div>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={form.variable} onChange={(e) => set("variable", e.target.checked)} />
            额度由管理员审核时评定(区间)
          </label>
          {form.variable ? (
            <div className="grid grid-cols-2 gap-2">
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">额度下限</label>
                <Input type="number" value={form.reward_min} onChange={(e) => set("reward_min", Number(e.target.value))} />
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">额度上限</label>
                <Input type="number" value={form.reward_max} onChange={(e) => set("reward_max", Number(e.target.value))} />
              </div>
            </div>
          ) : (
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">固定额度 (tokens)</label>
              <Input type="number" value={form.reward_tokens} onChange={(e) => set("reward_tokens", Number(e.target.value))} />
            </div>
          )}
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">排序(小在前)</label>
              <Input type="number" value={form.sort} onChange={(e) => set("sort", Number(e.target.value))} />
            </div>
            <label className="flex items-end gap-2 pb-2 text-sm">
              <input type="checkbox" checked={form.enabled} onChange={(e) => set("enabled", e.target.checked)} />
              启用(门户可见)
            </label>
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit}><Check className="h-4 w-4" />保存</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

const fmtCost = (n: number) => (n >= 1 ? `$${n.toFixed(2)}` : `$${n.toFixed(6)}`);

// 获取 provider 的可读名称(模块级:模型页与用量页共用);自定义显示名优先,否则按域名推断
const providerDisplayName = (p: { display_name?: string | null; base_url: string }) => {
  const dn = (p.display_name || "").trim();
  if (dn) return dn;
  const h = p.base_url.replace(/^https?:\/\//, "").split("/")[0];
  if (h.includes("deepseek")) return "DeepSeek";
  if (h.includes("openai")) return "OpenAI";
  if (h.includes("anthropic")) return "Anthropic";
  if (h.includes("dashscope")) return "通义千问";
  if (h.includes("bigmodel")) return "智谱 GLM";
  if (h.includes("moonshot")) return "Moonshot";
  if (h.includes("localhost") || h.includes("127.0.0.1")) return "本地服务";
  return h;
};

/** 用量分布单维度条形块(ECharts 横向条形图,top10,渐变+排名强调+条尾数值) */
function BreakdownBars({ title, rows, metric }: { title: string; rows: UsageBreakdownRow[]; metric: "tokens" | "cost" }) {
  const border = cssVar("--color-border", "#e4e4e7");
  const muted = cssVar("--color-muted-foreground", "#71717a");
  const val = (r: UsageBreakdownRow) => (metric === "cost" ? r.cost_usd : r.input_tokens + r.output_tokens);
  const compact = (v: number) => (v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1_000 ? `${(v / 1_000).toFixed(0)}K` : String(Math.round(v)));
  // tokens=靛紫,费用=琥珀;浅→深横向渐变,第一名不透明强调,其余降透明度形成层次。
  const c = metric === "cost"
    ? { light: "#fcd34d", base: "#f59e0b" }
    : { light: "#a5b4fc", base: "#6366f1" };
  // 排序后反转:ECharts 类目轴自下而上,反转让第一名显示在最上面。
  const sorted = [...rows].sort((a, b) => val(b) - val(a));
  const option: EChartsCoreOption = {
    grid: { left: 8, right: 100, top: 10, bottom: 0, containLabel: true },
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow" },
      confine: true,
      formatter: (params: any) => {
        const p = Array.isArray(params) ? params[0] : params;
        const r = sorted[p.dataIndex];
        if (!r) return "";
        const v = metric === "cost" ? fmtCost(r.cost_usd) : `${(r.input_tokens + r.output_tokens).toLocaleString()} tokens`;
        return `<b>${r.label}</b><br/>${p.marker} ${v}<br/><span style="color:${muted}">${r.calls.toLocaleString()} 次调用 · 占 Top10 的 ${p.percent}%</span>`;
      },
    },
    xAxis: {
      type: "value", splitLine: { show: false },
      axisLabel: { show: false }, axisLine: { show: false }, axisTick: { show: false },
    },
    yAxis: {
      type: "category",
      data: [...sorted].reverse().map((r) => r.label),
      axisTick: { show: false }, axisLine: { lineStyle: { color: border } },
      axisLabel: { color: muted, fontSize: 11, width: 96, overflow: "truncate" },
    },
    series: [{
      type: "bar",
      data: [...sorted].reverse().map((r, i, arr) => ({
        value: metric === "cost" ? Number(r.cost_usd.toFixed(4)) : r.input_tokens + r.output_tokens,
        itemStyle: {
          // 声明式横向渐变(浅→深);Top1(数组末位)不透明强调,其余整体降透明度。
          color: {
            type: "linear", x: 0, y: 0, x2: 1, y2: 0,
            colorStops: [
              { offset: 0, color: c.light },
              { offset: 1, color: c.base },
            ],
          },
          opacity: i === arr.length - 1 ? 1 : 0.62,
          borderRadius: [0, 3, 3, 0],
        },
      })),
      barMaxWidth: 14,
      showBackground: true,
      backgroundStyle: { color: "rgba(113,113,122,0.08)", borderRadius: [0, 3, 3, 0] },
      // 条尾直显数值,无需悬停即可读数;完整明细在 tooltip。
      label: {
        show: true,
        position: "right",
        distance: 6,
        color: muted,
        fontSize: 11,
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        formatter: (p: any) => {
          const r = sorted[p.dataIndex];
          return r ? `${compact(Number(p.value))} · ${r.calls.toLocaleString()}次` : "";
        },
      },
      emphasis: { itemStyle: { shadowBlur: 8, shadowColor: "rgba(99,102,241,0.35)", shadowOffsetX: 1 } },
    }],
  };
  return (
    <div className="min-w-0">
      <h4 className="text-xs font-medium text-muted-foreground">{title}</h4>
      {rows.length === 0 ? (
        <p className="mt-3 text-sm text-muted-foreground">暂无数据</p>
      ) : (
        <EChart option={option} height={Math.max(150, rows.length * 34 + 24)} />
      )}
    </div>
  );
}

function UsagePanel() {
  const navigate = useNavigate();
  const [rows, setRows] = useState<any[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(50);
  const [total, setTotal] = useState(0);
  const [bd, setBd] = useState<UsageBreakdownResp | null>(null);
  const [metric, setMetric] = useState<"tokens" | "cost">("tokens");
  const [allModels, setAllModels] = useState<ModelRow[]>([]);
  useEffect(() => { api.usage(page, pageSize).then((r) => { setRows(r.data); setTotal(r.total); }); }, [page, pageSize]);
  useEffect(() => { api.usageBreakdown().then(setBd).catch(() => { /* 静默,下轮刷新重试 */ }); }, []);
  useEffect(() => { api.models().then((r) => setAllModels(r.data)).catch(() => { /* 静默 */ }); }, []);
  // ID → 友好名称映射(供应商 prov-xxx → 展示名;用户 UUID → 用户名),拉一次全量构建字典。
  const [provMap, setProvMap] = useState<Record<string, string>>({});
  const [userMap, setUserMap] = useState<Record<string, string>>({});
  useEffect(() => { api.providers(1, 200).then((r) => setProvMap(Object.fromEntries(r.data.map((p) => [p.name, providerDisplayName(p)])))).catch(() => { /* 静默 */ }); }, []);
  useEffect(() => { api.users(1, 200).then((r) => setUserMap(Object.fromEntries(r.data.map((u) => [u.id, u.username ?? u.id.slice(0, 8)])))).catch(() => { /* 静默 */ }); }, []);
  const unpricedCount = allModels.filter((m) => m.input_price == null || m.output_price == null).length;
  return (
    <div className="space-y-5">
      {unpricedCount > 0 && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-2.5 text-sm text-amber-700">
          <span className="flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 shrink-0" />
            {unpricedCount} 个模型未设价，成本统计偏低（未定价模型按内置默认价表或 $0 计）
          </span>
          <button className="shrink-0 text-xs font-medium underline-offset-2 hover:underline" onClick={() => navigate("/models")}>去定价 →</button>
        </div>
      )}
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle className="text-base">用量分布</CardTitle>
              <CardDescription className="mt-0.5">Top 10 · 费用按计费口径(单价×倍率)折算,未定价模型记 $0</CardDescription>
            </div>
            <div className="inline-flex rounded-lg border border-input bg-card p-0.5 text-xs">
              {([["tokens", "Tokens"], ["cost", "费用"]] as const).map(([v, label]) => (
                <button key={v} onClick={() => setMetric(v)}
                  className={cn("rounded-md px-3 py-1 transition-colors",
                    metric === v ? "bg-primary font-medium text-primary-foreground" : "text-muted-foreground hover:text-foreground")}>
                  {label}
                </button>
              ))}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid gap-6 lg:grid-cols-3">
            <BreakdownBars title="按供应商" rows={(bd?.providers ?? []).map((r) => ({ ...r, label: provMap[r.label] ?? r.label }))} metric={metric} />
            <BreakdownBars title="按用户" rows={(bd?.users ?? []).map((r) => ({ ...r, label: userMap[r.label] ?? r.label }))} metric={metric} />
            <BreakdownBars title="按密钥" rows={(bd?.keys ?? []).map((r) => ({ ...r, label: r.label }))} metric={metric} />
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle className="text-base">调用明细({total})</CardTitle></CardHeader>
        <CardContent>
          {rows.length === 0 ? <p className="text-sm text-muted-foreground">暂无记录</p> : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>时间</TableHead>
                  <TableHead>用户</TableHead><TableHead>模型</TableHead>
                  <TableHead className="text-right">输入 (tokens)</TableHead>
                  <TableHead className="text-right">输出 (tokens)</TableHead>
                  <TableHead className="text-right">计费 (tokens)</TableHead>
                  <TableHead className="text-right">耗时</TableHead>
                  <TableHead className="text-right">费用</TableHead>
                  <TableHead>状态</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r, i) => {
                  // created_at 存储约定为 "@unix秒";兼容纯数字与 ISO 字符串。
                  const raw = String(r.created_at ?? "").replace(/^@/, "");
                  const tsMs = /^\d+$/.test(raw) ? Number(raw) * 1000 : NaN;
                  const d = isFinite(tsMs) ? new Date(tsMs) : (r.created_at ? new Date(r.created_at) : null);
                  const time = d && !isNaN(d.getTime())
                    ? `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`
                    : "—";
                  const ms = r.latency_ms;
                  const latency = ms != null ? (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`) : "—";
                  const ok = r.status != null && r.status >= 200 && r.status < 400;
                  return (
                    <TableRow key={i}>
                      <TableCell className="mono whitespace-nowrap text-xs text-muted-foreground" title={r.created_at}>{time}</TableCell>
                      <TableCell className="mono text-xs">{userMap[r.user_id] ?? String(r.user_id).slice(0, 8)}</TableCell>
                      <TableCell className="mono text-xs">{r.model || "—"}</TableCell>
                      <TableCell className="mono text-right tabular-nums">{(r.input_tokens ?? 0).toLocaleString()}</TableCell>
                      <TableCell className="mono text-right tabular-nums">{(r.output_tokens ?? 0).toLocaleString()}</TableCell>
                      <TableCell className="mono text-right tabular-nums">{(r.charged_tokens ?? 0).toLocaleString()}</TableCell>
                      <TableCell className="mono text-right tabular-nums">{latency}</TableCell>
                      <TableCell className="mono text-right tabular-nums">{r.cost_usd == null ? "—" : fmtCost(r.cost_usd)}</TableCell>
                      <TableCell>{r.status != null && (ok
                        ? <Badge variant="muted">{r.status}</Badge>
                        : <Badge variant="default" className="border-destructive/30 bg-destructive/15 text-destructive">{r.status || "失败"}</Badge>)}</TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
          <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
        </CardContent>
      </Card>
    </div>
  );
}

// ============================================================
// 语义缓存(命中率趋势 / 节省 tokens / 最近命中)
// ============================================================

function fmtTsShort(ts: number) {
  return new Date(ts * 1000).toLocaleString("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false,
  });
}

/** 缓存命中率趋势:每 5 分钟一桶,命中(绿)/未命中(灰)堆叠柱。 */
function CacheTrendChart({ trend }: { trend: CacheTrendPoint[] }) {
  const option: EChartsCoreOption = {
    grid: { left: 2, right: 2, top: 6, bottom: 0 },
    tooltip: {
      trigger: "axis",
      confine: true,
      formatter: (params: any) => {
        const p = Array.isArray(params) ? params[0] : params;
        const b = trend[p.dataIndex];
        if (!b) return "";
        return `${fmtTsShort(b.ts)}<br/>命中 <b>${b.hits.toLocaleString()}</b> · 未命中 <b>${b.misses.toLocaleString()}</b>`;
      },
    },
    xAxis: { type: "category", data: trend.map((b) => fmtTsShort(b.ts)), show: false },
    yAxis: { type: "value", show: false },
    series: [
      { name: "未命中", type: "bar", stack: "t", data: trend.map((b) => b.misses), itemStyle: { color: "#d4d4d8" }, barMaxWidth: 12 },
      { name: "命中", type: "bar", stack: "t", data: trend.map((b) => b.hits), itemStyle: { color: "#10b981" }, barMaxWidth: 12 },
    ],
  };
  return <EChart option={option} height={100} />;
}

function CachePanel() {
  const navigate = useNavigate();
  const [stats, setStats] = useState<CacheStatsResp | null>(null);
  const [hits, setHits] = useState<CacheHitRow[]>([]);
  const [emb, setEmb] = useState<EmbeddingSettingsResp | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const load = async () => {
    setRefreshing(true);
    try {
      const [s, h, e] = await Promise.all([
        api.cacheStats(),
        api.cacheHits(50),
        api.embeddingSettings().catch(() => null),
      ]);
      setStats(s); setHits(h.hits); setEmb(e);
    } catch { /* 静默,下轮自动重试 */ } finally { setRefreshing(false); }
  };
  useEffect(() => { load(); }, []);
  useEffect(() => {
    const t = setInterval(load, 10000);
    return () => clearInterval(t);
  }, []);

  const total = (stats?.hits ?? 0) + (stats?.misses ?? 0);
  const ratePct = total > 0 ? Math.round((stats?.hit_rate ?? 0) * 1000) / 10 : 0;
  const trend = stats?.trend ?? [];

  return (
    <div className="space-y-5">
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardContent className="pt-5">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <TrendingUp className="h-3.5 w-3.5" />缓存命中率
            </div>
            <div className="mono mt-1 text-2xl font-bold tracking-tight tabular-nums">
              {stats ? `${ratePct}%` : "…"}
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              命中 {fmtInt(stats?.hits ?? 0)} / 总请求 {fmtInt(total)}
            </div>
            <div className="mt-2 flex h-9 items-end gap-1">
              {trend.length > 0 ? (
                <CacheTrendChart trend={trend} />
              ) : (
                <div className="flex h-full w-full items-center justify-center text-[11px] text-muted-foreground">暂无趋势数据</div>
              )}
            </div>
            <div className="mt-1 text-[10px] text-muted-foreground">近 1 小时 · 每 5 分钟一桶(灰=未命中,绿=命中)</div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-5">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Zap className="h-3.5 w-3.5" />累计节省 tokens
            </div>
            <div className="mono mt-1 text-2xl font-bold tracking-tight text-emerald-600 tabular-nums">
              {stats ? fmtInt(stats.tokens_saved) : "…"}
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">命中即免费回放,不再计费</div>
            <div className="mt-2 space-y-1 text-xs text-muted-foreground">
              <div className="flex justify-between"><span>命中</span><span className="mono text-success">+{fmtInt(stats?.hits ?? 0)}</span></div>
              <div className="flex justify-between"><span>未命中(已回源)</span><span className="mono">{fmtInt(stats?.misses ?? 0)}</span></div>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-5">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Database className="h-3.5 w-3.5" />当前缓存条数
            </div>
            <div className="mono mt-1 text-2xl font-bold tracking-tight tabular-nums">
              {stats ? fmtInt(stats.entries) : "…"}
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">按 模型 + 供应商 隔离</div>
            <div className="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div className="h-full rounded-full bg-gradient-to-r from-indigo-500 to-violet-500 transition-[width] duration-300"
                style={{ width: `${Math.min(100, ((stats?.entries ?? 0) / 2048) * 100).toFixed(1)}%` }} />
            </div>
            <div className="mt-1 text-[10px] text-muted-foreground">容量上限 2048 条,超限淘汰最旧</div>
          </CardContent>
        </Card>
      </div>

      {emb && (!emb.enabled || !emb.has_key) && (
        <div className="flex flex-col gap-3 rounded-xl border border-indigo-500/20 bg-gradient-to-r from-indigo-500/10 via-indigo-500/5 to-transparent px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-indigo-500/15 text-indigo-600">
              <Sparkles className="h-4 w-4" />
            </div>
            <div>
              <p className="text-sm font-medium">升级为语义级缓存命中</p>
              <p className="mt-0.5 text-xs text-muted-foreground">
                当前仅相同请求可精确命中。配置一个 OpenAI 兼容的 embedding 供应商后,意思相近的请求也能免费回放。
              </p>
            </div>
          </div>
          <Button size="sm" variant="outline"
            className="shrink-0 border-indigo-500/40 text-indigo-600 hover:bg-indigo-500/10 hover:text-indigo-600"
            onClick={() => navigate("/settings")}>
            配置 embedding 供应商<ExternalLink className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">最近命中({hits.length})</CardTitle>
            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" onClick={load} disabled={refreshing}>
                <RefreshCw className={cn("h-4 w-4", refreshing && "animate-spin")} />刷新
              </Button>
              <Button variant="outline" size="sm" className="text-destructive hover:text-destructive"
                onClick={() => setConfirm({
                  title: "清空语义缓存?",
                  desc: "所有已缓存响应将被删除,后续请求重新回源并计费。",
                  action: async () => { await api.clearCache(); load(); },
                })}>
                <Trash2 className="h-4 w-4" />清空缓存
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {hits.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无命中记录 — 缓存命中后会出现在这里(每 10 秒自动刷新)。</p>
          ) : (
            <Table>
              <TableHeader><TableRow><TableHead>时间</TableHead><TableHead>命中类型</TableHead><TableHead>相似度</TableHead><TableHead>模型</TableHead><TableHead>供应商</TableHead><TableHead className="text-right">节省 tokens</TableHead></TableRow></TableHeader>
              <TableBody>
                {hits.map((h, i) => (
                  <TableRow key={i}>
                    <TableCell className="mono text-xs text-muted-foreground">{fmtTsShort(h.ts)}</TableCell>
                    <TableCell>
                      {h.hit_type === "exact" ? (
                        <Badge variant="success" className="text-[10px]">精确</Badge>
                      ) : (
                        <Badge className="bg-indigo-500/10 text-indigo-600 ring-1 ring-indigo-500/20 text-[10px]">语义</Badge>
                      )}
                    </TableCell>
                    <TableCell className="mono text-xs">
                      {h.hit_type === "semantic" && h.similarity != null ? (
                        <span className="text-indigo-600">{(h.similarity * 100).toFixed(1)}%</span>
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell className="mono">{h.model}</TableCell>
                    <TableCell>{h.provider}</TableCell>
                    <TableCell className="mono text-right text-emerald-600">+{fmtInt(h.tokens_saved)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

const PORT_OPTIONS = [
  { port: 465, label: "465 (SSL / 隐式 TLS)" },
  { port: 587, label: "587 (STARTTLS)" },
  { port: 25, label: "25 (STARTTLS)" },
];

function SettingsPanel() {
  const [host, setHost] = useState("");
  const [port, setPort] = useState(465);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [from, setFrom] = useState("");
  const [hasPassword, setHasPassword] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testTo, setTestTo] = useState("");
  const [testing, setTesting] = useState<null | "loading" | { ok: boolean; msg: string }>(null);
  const [err, setErr] = useState("");
  const [note, setNote] = useState("");
  // 容错与降级
  const [fbEnabled, setFbEnabled] = useState(true);
  const [fbMaxRetries, setFbMaxRetries] = useState(5);
  const [fbSaving, setFbSaving] = useState(false);
  const [fbNote, setFbNote] = useState("");
  const [fbErr, setFbErr] = useState("");
  // 语义缓存
  const [cacheEnabled, setCacheEnabled] = useState(true);
  const [cacheTtl, setCacheTtl] = useState(3600);
  const [cacheThreshold, setCacheThreshold] = useState(0.8);
  const [cacheMultiTurn, setCacheMultiTurn] = useState(3);
  const [cacheBilling, setCacheBilling] = useState(0);
  const [cacheSaving, setCacheSaving] = useState(false);
  const [cacheNote, setCacheNote] = useState("");
  const [cacheErr, setCacheErr] = useState("");
  // L2 语义向量(embedding 供应商)
  const [embEnabled, setEmbEnabled] = useState(false);
  const [embBaseUrl, setEmbBaseUrl] = useState("");
  const [embModel, setEmbModel] = useState("");
  const [embKey, setEmbKey] = useState("");
  const [embHasKey, setEmbHasKey] = useState(false);
  const [embSaving, setEmbSaving] = useState(false);
  const [embNote, setEmbNote] = useState("");
  const [embErr, setEmbErr] = useState("");
  const [embTesting, setEmbTesting] = useState<null | "loading" | { ok: boolean; msg: string }>(null);
  // 日志(预览与保留)
  const [logPreview, setLogPreview] = useState(8192);
  const [logRetention, setLogRetention] = useState(30);
  const [logSaving, setLogSaving] = useState(false);
  const [logNote, setLogNote] = useState("");
  const [logErr, setLogErr] = useState("");
  // 门户设置
  const [portalRewards, setPortalRewards] = useState(false);
  const [portalSaving, setPortalSaving] = useState(false);
  const [portalNote, setPortalNote] = useState("");
  const [portalErr, setPortalErr] = useState("");
  // 软件更新
  const [version, setVersion] = useState("");
  const [updateInfo, setUpdateInfo] = useState<{ current: string; latest: string | null; has_update: boolean; release_url?: string; error?: string } | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const checkUpdate = async () => {
    setUpdateChecking(true);
    try { setUpdateInfo(await api.versionCheck()); } catch { setUpdateInfo(null); } finally { setUpdateChecking(false); }
  };
  useEffect(() => { healthz().then((h) => setVersion(h.version)).catch(() => {}); }, []);

  const load = async () => {
    setLoading(true);
    try {
      const r = await api.emailSettings();
      setHost(r.smtp_host ?? ""); setPort(r.smtp_port || 465);
      setUsername(r.username ?? ""); setFrom(r.from ?? "");
      setHasPassword(r.has_password); setPassword(""); setErr(""); setNote("");
    } catch (e: any) { setErr(e.message); } finally { setLoading(false); }
  };
  useEffect(() => { load(); }, []);

  useEffect(() => {
    api.fallbackSettings()
      .then((r) => { setFbEnabled(r.fallback_enabled); setFbMaxRetries(r.max_retries); })
      .catch(() => { /* 回退默认值 */ });
    api.cacheSettings()
      .then((r) => {
        setCacheEnabled(r.enabled); setCacheTtl(r.ttl_secs);
        setCacheThreshold(r.similarity_threshold); setCacheMultiTurn(r.multi_turn_max);
        setCacheBilling(r.billing_ratio ?? 0);
      })
      .catch(() => { /* 回退默认值 */ });
    api.embeddingSettings()
      .then((r) => {
        setEmbEnabled(r.enabled); setEmbBaseUrl(r.base_url); setEmbModel(r.model);
        setEmbHasKey(r.has_key); setEmbKey("");
      })
      .catch(() => { /* 回退默认值 */ });
    api.loggingSettings()
      .then((r) => { setLogPreview(r.body_preview_max_bytes); setLogRetention(r.retention_days); })
      .catch(() => { /* 回退默认值 */ });
    api.portalSettings()
      .then((r) => setPortalRewards(r.rewards_enabled))
      .catch(() => { /* 回退默认值 */ });
  }, []);

  const enabled = host.trim().length > 0;
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  const save = async () => {
    setErr(""); setNote(""); setSaving(true);
    try {
      const body: any = {
        smtp_host: host.trim(), smtp_port: port,
        username: username.trim(), from: from.trim(),
      };
      if (password) body.password = password;
      await api.saveEmailSettings(body);
      setHasPassword(!!password || hasPassword);
      setPassword("");
      setNote("已保存,即时生效。");
    } catch (e: any) { setErr(e.message); } finally { setSaving(false); }
  };

  const runTest = async () => {
    setErr(""); setNote(""); setTesting("loading");
    try {
      const body: any = {
        smtp_host: host.trim(), smtp_port: port,
        username: username.trim(), from: from.trim(), test_to: testTo.trim(),
      };
      if (password) body.password = password;
      const r = await api.testEmail(body);
      setTesting(r.ok ? { ok: true, msg: "发送成功,请到收件箱(及垃圾箱)确认。" } : { ok: false, msg: r.error || "失败" });
    } catch (e: any) {
      setTesting({ ok: false, msg: e.message });
    }
  };

  const saveFallback = async () => {
    setFbErr(""); setFbNote("");
    if (fbMaxRetries > 100) { setFbErr("最多尝试候选数不能超过 100"); return; }
    setFbSaving(true);
    try {
      await api.saveFallbackSettings({ fallback_enabled: fbEnabled, max_retries: fbMaxRetries });
      setFbNote(fbEnabled ? "已保存,即时生效。" : "已保存:降级已关闭,仅使用首个候选。");
    } catch (e: any) {
      setFbErr(e.message);
    } finally { setFbSaving(false); }
  };

  const saveCache = async () => {
    setCacheErr(""); setCacheNote("");
    if (cacheTtl < 1 || cacheTtl > 604800) { setCacheErr("TTL 需在 1~604800 秒(7 天)之间"); return; }
    if (cacheBilling < 0 || cacheBilling > 1) { setCacheErr("命中计费折扣率需在 0.0~1.0 之间"); return; }
    setCacheSaving(true);
    try {
      await api.saveCacheSettings({
        enabled: cacheEnabled,
        ttl_secs: Math.floor(cacheTtl),
        similarity_threshold: cacheThreshold,
        multi_turn_max: cacheMultiTurn,
        billing_ratio: cacheBilling,
      });
      setCacheNote(cacheEnabled ? "已保存,即时生效。" : "已保存:缓存已关闭,所有请求直接回源。");
    } catch (e: any) {
      setCacheErr(e.message);
    } finally { setCacheSaving(false); }
  };

  const saveEmbedding = async () => {
    setEmbErr(""); setEmbNote("");
    if (embEnabled) {
      if (!embBaseUrl.trim()) { setEmbErr("开启语义向量需填写 API 基础地址"); return; }
      if (!embModel.trim()) { setEmbErr("开启语义向量需填写模型名"); return; }
    }
    setEmbSaving(true);
    try {
      const body: any = { enabled: embEnabled, base_url: embBaseUrl.trim(), model: embModel.trim() };
      if (embKey) body.api_key = embKey;
      await api.saveEmbeddingSettings(body);
      setEmbHasKey(!!embKey || embHasKey);
      setEmbKey("");
      setEmbNote(embEnabled ? "已保存,即时生效。" : "已保存:语义向量已关闭,仅精确哈希命中。");
    } catch (e: any) {
      setEmbErr(e.message);
    } finally { setEmbSaving(false); }
  };

  const testEmbeddingConn = async () => {
    setEmbErr(""); setEmbNote(""); setEmbTesting("loading");
    try {
      const body: any = { enabled: embEnabled, base_url: embBaseUrl.trim(), model: embModel.trim() };
      if (embKey) body.api_key = embKey;
      const r = await api.testEmbedding(body);
      setEmbTesting(r.ok ? { ok: true, msg: `连通成功,向量维度 ${r.dim}。` } : { ok: false, msg: r.error || "失败" });
    } catch (e: any) {
      setEmbTesting({ ok: false, msg: e.message });
    }
  };

  const saveLogging = async () => {
    setLogErr(""); setLogNote("");
    if (logPreview < 0 || logPreview > 1048576) { setLogErr("预览上限需在 0~1048576 字节(1MB)之间"); return; }
    if (logRetention < 0 || logRetention > 365) { setLogErr("保留天数需在 0~365 之间(0=永久)"); return; }
    setLogSaving(true);
    try {
      await api.saveLoggingSettings({
        body_preview_max_bytes: Math.floor(logPreview),
        retention_days: Math.floor(logRetention),
      });
      setLogNote("已保存,即时生效。");
    } catch (e: any) {
      setLogErr(e.message);
    } finally { setLogSaving(false); }
  };

  const savePortal = async () => {
    setPortalErr(""); setPortalNote("");
    setPortalSaving(true);
    try {
      await api.savePortalSettings({ rewards_enabled: portalRewards });
      setPortalNote("已保存,即时生效。");
    } catch (e: any) {
      setPortalErr(e.message);
    } finally { setPortalSaving(false); }
  };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">邮箱配置</CardTitle>
            <Badge variant={enabled ? "success" : "muted"}>{enabled ? "✓ 已配置" : "⚠ 未配置"}</Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {loading ? <p className="text-sm text-muted-foreground">加载中…</p> : (
            <>
              <p className="text-xs text-muted-foreground">
                注册验证码通过此 SMTP 发送。端口 465=SSL(隐式 TLS)、587/25=STARTTLS;授权码填「客户端授权码」而非登录密码。
                {enabled ? "" : "未配置时注册走开发模式(验证码在接口响应 dev_code 返回并打日志)。"}
              </p>
              <div className="grid gap-3 sm:grid-cols-2">
                <div><label className="mb-1 block text-xs text-muted-foreground">SMTP 主机</label>
                  <Input placeholder="如 smtp.163.com" value={host} onChange={(e) => setHost(e.target.value)} /></div>
                <div><label className="mb-1 block text-xs text-muted-foreground">端口</label>
                  <select value={port} onChange={(e) => setPort(Number(e.target.value))} className={sel}>
                    {PORT_OPTIONS.map((p) => <option key={p.port} value={p.port}>{p.label}</option>)}
                  </select></div>
                <div><label className="mb-1 block text-xs text-muted-foreground">用户名</label>
                  <Input placeholder="发信邮箱地址" value={username} onChange={(e) => setUsername(e.target.value)} /></div>
                <div><label className="mb-1 block text-xs text-muted-foreground">授权码{hasPassword ? "(已设置,留空保持不变)" : "(客户端授权码)"}</label>
                  <Input type="password" placeholder={hasPassword ? "留空保持不变" : "客户端授权码"} value={password} onChange={(e) => setPassword(e.target.value)} /></div>
                <div className="sm:col-span-2"><label className="mb-1 block text-xs text-muted-foreground">发件人(可选,留空用用户名)</label>
                  <Input placeholder="留空则用用户名" value={from} onChange={(e) => setFrom(e.target.value)} /></div>
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t pt-4">
                <Button onClick={save} disabled={saving}>{saving ? "保存中…" : "保存配置"}</Button>
                <div className="flex items-center gap-2">
                  <Input className="w-56" placeholder="测试收件邮箱" value={testTo} onChange={(e) => setTestTo(e.target.value)} />
                  <Button variant="outline" onClick={runTest} disabled={testing === "loading" || !host.trim() || !testTo.trim()}>
                    <Send className="h-4 w-4" />{testing === "loading" ? "发送中…" : "发送测试邮件"}
                  </Button>
                </div>
                {testing && testing !== "loading" && (
                  <span className={cn("text-xs", testing.ok ? "text-success" : "text-destructive")} title={testing.msg}>
                    {testing.ok ? "✓ " : "✗ "}{testing.msg}
                  </span>
                )}
                {note && <span className="text-xs text-success">{note}</span>}
                {err && <span className="text-xs text-destructive">{err}</span>}
              </div>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">容错与降级</CardTitle>
            <Badge variant={fbEnabled ? "success" : "muted"}>{fbEnabled ? "故障自动切换" : "仅首候选"}</Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            开启后,主候选不可用(401/403/限流/5xx/网络错误)时自动按排序切换到下一个候选供应商,对调用方完全无感;响应头 X-Relay-Upstream 会标明实际命中的供应商。关闭则只使用首个候选,不做降级重试。
          </p>
          <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
            <div className="space-y-0.5">
              <p className="text-sm font-medium">故障自动切换</p>
              <p className="text-xs text-muted-foreground">关闭后所有请求只走排序第一的候选。</p>
            </div>
            <Switch checked={fbEnabled} onCheckedChange={setFbEnabled} aria-label="切换故障自动切换" />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">最多尝试候选数(0 = 不限)</label>
              <Input
                type="number" min={0} max={100} disabled={!fbEnabled}
                className={fbEnabled ? "" : "opacity-50"}
                value={fbMaxRetries}
                onChange={(e) => setFbMaxRetries(Math.max(0, Math.floor(Number(e.target.value) || 0)))}
              />
              <p className="mt-1 text-[11px] text-muted-foreground">按模型路由的候选顺序依次尝试,超过次数即返回最后一次的错误。</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t pt-4">
            <Button onClick={saveFallback} disabled={fbSaving}>{fbSaving ? "保存中…" : "保存配置"}</Button>
            {fbNote && <span className="text-xs text-success">{fbNote}</span>}
            {fbErr && <span className="text-xs text-destructive">{fbErr}</span>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">语义缓存</CardTitle>
            <Badge variant={cacheEnabled ? "success" : "muted"}>{cacheEnabled ? "缓存开启" : "已关闭"}</Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            确定性请求(temperature≤0.001 或带 seed)在 TTL 内命中缓存直接回放,并按下方「命中计费折扣率」扣费(tokens 照实入账);响应头 X-Relay-Cache 标记 HIT (exact)/HIT (semantic)/MISS,请求头 x-relay-cache-control: no-cache 可跳过缓存。命中还要求路由级开关已启用(路由页「缓存」徽标)。
          </p>
          <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
            <div className="space-y-0.5">
              <p className="text-sm font-medium">启用语义缓存</p>
              <p className="text-xs text-muted-foreground">关闭后所有请求直接转发上游,不做缓存。</p>
            </div>
            <Switch checked={cacheEnabled} onCheckedChange={setCacheEnabled} aria-label="切换语义缓存" />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="sm:col-span-2">
              <label className="mb-1 block text-xs text-muted-foreground">相似度阈值(0.5 ~ 0.95,当前 <span className="mono font-medium text-foreground">{cacheThreshold.toFixed(2)}</span>)</label>
              <input type="range" min="0.5" max="0.95" step="0.01" value={cacheThreshold}
                onChange={(e) => setCacheThreshold(Number(e.target.value))}
                disabled={!cacheEnabled} aria-label="语义相似度阈值"
                className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-muted accent-primary disabled:cursor-not-allowed disabled:opacity-50" />
              <p className="mt-1 text-[11px] text-muted-foreground">值越高要求越严格(需更接近原文才命中);对语义命中生效(需在下方配置 embedding 供应商),精确命中不受影响。</p>
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">缓存有效期 TTL(秒)</label>
              <Input type="number" min={1} max={604800} disabled={!cacheEnabled} className={cacheEnabled ? "" : "opacity-50"}
                value={cacheTtl}
                onChange={(e) => setCacheTtl(Math.max(1, Math.floor(Number(e.target.value) || 1)))} />
              <p className="mt-1 text-[11px] text-muted-foreground">
                {cacheTtl >= 3600 && cacheTtl % 3600 === 0 ? `约 ${cacheTtl / 3600} 小时` : cacheTtl >= 60 ? `约 ${Math.round(cacheTtl / 60)} 分钟` : "最短 1 秒,最长 7 天"}
              </p>
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">多轮对话阈值(条)</label>
              <Input type="number" min={1} max={100} disabled={!cacheEnabled} className={cacheEnabled ? "" : "opacity-50"}
                value={cacheMultiTurn}
                onChange={(e) => setCacheMultiTurn(Math.max(1, Math.floor(Number(e.target.value) || 1)))} />
              <p className="mt-1 text-[11px] text-muted-foreground">消息条数超过该值的长对话跳过缓存,避免低质命中。</p>
            </div>
            <div className="sm:col-span-2">
              <label className="mb-1 block text-xs text-muted-foreground">命中计费折扣率(0.0 ~ 1.0,当前 <span className="mono font-medium text-foreground">{cacheBilling.toFixed(2)}</span>)</label>
              <input type="range" min="0" max="1" step="0.05" value={cacheBilling}
                onChange={(e) => setCacheBilling(Number(e.target.value))}
                disabled={!cacheEnabled} aria-label="命中计费折扣率"
                className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-muted accent-primary disabled:cursor-not-allowed disabled:opacity-50" />
              <p className="mt-1 text-[11px] text-muted-foreground">命中回放时的扣费 = 实际 tokens × 用户倍率 × 折扣率。0 表示命中免费,1 表示照常计费;tokens 始终按实际值入账。</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t pt-4">
            <Button onClick={saveCache} disabled={cacheSaving}>{cacheSaving ? "保存中…" : "保存配置"}</Button>
            {cacheNote && <span className="text-xs text-success">{cacheNote}</span>}
            {cacheErr && <span className="text-xs text-destructive">{cacheErr}</span>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">语义向量(embedding)</CardTitle>
            <Badge variant={embEnabled && embHasKey ? "success" : "muted"}>
              {embEnabled ? (embHasKey ? "✓ 已启用" : "⚠ 缺少 API Key") : "未启用"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            配置 OpenAI 兼容的 /embeddings 接口后,语义缓存从「精确哈希命中」升级为「语义相似命中」:意思相近的请求(相似度达到上方阈值)也免费回放。请求文本会发送至该供应商做向量化(仅用于本地相似度计算);任何失败都会自动降级为精确命中,不影响转发。
          </p>
          <div className="flex items-center justify-between gap-4 rounded-xl border bg-muted/30 px-4 py-3">
            <div className="space-y-0.5">
              <p className="text-sm font-medium">启用语义向量</p>
              <p className="text-xs text-muted-foreground">关闭后语义缓存仅做精确哈希命中。</p>
            </div>
            <Switch checked={embEnabled} onCheckedChange={setEmbEnabled} aria-label="切换语义向量" />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div><label className="mb-1 block text-xs text-muted-foreground">API 基础地址 base_url</label>
              <Input placeholder="如 https://api.openai.com/v1" disabled={!embEnabled} className={embEnabled ? "" : "opacity-50"}
                value={embBaseUrl} onChange={(e) => setEmbBaseUrl(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">Embedding 模型</label>
              <Input placeholder="如 text-embedding-3-small" disabled={!embEnabled} className={embEnabled ? "" : "opacity-50"}
                value={embModel} onChange={(e) => setEmbModel(e.target.value)} /></div>
            <div className="sm:col-span-2"><label className="mb-1 block text-xs text-muted-foreground">API Key{embHasKey ? "(已设置,留空保持不变)" : ""}</label>
              <Input type="password" placeholder={embHasKey ? "留空保持不变" : "sk-..."} disabled={!embEnabled}
                className={embEnabled ? "" : "opacity-50"}
                value={embKey} onChange={(e) => setEmbKey(e.target.value)} /></div>
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t pt-4">
            <Button onClick={saveEmbedding} disabled={embSaving}>{embSaving ? "保存中…" : "保存配置"}</Button>
            <Button variant="outline" onClick={testEmbeddingConn}
              disabled={embTesting === "loading" || !embBaseUrl.trim() || !embModel.trim()}>
              <Zap className="h-4 w-4" />{embTesting === "loading" ? "测试中…" : "测试连接"}
            </Button>
            {embTesting && embTesting !== "loading" && (
              <span className={cn("text-xs", embTesting.ok ? "text-success" : "text-destructive")} title={embTesting.msg}>
                {embTesting.ok ? "✓ " : "✗ "}{embTesting.msg}
              </span>
            )}
            {embNote && <span className="text-xs text-success">{embNote}</span>}
            {embErr && <span className="text-xs text-destructive">{embErr}</span>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">日志采集与保留</CardTitle>
            <Badge variant={logRetention > 0 ? "success" : "muted"}>
              {logRetention > 0 ? `保留 ${logRetention} 天` : "永久保留"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-muted-foreground">
            控制「请求链路」中请求/响应体预览的采集上限与日志保留天数。预览上限 0 表示不采集正文(仅存元数据);保留天数 0 表示永久保留,超期日志由后台任务分批清理。
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div><label className="mb-1 block text-xs text-muted-foreground">正文预览上限(字节,0=不采集)</label>
              <Input type="number" min={0} max={1048576} value={logPreview}
                onChange={(e) => setLogPreview(Number(e.target.value))} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">保留天数(0=永久,最大 365)</label>
              <Input type="number" min={0} max={365} value={logRetention}
                onChange={(e) => setLogRetention(Number(e.target.value))} /></div>
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t pt-4">
            <Button onClick={saveLogging} disabled={logSaving}>{logSaving ? "保存中…" : "保存配置"}</Button>
            {logNote && <span className="text-xs text-success">{logNote}</span>}
            {logErr && <span className="text-xs text-destructive">{logErr}</span>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">门户设置</CardTitle>
            <Badge variant={portalRewards ? "success" : "muted"}>
              {portalRewards ? "奖励已开启" : "奖励已关闭"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium">启用奖励功能</div>
              <div className="text-xs text-muted-foreground">开启后,门户导航将显示"奖励"菜单,用户可查看并申领奖励任务。</div>
            </div>
            <Switch checked={portalRewards} onCheckedChange={setPortalRewards} />
          </div>
          <div className="flex flex-wrap items-center gap-2 border-t pt-4">
            <Button onClick={savePortal} disabled={portalSaving}>{portalSaving ? "保存中…" : "保存配置"}</Button>
            {portalNote && <span className="text-xs text-success">{portalNote}</span>}
            {portalErr && <span className="text-xs text-destructive">{portalErr}</span>}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <CardTitle className="text-base">软件更新</CardTitle>
            <Badge variant="muted">当前 v{version}</Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" onClick={checkUpdate} disabled={updateChecking}>
              {updateChecking ? "检查中…" : "检查更新"}
            </Button>
            {updateInfo?.has_update && updateInfo.latest && (
              <Badge>新版本 v{updateInfo.latest} 可用</Badge>
            )}
            {!updateInfo?.has_update && updateInfo?.latest && !updateInfo?.error && (
              <span className="text-xs text-muted-foreground">已是最新版本</span>
            )}
          </div>
          {updateInfo?.error && (
            <p className="text-xs text-muted-foreground">{updateInfo.error}。</p>
          )}
          {updateInfo?.has_update && updateInfo.release_url && (
            <p className="text-xs text-muted-foreground">
              前往{" "}
              <a href={updateInfo.release_url} target="_blank" rel="noreferrer" className="text-primary hover:underline">
                GitHub Releases
              </a>{" "}
              下载新版本包,解压后执行{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[12px] text-foreground">sudo ./install.sh</code>{" "}
              即可原地升级(配置与数据保留);Linux 服务器也可直接运行{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[12px] text-foreground">sudo ./upgrade.sh</code>{" "}
              一键升级。
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ============================================================
// 上游治理仪表盘(熔断器状态 + 并发槽占用率)
// ============================================================

type BreakerTone = "healthy" | "warn" | "broken";
function breakerTone(fc: number, _threshold: number, broken: boolean): BreakerTone {
  if (broken) return "broken";
  if (fc === 0) return "healthy";
  // 任何非零失败计数都值得关注:healthy 与 broken 之间的过渡态统一用告警色。
  return "warn";
}
const TONE_CLS: Record<BreakerTone, string> = {
  healthy: "from-emerald-400/90 via-emerald-500 to-teal-600",
  warn:    "from-amber-300/90 via-amber-500 to-orange-600",
  broken:  "from-rose-400/95 via-rose-600 to-red-700",
};
const TONE_BADGE: Record<BreakerTone, { label: string; cls: string }> = {
  healthy: { label: "健康",       cls: "bg-emerald-500/10 text-emerald-600 ring-1 ring-emerald-500/20" },
  warn:    { label: "告警",       cls: "bg-amber-500/10 text-amber-700 ring-1 ring-amber-500/20" },
  broken:  { label: "熔断中", cls: "bg-rose-500/15 text-rose-700 ring-1 ring-rose-500/30" },
};

/** 恢复倒计时环:把剩余 ms 画成 SVG 圆环进度 */
function CountdownRing({ remaining_ms, window_secs }: { remaining_ms: number; window_secs: number }) {
  const total = window_secs * 1000;
  const pct = total <= 0 ? 0 : Math.max(0, Math.min(1, remaining_ms / total));
  const R = 18, C = 2 * Math.PI * R;
  const dash = C * pct;
  const secs = Math.ceil(remaining_ms / 1000);
  return (
    <div className="relative inline-flex h-12 w-12 shrink-0 items-center justify-center">
      <svg viewBox="0 0 44 44" className="h-12 w-12 -rotate-90">
        <circle cx="22" cy="22" r={R} stroke="currentColor" strokeOpacity=".12" strokeWidth="3.5" fill="none" />
        <circle cx="22" cy="22" r={R} stroke="currentColor" strokeWidth="3.5" fill="none"
          strokeLinecap="round"
          strokeDasharray={`${dash.toFixed(2)} ${C.toFixed(2)}`}
          className="text-rose-500 transition-all duration-300 ease-linear" />
      </svg>
      <span className="absolute inset-0 flex items-center justify-center text-[11px] font-semibold tabular-nums text-rose-600">
        {secs > 0 ? `${secs}s` : "✓"}
      </span>
    </div>
  );
}

/** 并发槽横向进度条 + 数字 */
function ConcurrencyBar({ s }: { s: UpstreamRow["concurrency"] }) {
  const used = Math.min(100, s.utilization_pct);
  const tone = used >= 95 ? "from-rose-500 to-red-600"
              : used >= 80 ? "from-amber-500 to-orange-600"
              : used >= 40 ? "from-sky-500 to-indigo-600"
              : "from-emerald-500 to-teal-600";
  const unlimited = s.limit === 0;
  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-[11px] text-muted-foreground">
        <span className="inline-flex items-center gap-1"><Cpu className="h-3 w-3" />并发槽</span>
        <span className="tabular-nums">
          {unlimited ? "不限" : (
            <>
              <span className="text-foreground font-medium">{s.in_flight}</span>
              <span className="mx-0.5 opacity-60">/</span>
              <span>{s.limit}</span>
            </>
          )}
        </span>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
        {!unlimited && (
          <div
            className={`h-full rounded-full bg-gradient-to-r ${tone} transition-[width] duration-300`}
            style={{ width: `${used}%` }}
          />
        )}
        {unlimited && <div className="h-full w-full rounded-full bg-gradient-to-r from-emerald-500/40 to-teal-500/40" />}
      </div>
      {!unlimited && <div className="mt-0.5 text-right text-[10px] text-muted-foreground tabular-nums">{used}%</div>}
    </div>
  );
}

function UpstreamCard({ row, onReset, tickMs }: {
  row: UpstreamRow;
  onReset: (id: string) => Promise<void>;
  /** 每 tickMs 会本地刷新一次(用于熔断倒计时) */
  tickMs: number;
}) {
  // 基于后端 recover_remaining_ms 的本地倒计时,避免高频刷新 API
  const [localRemaining, setLocalRemaining] = useState(row.breaker.recover_remaining_ms);
  const [resetting, setResetting] = useState(false);
  useEffect(() => { setLocalRemaining(row.breaker.recover_remaining_ms); }, [row.breaker.recover_remaining_ms]);
  useEffect(() => {
    if (localRemaining <= 0) return;
    const t = setTimeout(() => {
      setLocalRemaining((v) => Math.max(0, v - tickMs));
    }, tickMs);
    return () => clearTimeout(t);
  }, [localRemaining, tickMs]);

  const tone = breakerTone(row.breaker.fail_count, row.breaker.threshold, row.breaker.is_broken && localRemaining > 0);
  const cls = TONE_CLS[tone];
  const badge = TONE_BADGE[tone];
  const id = `${row.kind}|${row.base_url}|${row.key_fingerprint}`;

  const reset = async () => {
    setResetting(true);
    try { await onReset(id); } finally { setResetting(false); }
  };

  return (
    <Card className={cn(
      "group relative overflow-hidden transition-all duration-200",
      tone === "broken" && "ring-2 ring-rose-500/30 shadow-[0_0_0_1px_theme(colors.rose.500/10%),0_8px_30px_-12px_theme(colors.rose.500/50%)]",
      tone === "warn"   && "ring-1 ring-amber-500/20",
      "hover:shadow-md"
    )}>
      {/* 左色条:状态可视化 */}
      <div className={cn(
        "pointer-events-none absolute left-0 top-0 h-full w-1.5 bg-gradient-to-b",
        cls,
        tone === "broken" && "animate-pulse"
      )} />
      <CardContent className="p-4 pl-5">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            {/* 头部:协议 + Base URL + 状态徽标 */}
            <div className="mb-1.5 flex items-center gap-2">
              <Badge variant="muted" className="shrink-0 text-[10px] uppercase tracking-wider">{row.kind}</Badge>
              <span className={cn("rounded-md px-1.5 py-0.5 text-[10px] font-medium", badge.cls)}>
                {tone === "broken" && <ShieldAlert className="mr-1 inline h-3 w-3 -translate-y-[1px]" />}
                {tone === "healthy" && <ShieldCheck className="mr-1 inline h-3 w-3 -translate-y-[1px]" />}
                {badge.label}
              </span>
              {row.has_key ? (
                <span className="rounded-md bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-medium text-sky-700 ring-1 ring-sky-500/20"
                  title={`Key 指纹: ${row.key_fingerprint}`}>
                  Key · {row.key_fingerprint.slice(0, 8)}
                </span>
              ) : (
                <span className="rounded-md bg-slate-500/10 px-1.5 py-0.5 text-[10px] text-slate-500 ring-1 ring-slate-500/20">
                  无 Key
                </span>
              )}
            </div>
            <div className="mono mb-3 truncate text-xs text-muted-foreground" title={row.base_url}>{row.base_url}</div>

            <div className="space-y-3">
              {/* 熔断器状态 */}
              <div className="flex items-start gap-3 rounded-xl bg-muted/40 p-3">
                {tone === "broken"
                  ? <CountdownRing remaining_ms={localRemaining} window_secs={row.breaker.window_secs} />
                  : (
                    <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl">
                      {tone === "healthy"
                        ? <ShieldCheck className="h-6 w-6 text-emerald-500" />
                        : <Clock className="h-6 w-6 text-amber-500" />}
                    </div>
                  )}
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex items-center justify-between text-xs">
                    <span className="font-medium text-foreground">熔断器</span>
                    <span className="tabular-nums text-muted-foreground">
                      {row.breaker.fail_count}<span className="opacity-50">/</span>{row.breaker.threshold}
                    </span>
                  </div>
                  {/* 失败计数条:阈值 3,三段式 */}
                  <div className="flex h-1.5 gap-1">
                    {Array.from({ length: row.breaker.threshold }).map((_, i) => (
                      <div key={i} className={cn(
                        "h-full flex-1 rounded-full transition-all duration-300",
                        i < row.breaker.fail_count
                          ? (tone === "broken" ? "bg-rose-500" : "bg-amber-500")
                          : "bg-muted-foreground/15"
                      )} />
                    ))}
                  </div>
                  <div className="mt-1.5 text-[10px] text-muted-foreground">
                    {tone === "healthy" && "连续 0 次失败"}
                    {tone === "warn" && !row.breaker.is_broken && `已 ${row.breaker.fail_count} 次失败,再失败 ${Math.max(0, row.breaker.threshold - row.breaker.fail_count)} 次即熔断`}
                    {tone === "broken" && `熔断窗 ${row.breaker.window_secs}s,过半开 1 次探活恢复`}
                  </div>
                </div>
              </div>

              {/* 并发槽状态 */}
              <ConcurrencyBar s={row.concurrency} />
            </div>
          </div>
        </div>

        {/* 底部操作条 */}
        <div className="mt-3 flex items-center justify-between border-t pt-3">
          <div className="text-[10px] text-muted-foreground tabular-nums">
            <span className="font-mono">{encodeURIComponent(id).length > 40 ? id.slice(0, 40) + "…" : id}</span>
          </div>
          <div className="flex items-center gap-2">
            {tone === "broken" && (
              <span className="inline-flex items-center gap-1 text-[11px] text-rose-600">
                <Clock className="h-3 w-3" />
                {localRemaining > 0
                  ? `约 ${Math.ceil(localRemaining / 1000)}s 后自动探活`
                  : "恢复窗口已过,等待下一次请求探活"}
              </span>
            )}
            <Button size="sm" variant="outline" disabled={resetting} onClick={reset}>
              <RotateCcw className={cn("h-3.5 w-3.5", resetting && "animate-spin")} />
              {resetting ? "处理中…" : "手动清零"}
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

type FilterTone = "all" | BreakerTone;
const FILTERS: { k: FilterTone; label: string; icon?: any }[] = [
  { k: "all",    label: "全部" },
  { k: "broken", label: "熔断", icon: ShieldAlert },
  { k: "warn",   label: "告警", icon: Clock },
  { k: "healthy",label: "健康", icon: ShieldCheck },
];

/** 审计 ring buffer 容量兜底(与后端 state.rs 的 AUDIT_FAILURE_CAP 保持一致,前端只是展示 fallback) */
const AUDIT_CAP_FALLBACK = 500;

/** 相对时间:把 unix ms 时间戳变成「刚刚 / X 秒前 / X 分钟前 / X 小时前 / 昨天 / M-D HH:mm」 */
function fmtRel(ts_ms: number): string {
  const diff = Date.now() - ts_ms;
  if (diff < 1000) return "刚刚";
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return `${sec} 秒前`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} 分钟前`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} 小时前`;
  const day = Math.floor(hr / 24);
  if (day === 1) return "昨天";
  if (day < 7) return `${day} 天前`;
  const d = new Date(ts_ms);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${mi}`;
}

function UpstreamsPanel() {
  const [data, setData] = useState<UpstreamsResp | null>(null);
  const [failures, setFailures] = useState<AuditFailuresResp | null>(null);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState("");
  const [auto, setAuto] = useState(true);
  const [intervalSecs, setIntervalSecs] = useState(5);
  const [tone, setTone] = useState<FilterTone>("all");
  const [q, setQ] = useState("");
  const [toast, setToast] = useState<{ type: "ok" | "err"; msg: string } | null>(null);
  const [resetAllOpen, setResetAllOpen] = useState(false);
  const [resetAllBusy, setResetAllBusy] = useState(false);
  const [failuresLimit, setFailuresLimit] = useState<number>(50);

  const load = async () => {
    try {
      setErr("");
      const [u, f] = await Promise.all([
        api.listUpstreams(),
        api.listFailures(failuresLimit),
      ]);
      setData(u); setFailures(f);
    } catch (e: any) { setErr(e.message || "加载失败"); }
    finally { setLoading(false); }
  };
  // 首屏 + 手动刷新
  useEffect(() => { load(); }, []);
  // failuresLimit 改变再拉一次
  useEffect(() => { load(); /* eslint-disable-line react-hooks/exhaustive-deps */ }, [failuresLimit]);
  // 自动轮询
  useEffect(() => {
    if (!auto) return;
    const t = setInterval(load, intervalSecs * 1000);
    return () => clearInterval(t);
  }, [auto, intervalSecs, failuresLimit]);
  // toast 3s 自动消失
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  const fmt = (n: number) => (n ?? 0).toLocaleString();
  const healthy = (data?.total ?? 0) - (data?.broken_count ?? 0);
  const totalInFlight = (data?.items ?? []).reduce((a, r) => a + r.concurrency.in_flight, 0);
  const totalLimit = (data?.items ?? []).reduce((a, r) => a + r.concurrency.limit, 0);
  const utilPct = totalLimit === 0 ? 0 : Math.round((totalInFlight * 100) / totalLimit);

  const items = (data?.items ?? []).filter((r) => {
    const t = breakerTone(r.breaker.fail_count, r.breaker.threshold, r.breaker.is_broken);
    if (tone !== "all" && t !== tone) return false;
    if (!q.trim()) return true;
    const needle = q.trim().toLowerCase();
    return r.base_url.toLowerCase().includes(needle)
      || r.kind.toLowerCase().includes(needle)
      || r.key_fingerprint.toLowerCase().includes(needle);
  });

  const resetOne = async (id: string) => {
    try {
      const r = await api.resetOneBreaker(id);
      setToast({ type: r.matched ? "ok" : "err", msg: r.matched ? "已清零熔断器" : "未匹配到该上游" });
    } catch (e: any) { setToast({ type: "err", msg: e.message || "清零失败" }); }
    await load();
  };

  const resetAll = async () => {
    setResetAllBusy(true);
    try {
      const r = await api.resetAllBreakers();
      setToast({ type: "ok", msg: `已清零 ${r.cleared} 个上游的熔断器` });
      setResetAllOpen(false);
    } catch (e: any) { setToast({ type: "err", msg: e.message || "清零失败" }); }
    finally { setResetAllBusy(false); await load(); }
  };

  return (
    <div className="space-y-5">
      {/* Toast */}
      {toast && (
        <div className={cn(
          "fixed right-6 top-6 z-50 flex items-center gap-2 rounded-xl px-4 py-3 text-sm shadow-2xl backdrop-blur",
          toast.type === "ok"
            ? "bg-emerald-500/95 text-white ring-1 ring-emerald-400/60"
            : "bg-rose-500/95 text-white ring-1 ring-rose-400/60"
        )}>
          {toast.type === "ok" ? <Check className="h-4 w-4" /> : <ShieldAlert className="h-4 w-4" />}
          <span>{toast.msg}</span>
        </div>
      )}

      {/* 顶部总览 */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          label="上游总数"
          value={fmt(data?.total ?? 0)}
          sub={(data?.total ?? 0) === 0 ? "尚无访问过的上游" : "被访问过的连接数"}
        />
        <div className={cn(
          "transition-all",
          (data?.broken_count ?? 0) > 0 && "animate-[pulse_2s_ease-in-out_infinite]"
        )}>
          <Card className={cn((data?.broken_count ?? 0) > 0 && "ring-2 ring-rose-500/30 bg-rose-500/[0.02]")}>
            <CardContent className="pt-5">
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted-foreground">熔断中</div>
                <ShieldAlert className={cn("h-4 w-4", (data?.broken_count ?? 0) > 0 ? "text-rose-500" : "text-slate-400")} />
              </div>
              <div className={cn("mono mt-1 text-2xl font-bold tracking-tight",
                (data?.broken_count ?? 0) > 0 ? "text-rose-600" : "text-slate-400")}>
                {fmt(data?.broken_count ?? 0)}
              </div>
              <div className="mt-0.5 text-xs text-muted-foreground">
                {(data?.broken_count ?? 0) > 0 ? "请立即查看红色卡片" : "全部正常 🎉"}
              </div>
            </CardContent>
          </Card>
        </div>
        <StatCard
          label="健康数"
          value={fmt(healthy)}
          sub={healthy > 0 ? "可承接请求的上游" : ""}
        />
        <Card>
          <CardContent className="pt-5">
            <div className="flex items-center justify-between">
              <div className="text-xs text-muted-foreground">全局并发占用</div>
              <Cpu className={cn("h-4 w-4", utilPct >= 80 ? "text-rose-500" : "text-sky-500")} />
            </div>
            <div className="mono mt-1 text-2xl font-bold tracking-tight tabular-nums">
              {totalLimit === 0 ? "∞" : (
                <>
                  {fmt(totalInFlight)}
                  <span className="mx-1 text-base font-normal text-muted-foreground opacity-70">/</span>
                  <span className="text-base font-normal text-muted-foreground">{fmt(totalLimit)}</span>
                </>
              )}
            </div>
            <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={cn(
                  "h-full rounded-full bg-gradient-to-r transition-[width] duration-300",
                  utilPct >= 80 ? "from-amber-500 to-rose-500" : "from-sky-500 to-indigo-500"
                )}
                style={{ width: `${totalLimit === 0 ? 100 : utilPct}%` }}
              />
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground tabular-nums">
              {totalLimit === 0 ? "未配置并发上限" : `占用率 ${utilPct}%`}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* 操作条 */}
      <Card>
        <CardHeader className="flex-row items-center justify-between gap-3 space-y-0">
          <CardTitle className="text-base flex items-center gap-2">
            <Zap className="h-4 w-4 text-primary" />
            上游治理仪表盘
          </CardTitle>
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="w-56 pl-8"
                placeholder="搜 base_url / 协议 / 指纹"
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
            </div>
            <div className="flex items-center gap-0.5 rounded-lg bg-muted p-0.5">
              {FILTERS.map((f) => (
                <button key={f.k} onClick={() => setTone(f.k)}
                  className={cn(
                    "inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                    tone === f.k ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
                  )}>
                  {f.icon && <f.icon className="h-3 w-3" />}{f.label}
                </button>
              ))}
            </div>
            <div className="flex items-center gap-1 rounded-lg bg-muted p-0.5">
              <button
                onClick={() => setAuto((v) => !v)}
                title={auto ? "暂停自动刷新" : "开启自动刷新"}
                className={cn(
                  "inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                  auto ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
                )}>
                <RefreshCw className={cn("h-3 w-3", auto && "animate-spin [animation-duration:2s]")} />
                {auto ? "自动" : "已暂停"}
              </button>
              <select
                value={intervalSecs}
                onChange={(e) => setIntervalSecs(Number(e.target.value))}
                disabled={!auto}
                className="rounded-md border-0 bg-transparent px-1.5 py-1 text-xs text-muted-foreground focus:outline-none focus:ring-0 disabled:opacity-50"
              >
                {[2, 5, 10, 30].map((s) => <option key={s} value={s}>{s}s</option>)}
              </select>
            </div>
            <Button size="sm" variant="outline" onClick={load} disabled={loading}>
              <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
              {loading ? "加载中" : "立即刷新"}
            </Button>
            <Button size="sm" variant="destructive" onClick={() => setResetAllOpen(true)}
              disabled={!data || data.total === 0}>
              <RotateCcw className="h-3.5 w-3.5" />
              全部清零
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {loading && !data ? (
            <p className="py-10 text-center text-sm text-muted-foreground">加载中…</p>
          ) : err ? (
            <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3 text-sm text-destructive">
              {err}
            </div>
          ) : items.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-16 text-center">
              <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-muted">
                <Zap className="h-7 w-7 text-muted-foreground/70" />
              </div>
              <div className="text-sm font-medium text-foreground">
                {(data?.total ?? 0) === 0 ? "尚无访问记录" : "无匹配的上游"}
              </div>
              <div className="mt-1 max-w-sm text-xs text-muted-foreground">
                {(data?.total ?? 0) === 0
                  ? "上游治理在首次调用模型路由后才会记录连接,请先让用户发起一次请求再查看。"
                  : "尝试调整筛选条件或搜索关键词。"}
              </div>
            </div>
          ) : (
            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
              {items.map((r, i) => (
                <UpstreamCard key={`${r.kind}|${r.base_url}|${r.key_fingerprint}|${i}`} row={r} onReset={resetOne} tickMs={250} />
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ================== 失败审计(路由转移记录) ================== */}
      <Card>
        <CardHeader className="flex-row items-center justify-between gap-3 space-y-0">
          <CardTitle className="text-base flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-amber-500" />
            失败审计 · 路由转移记录
            <Badge variant="muted" className="ml-1.5 text-[10px]">
              {(failures?.stored ?? 0).toLocaleString()}
              <span className="mx-1 opacity-50">/</span>
              {failures?.capacity ?? AUDIT_CAP_FALLBACK}
            </Badge>
          </CardTitle>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">显示最近</span>
            <select
              value={failuresLimit}
              onChange={(e) => setFailuresLimit(Number(e.target.value))}
              className="h-8 rounded-md border bg-transparent px-2 text-xs focus:outline-none focus:ring-0"
            >
              {[10, 30, 50, 100, 200].map((n) => <option key={n} value={n}>{n} 条</option>)}
            </select>
            <Button size="sm" variant="outline" onClick={load} disabled={loading}>
              <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
              刷新
            </Button>
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {!failures || failures.items.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-14 text-center">
              <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-emerald-500/10 ring-1 ring-emerald-500/20">
                <Check className="h-7 w-7 text-emerald-500" />
              </div>
              <div className="text-sm font-medium text-foreground">暂无失败记录 · 全绿中 ✨</div>
              <div className="mt-1 max-w-md text-xs text-muted-foreground">
                每次路由在某个上游遇到 429 / 5xx / 断链就会记一条,按时间倒序保留最多 {(failures?.capacity ?? AUDIT_CAP_FALLBACK).toLocaleString()} 条。
              </div>
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/30 text-left text-[11px] uppercase tracking-wider text-muted-foreground">
                    <th className="px-4 py-2.5 font-medium">时间</th>
                    <th className="px-3 py-2.5 font-medium">上游</th>
                    <th className="px-3 py-2.5 font-medium">模型 · 链路</th>
                    <th className="px-3 py-2.5 font-medium">失败计数</th>
                    <th className="px-3 py-2.5 font-medium">错误摘要</th>
                    <th className="px-4 py-2.5 text-right font-medium">操作</th>
                  </tr>
                </thead>
                <tbody>
                  {failures.items.map((r, i) => (
                    <tr key={`${r.ts_ms}-${r.upstream_id}-${i}`} className={cn(
                      "border-b transition-colors hover:bg-muted/30",
                      r.triggered_break && "bg-rose-500/[0.04] hover:bg-rose-500/[0.07]"
                    )}>
                      <td className="px-4 py-3 align-top tabular-nums">
                        <div className="flex flex-col gap-0.5">
                          <span className="text-xs font-medium text-foreground">{fmtRel(r.ts_ms)}</span>
                          <span className="text-[10px] text-muted-foreground" title={new Date(r.ts_ms).toLocaleString()}>
                            {new Date(r.ts_ms).toLocaleString("zh-CN", { hour12: false })}
                          </span>
                        </div>
                      </td>
                      <td className="px-3 py-3 align-top">
                        <div className="flex flex-col gap-1.5">
                          <div className="flex flex-wrap items-center gap-1.5">
                            <Badge variant="muted" className="text-[10px] uppercase tracking-wider">{r.kind}</Badge>
                            {r.key_fingerprint ? (
                              <Badge variant="muted" className="text-[10px] bg-sky-500/10 text-sky-700 ring-1 ring-sky-500/20"
                                title={`Key 指纹: ${r.key_fingerprint}`}>
                                Key · {r.key_fingerprint.slice(0, 8)}
                              </Badge>
                            ) : (
                              <Badge variant="muted" className="text-[10px] text-slate-500">无 Key</Badge>
                            )}
                            {r.triggered_break && (
                              <span className="inline-flex items-center gap-1 rounded-md bg-rose-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-rose-700 ring-1 ring-rose-500/30">
                                <ShieldAlert className="h-3 w-3" />
                                触发熔断
                              </span>
                            )}
                          </div>
                          <span className="mono text-xs text-muted-foreground break-all" title={r.base_url}>{r.base_url}</span>
                        </div>
                      </td>
                      <td className="px-3 py-3 align-top">
                        <div className="flex flex-col gap-1">
                          <div className="inline-flex w-max items-center gap-1 rounded-md bg-indigo-500/10 px-1.5 py-0.5 text-[11px] font-medium text-indigo-700 ring-1 ring-indigo-500/20"
                            title="用户请求的对外模型名">
                            {r.requested_model || "—"}
                          </div>
                          <span className="text-[11px] text-muted-foreground">
                            {r.path === "chat" ? "聊天补全 /v1/chat/completions" :
                             r.path === "messages" ? "Anthropic Messages /v1/messages" :
                             r.path}
                          </span>
                        </div>
                      </td>
                      <td className="px-3 py-3 align-top">
                        <div className="inline-flex items-center gap-2">
                          <div className="flex h-4 gap-0.5">
                            {Array.from({ length: r.threshold }).map((_, idx) => (
                              <div key={idx} className={cn(
                                "h-full w-3 rounded-sm transition-colors",
                                idx < r.fail_count_after
                                  ? r.triggered_break ? "bg-rose-500" : "bg-amber-500"
                                  : "bg-muted-foreground/15"
                              )} />
                            ))}
                          </div>
                          <span className="tabular-nums text-xs text-muted-foreground">
                            {r.fail_count_after}<span className="opacity-50">/</span>{r.threshold}
                          </span>
                        </div>
                      </td>
                      <td className="px-3 py-3 align-top max-w-md">
                        <div className="group relative">
                          <code className="block max-w-[28rem] truncate rounded-md bg-muted/60 px-2 py-1 text-[11px] text-rose-700 ring-1 ring-rose-500/10"
                            title={r.error_summary}>
                            {r.error_summary || "(无错误描述)"}
                          </code>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-right align-top">
                        <div className="flex justify-end gap-1.5">
                          <Button size="sm" variant="outline"
                            onClick={() => {
                              // 定位:把搜索框设为「协议 + base_url」,筛选保持 all
                              setTone("all");
                              setQ(`${r.kind} ${r.base_url}`);
                              // 滚到页面顶部(上游卡片)
                              window.scrollTo({ top: 0, behavior: "smooth" });
                            }}>
                            <Link2 className="h-3.5 w-3.5" />
                            定位上游
                          </Button>
                          <Button size="sm" variant="secondary" onClick={() => resetOne(r.upstream_id)}>
                            <RotateCcw className="h-3.5 w-3.5" />
                            清零熔断器
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 全部清零确认对话框 */}
      <Dialog open={resetAllOpen} onOpenChange={(o) => !o && setResetAllOpen(false)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle className="inline-flex items-center gap-2">
              <RotateCcw className="h-4 w-4 text-rose-500" />
              清零所有上游熔断器?
            </DialogTitle>
            <DialogDescription>
              手动清除 {data?.total ?? 0} 个上游的连续失败计数。仅建议在你已修复上游故障后使用。
              自动恢复会在 30s 熔断窗结束后由半开探活触发,无需手动干预。
            </DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setResetAllOpen(false)}>取消</Button>
            <Button variant="destructive" disabled={resetAllBusy} onClick={resetAll}>
              <RotateCcw className={cn("h-4 w-4", resetAllBusy && "animate-spin")} />
              {resetAllBusy ? "处理中…" : "确认全部清零"}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ==========================================================================
// 接口指标仪表盘
// ==========================================================================

/** 可选时间窗口(秒):label */
const RANGE_PRESETS: { label: string; secs: number }[] = [
  { label: "最近 5 分钟", secs: 300 },
  { label: "最近 15 分钟", secs: 900 },
  { label: "最近 30 分钟", secs: 1800 },
  { label: "最近 1 小时", secs: 3600 },
  { label: "最近 4 小时", secs: 14400 },
  { label: "最近 12 小时", secs: 43200 },
  { label: "最近 24 小时", secs: 86400 },
  { label: "全部(最多 48h)", secs: 0 },
];

const fmtInt = (n: number) => {
  if (!isFinite(n)) return "0";
  const v = Math.max(0, Math.round(n));
  if (v >= 1_000_000) return (v / 1_000_000).toFixed(2) + "M";
  if (v >= 1_000) return (v / 1_000).toFixed(1) + "K";
  return String(v);
};
const fmtMs = (n: number) => (isFinite(n) ? (n < 1000 ? `${Math.round(n)} ms` : `${(n / 1000).toFixed(2)} s`) : "—");
const fmtPct = (n: number) => (isFinite(n) ? `${(n * 100).toFixed(2)}%` : "—");
const fmtBuckets = (n: number, label: string) => {
  if (!isFinite(n)) return `0 ${label}`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M ${label}`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K ${label}`;
  return `${Math.round(n)} ${label}`;
};

function KpiCard({
  label,
  value,
  sub,
  tone = "default",
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: "default" | "good" | "warn" | "bad" | "info";
}) {
  const toneText: Record<string, string> = {
    default: "text-slate-700",
    good: "text-emerald-700",
    warn: "text-amber-700",
    bad: "text-rose-700",
    info: "text-sky-700",
  };
  return (
    <div className="rounded-xl border bg-gradient-to-br from-slate-50 to-white p-4 shadow-sm border-slate-200 dark:border-slate-800 dark:from-slate-900 dark:to-slate-900/40">
      <div className="text-xs tracking-wide text-slate-500 dark:text-slate-400">{label}</div>
      <div className={cn("mt-1 text-2xl font-semibold tabular-nums", toneText[tone], "dark:text-inherit")}>
        {value}
      </div>
      {sub ? <div className="mt-1 text-xs text-slate-500 tabular-nums dark:text-slate-400">{sub}</div> : null}
    </div>
  );
}

/** 请求量(柱状) + P95 延迟(折线)双轴图 */
/** 请求量(柱状) + P95 延迟(折线)双轴图 */
function ReqLatencyChart({ points }: { points: MetricsSeriesPoint[] }) {
  const border = cssVar("--color-border", "#e4e4e7");
  const muted = cssVar("--color-muted-foreground", "#71717a");
  const fmtTs = (ts: number) => {
    const d = new Date(ts * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };
  const compact = (v: number) => (v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1_000 ? `${(v / 1_000).toFixed(0)}K` : String(v));
  const option: EChartsCoreOption = {
    grid: { left: 8, right: 8, top: 32, bottom: 0, containLabel: true },
    legend: { top: 0, textStyle: { color: muted, fontSize: 11 }, itemWidth: 14, itemHeight: 8 },
    tooltip: {
      trigger: "axis",
      confine: true,
      formatter: (params: any) => {
        const list = Array.isArray(params) ? params : [params];
        const p = points[list[0]?.dataIndex ?? 0];
        if (!p) return "";
        const rows = list.map((x: any) => `${x.marker} ${x.seriesName} <b>${fmtInt(x.value)}</b>`);
        return [
          `<b>${new Date(p.ts * 1000).toLocaleString()}</b>`,
          ...rows,
          `成功 ${fmtInt(p.success)} · 不可用 <span style="color:#f43f5e">${fmtInt(p.fail_unavailable)}</span>`,
          `P50 ${fmtMs(p.p50_ms)} · P95 <span style="color:#0284c7">${fmtMs(p.p95_ms)}</span> · P99 ${fmtMs(p.p99_ms)}`,
          `IN ${fmtInt(p.input_tokens)} · OUT ${fmtInt(p.output_tokens)} tok`,
        ].join("<br/>");
      },
    },
    xAxis: {
      type: "category", data: points.map((p) => fmtTs(p.ts)), boundaryGap: true,
      axisTick: { show: false }, axisLine: { lineStyle: { color: border } },
      axisLabel: { color: muted, fontSize: 11 },
    },
    yAxis: [
      { type: "value", name: "请求数", nameTextStyle: { color: muted, fontSize: 11 }, splitLine: { lineStyle: { type: "dashed", color: border } }, axisLabel: { color: muted, fontSize: 11, formatter: compact } },
      { type: "value", name: "P95 (ms)", nameTextStyle: { color: "#0284c7", fontSize: 11 }, splitLine: { show: false }, axisLabel: { color: "#0284c7", fontSize: 11, formatter: compact } },
    ],
    series: [
      { name: "请求数", type: "bar", data: points.map((p) => p.requests), barMaxWidth: 22, itemStyle: { color: "#6366f1", opacity: 0.75, borderRadius: [3, 3, 0, 0] } },
      { name: "P95 (ms)", type: "line", yAxisIndex: 1, data: points.map((p) => p.p95_ms), smooth: true, symbol: "circle", symbolSize: 5, itemStyle: { color: "#0ea5e9" }, lineStyle: { width: 2, color: "#0ea5e9" } },
    ],
  };
  return <EChart option={option} height={280} />;
}

/** TPM 时序图:总量(面积) + 输入/输出分解。 */
function TpmChart({ points }: { points: MetricsSeriesPoint[] }) {
  const border = cssVar("--color-border", "#e4e4e7");
  const muted = cssVar("--color-muted-foreground", "#71717a");
  const fmtTs = (ts: number) => {
    const d = new Date(ts * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };
  const compact = (v: number) => (v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1_000 ? `${(v / 1_000).toFixed(0)}K` : String(v));
  const option: EChartsCoreOption = {
    grid: { left: 8, right: 8, top: 32, bottom: 0, containLabel: true },
    legend: { top: 0, textStyle: { color: muted, fontSize: 11 }, itemWidth: 14, itemHeight: 8 },
    tooltip: {
      trigger: "axis",
      confine: true,
      formatter: (params: any) => {
        const list = Array.isArray(params) ? params : [params];
        const rows = list.map((x: any) => `${x.marker} ${x.seriesName} <b>${Number(x.value).toLocaleString()}</b>`);
        return `${list[0]?.axisValue ?? ""}<br/>${rows.join("<br/>")}`;
      },
    },
    xAxis: {
      type: "category", data: points.map((p) => fmtTs(p.ts)), boundaryGap: false,
      axisTick: { show: false }, axisLine: { lineStyle: { color: border } },
      axisLabel: { color: muted, fontSize: 11 },
    },
    yAxis: [
      { type: "value", splitLine: { lineStyle: { type: "dashed", color: border } }, axisLabel: { color: muted, fontSize: 11, formatter: compact } },
    ],
    series: [
      { name: "总 tokens (IN+OUT)", type: "line", data: points.map((p) => p.input_tokens + p.output_tokens), smooth: true, symbol: "none", itemStyle: { color: "#10b981" }, lineStyle: { width: 2.5, color: "#10b981" }, areaStyle: { color: "#10b981", opacity: 0.12 } },
      { name: "仅 IN tokens", type: "line", data: points.map((p) => p.input_tokens), smooth: true, symbol: "none", itemStyle: { color: "#6366f1" }, lineStyle: { width: 2, color: "#6366f1" } },
    ],
  };
  return <EChart option={option} height={240} />;
}

function MetricsPanel() {
  const [range, setRange] = useState<number>(3600);
  const [busy, setBusy] = useState(false);
  const [data, setData] = useState<MetricsDashboardResp | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setBusy(true);
      setErr(null);
      try {
        const d = await api.metricsDashboard(range === 0 ? undefined : range, 10);
        if (cancelled) return;

        // ============== 数据完整性门禁 ==============
        // 按 Experience 435821:接口返回半成品或缺字段时拒绝 setState,避免 UI 抖动/NaN。
        const bad =
          !d ||
          typeof d !== "object" ||
          !("summary" in d) ||
          !d.summary ||
          !("series" in d) ||
          !Array.isArray(d.series) ||
          !("upstreams" in d) ||
          !Array.isArray(d.upstreams) ||
          typeof d.summary.requests !== "number" ||
          typeof d.summary.success_rate !== "number" ||
          typeof d.summary.rps !== "number" ||
          typeof d.summary.tpm !== "number";
        if (bad) {
          setErr("接口返回格式不完整,已拒绝本次刷新");
          return;
        }
        // 额外防 NaN/Infinity:把 summary 里非有限数强制置 0
        const s = d.summary;
        const sanitize = (n: number) => (Number.isFinite(n) ? n : 0);
        const s2: typeof s = {
          ...s,
          requests: sanitize(s.requests),
          success_rate: sanitize(s.success_rate),
          rps: sanitize(s.rps),
          tpm: sanitize(s.tpm),
          input_tokens: sanitize(s.input_tokens),
          output_tokens: sanitize(s.output_tokens),
          fail_unavailable: sanitize(s.fail_unavailable),
          fail_other: sanitize(s.fail_other),
          avg_ms: sanitize(s.avg_ms),
          p50_ms: sanitize(s.p50_ms),
          p95_ms: sanitize(s.p95_ms),
          p99_ms: sanitize(s.p99_ms),
          range_secs: sanitize(s.range_secs),
          sampled_at: sanitize(s.sampled_at),
        };
        const series2: MetricsSeriesPoint[] = (d.series || [])
          .filter((p: any) => p && typeof p.ts === "number" && typeof p.requests === "number")
          .map((p: any) => ({
            ts: sanitize(p.ts),
            requests: sanitize(p.requests),
            success: sanitize(p.success),
            fail_unavailable: sanitize(p.fail_unavailable),
            fail_other: sanitize(p.fail_other),
            input_tokens: sanitize(p.input_tokens),
            output_tokens: sanitize(p.output_tokens),
            avg_ms: sanitize(p.avg_ms),
            p50_ms: sanitize(p.p50_ms),
            p95_ms: sanitize(p.p95_ms),
            p99_ms: sanitize(p.p99_ms),
          }));
        const ups2: MetricsUpstreamRow[] = (d.upstreams || [])
          .filter(
            (u: any) =>
              u && typeof u.upstream_id === "string" && typeof u.requests === "number"
          )
          .map((u: any) => ({
            upstream_id: u.upstream_id,
            kind: u.kind ?? "",
            base_url: u.base_url ?? "",
            key_fingerprint: u.key_fingerprint ?? "",
            requests: sanitize(u.requests),
            success_rate: sanitize(u.success_rate),
            tpm: sanitize(u.tpm),
            avg_ms: sanitize(u.avg_ms),
            p99_ms: sanitize(u.p99_ms),
            fail_unavailable: sanitize(u.fail_unavailable),
          }));
        setData({
          ...d,
          range_secs: sanitize(d.range_secs),
          sampled_at: sanitize(d.sampled_at),
          summary: s2,
          series: series2,
          upstreams: ups2,
        });
      } catch (e: any) {
        if (cancelled) return;
        setErr(e?.message ?? "读取失败");
      } finally {
        if (!cancelled) setBusy(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [range, tick]);

  useEffect(() => {
    const t = window.setInterval(() => setTick((x) => x + 1), 15_000);
    return () => window.clearInterval(t);
  }, []);

  const s = data?.summary;
  const series = data?.series ?? [];
  const ups = data?.upstreams ?? [];

  return (
    <div className="space-y-4">
      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
        <div>
          <h2 className="text-xl font-semibold text-slate-900 dark:text-slate-100">接口指标仪表盘</h2>
          <p className="mt-0.5 text-xs text-slate-500">
            实时滚动窗口(秒桶 120s / 分钟桶 48h),15s 自动刷新。
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex flex-wrap gap-1 rounded-lg border border-slate-200 bg-white p-1 text-xs shadow-sm dark:border-slate-800 dark:bg-slate-900">
            {RANGE_PRESETS.map((p) => (
              <button
                key={p.secs}
                onClick={() => setRange(p.secs)}
                className={cn(
                  "rounded-md px-2.5 py-1 transition",
                  range === p.secs
                    ? "bg-indigo-600 text-white shadow"
                    : "text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
                )}
              >
                {p.label}
              </button>
            ))}
          </div>
          <Button variant="ghost" size="sm" onClick={() => setTick((x) => x + 1)} disabled={busy} className="gap-1">
            <RefreshCw className={cn("h-4 w-4", busy && "animate-spin")} />
            刷新
          </Button>
        </div>
      </div>

      {err ? (
        <div className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-700">读取失败：{err}</div>
      ) : null}

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <KpiCard
          label="请求量"
          value={fmtInt(s?.requests ?? 0)}
          sub={`RPS ${(s?.rps ?? 0).toFixed(2)} · 窗口 ${
            (s?.range_secs ?? 0) >= 3600
              ? `${((s?.range_secs ?? 0) / 3600).toFixed(1)}h`
              : `${s?.range_secs ?? 0}s`
          }`}
          tone="info"
        />
        <KpiCard
          label="成功率"
          value={fmtPct(s?.success_rate ?? 0)}
          sub={
            (s?.fail_unavailable ?? 0) + (s?.fail_other ?? 0) > 0
              ? `不可用 ${fmtInt(s?.fail_unavailable ?? 0)} · 其他 ${fmtInt(s?.fail_other ?? 0)}`
              : "全部成功 ✓"
          }
          tone={(s?.success_rate ?? 1) >= 0.99 ? "good" : (s?.success_rate ?? 1) >= 0.95 ? "warn" : "bad"}
        />
        <KpiCard
          label="TPM (吞吐)"
          value={fmtBuckets(s?.tpm ?? 0, "/min")}
          sub={`IN ${fmtBuckets(s?.input_tokens ?? 0, "")} · OUT ${fmtBuckets(s?.output_tokens ?? 0, "")}`}
          tone="default"
        />
        <KpiCard
          label="延迟 P95"
          value={fmtMs(s?.p95_ms ?? 0)}
          sub={`avg ${fmtMs(s?.avg_ms ?? 0)} · P50 ${fmtMs(s?.p50_ms ?? 0)} · P99 ${fmtMs(s?.p99_ms ?? 0)}`}
          tone={(s?.p95_ms ?? 0) < 2000 ? "good" : (s?.p95_ms ?? 0) < 8000 ? "warn" : "bad"}
        />
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">请求量(柱状) + P95 延迟(折线)</CardTitle>
            <CardDescription className="text-xs">悬停查看单桶明细。</CardDescription>
          </CardHeader>
          <CardContent className="px-2">
            {series.length === 0 ? (
              <div className="flex h-[260px] items-center justify-center text-sm text-slate-400">
                暂无数据 · 发起一次模型请求即可
              </div>
            ) : (
              <ReqLatencyChart points={series} />
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">Tokens 吞吐时序</CardTitle>
            <CardDescription className="text-xs">输入(紫) vs 输入+输出(绿)；单位:每桶累计 tokens。</CardDescription>
          </CardHeader>
          <CardContent className="px-2">
            {series.length === 0 ? (
              <div className="flex h-[220px] items-center justify-center text-sm text-slate-400">暂无数据</div>
            ) : (
              <TpmChart points={series} />
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <CardTitle className="text-base">上游排行榜 Top {ups.length}</CardTitle>
              <CardDescription className="text-xs">按窗口内总请求数降序。</CardDescription>
            </div>
            <div className="text-xs text-slate-500 tabular-nums">
              {data?.sampled_at ? `采样于 ${new Date(data.sampled_at * 1000).toLocaleString()}` : ""}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[920px] border-collapse text-sm">
              <thead>
                <tr className="text-left text-xs uppercase tracking-wider text-slate-500">
                  <th className="border-b px-3 py-2">#</th>
                  <th className="border-b px-3 py-2">上游 (kind / base_url / key)</th>
                  <th className="border-b px-3 py-2 text-right">请求</th>
                  <th className="border-b px-3 py-2 text-right">成功率</th>
                  <th className="border-b px-3 py-2 text-right">TPM</th>
                  <th className="border-b px-3 py-2 text-right">平均延迟</th>
                  <th className="border-b px-3 py-2 text-right">P99</th>
                  <th className="border-b px-3 py-2 text-right">不可用(次)</th>
                </tr>
              </thead>
              <tbody>
                {ups.length === 0 ? (
                  <tr>
                    <td colSpan={8} className="px-3 py-10 text-center text-slate-400">
                      暂无上游数据 · 调用一次模型即可
                    </td>
                  </tr>
                ) : (
                  ups.map((u, i) => {
                    const ok = u.success_rate;
                    const toneRow =
                      ok >= 0.99
                        ? ""
                        : ok >= 0.95
                        ? "bg-amber-50/40 dark:bg-amber-900/10"
                        : "bg-rose-50/40 dark:bg-rose-900/10";
                    return (
                      <tr key={u.upstream_id} className={toneRow}>
                        <td className="border-b px-3 py-2 font-mono text-xs text-slate-400 tabular-nums">
                          {i + 1}
                        </td>
                        <td className="border-b px-3 py-2">
                          <div className="flex flex-wrap items-center gap-2 text-xs">
                            <span
                              className={cn(
                                "rounded px-1.5 py-0.5 font-semibold",
                                u.kind.toLowerCase().includes("openai")
                                  ? "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/40 dark:text-indigo-200"
                                  : "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200"
                              )}
                            >
                              {u.kind}
                            </span>
                            <span
                              className="max-w-[260px] truncate font-mono text-slate-700 dark:text-slate-200"
                              title={u.base_url}
                            >
                              {u.base_url}
                            </span>
                            <span
                              className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[10px] text-slate-500 dark:bg-slate-800 dark:text-slate-400"
                              title="key fingerprint"
                            >
                              {u.key_fingerprint}
                            </span>
                          </div>
                          <div
                            className="mt-1 truncate font-mono text-[10px] text-slate-400"
                            title={u.upstream_id}
                          >
                            {u.upstream_id}
                          </div>
                        </td>
                        <td className="border-b px-3 py-2 text-right tabular-nums">{fmtInt(u.requests)}</td>
                        <td
                          className={cn(
                            "border-b px-3 py-2 text-right tabular-nums font-semibold",
                            ok >= 0.99
                              ? "text-emerald-600 dark:text-emerald-400"
                              : ok >= 0.95
                              ? "text-amber-600 dark:text-amber-400"
                              : "text-rose-600 dark:text-rose-400"
                          )}
                        >
                          {fmtPct(ok)}
                        </td>
                        <td className="border-b px-3 py-2 text-right tabular-nums">
                          {fmtBuckets(u.tpm, "")}
                        </td>
                        <td className="border-b px-3 py-2 text-right tabular-nums text-slate-600 dark:text-slate-300">
                          {fmtMs(u.avg_ms)}
                        </td>
                        <td
                          className={cn(
                            "border-b px-3 py-2 text-right tabular-nums",
                            u.p99_ms < 2000
                              ? "text-slate-700 dark:text-slate-200"
                              : u.p99_ms < 8000
                              ? "text-amber-600 dark:text-amber-400"
                              : "text-rose-600 dark:text-rose-400"
                          )}
                        >
                          {fmtMs(u.p99_ms)}
                        </td>
                        <td
                          className={cn(
                            "border-b px-3 py-2 text-right tabular-nums",
                            u.fail_unavailable === 0
                              ? "text-slate-400"
                              : "font-semibold text-rose-600 dark:text-rose-400"
                          )}
                        >
                          {fmtInt(u.fail_unavailable)}
                        </td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// ======================== 请求链路追踪 ========================

function attemptStatusColor(status: number): string {
  if (status === 200) return "text-emerald-600 bg-emerald-50 ring-emerald-200";
  if (status === -1) return "text-amber-600 bg-amber-50 ring-amber-200";
  if (status >= 500) return "text-rose-600 bg-rose-50 ring-rose-200";
  if (status > 0) return "text-orange-600 bg-orange-50 ring-orange-200";
  return "text-muted-foreground bg-muted";
}

function attemptStatusLabel(status: number): string {
  if (status === 200) return "200 OK";
  if (status === -1) return "熔断跳过";
  if (status === 503) return "503 不可用";
  if (status === 0) return "未尝试";
  return `${status}`;
}

// 变更方法配色:DELETE 红(最危险)、PUT/PATCH 黄(修改)、POST 靛(新建)。
function auditMethodColor(m: string): string {
  if (m === "DELETE") return "bg-rose-500/10 text-rose-700 ring-1 ring-rose-500/20";
  if (m === "PUT" || m === "PATCH") return "bg-amber-500/10 text-amber-700 ring-1 ring-amber-500/20";
  return "bg-indigo-500/10 text-indigo-700 ring-1 ring-indigo-500/20";
}

function AuditPanel() {
  const [rows, setRows] = useState<AuditLogRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);

  const load = async () => {
    setLoading(true); setErr("");
    try {
      const r = await api.auditLogs(page, pageSize);
      setRows(r.data); setTotal(r.total);
    } catch (e: any) {
      setErr(e?.message || "加载失败");
    } finally { setLoading(false); }
  };

  useEffect(() => { load(); }, [page, pageSize]);

  const fmtTime = (ts: number) => new Date(ts * 1000).toLocaleString("zh-CN", { hour12: false });

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div>
              <CardTitle className="text-base">操作审计({loading ? "…" : total})</CardTitle>
              <p className="mt-1 text-xs text-muted-foreground">记录管理端全部变更操作(POST/PUT/PATCH/DELETE),含登录尝试。</p>
            </div>
            <Button size="sm" variant="outline" className="h-8" onClick={() => load()}>
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {err ? (
            <div className="flex flex-col items-center gap-2 py-8">
              <p className="text-sm text-destructive">{err}</p>
              <Button size="sm" variant="outline" onClick={() => load()}>重试</Button>
            </div>
          ) : loading ? (
            <p className="text-sm text-muted-foreground">加载中…</p>
          ) : rows.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无审计记录</p>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>时间</TableHead>
                    <TableHead>操作者</TableHead>
                    <TableHead>方法</TableHead>
                    <TableHead>路径</TableHead>
                    <TableHead>状态</TableHead>
                    <TableHead>IP</TableHead>
                    <TableHead className="text-right">耗时</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((r, i) => (
                    <TableRow key={i}>
                      <TableCell className="mono text-xs whitespace-nowrap">{fmtTime(r.ts)}</TableCell>
                      <TableCell className="text-xs font-medium">{r.actor}</TableCell>
                      <TableCell>
                        <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold", auditMethodColor(r.method))}>
                          {r.method}
                        </span>
                      </TableCell>
                      <TableCell className="mono max-w-[320px] truncate text-xs" title={r.path}>{r.path}</TableCell>
                      <TableCell>
                        <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold ring-1 ring-inset", attemptStatusColor(r.status))}>
                          {r.status}
                        </span>
                      </TableCell>
                      <TableCell className="mono text-xs text-muted-foreground">{r.ip}</TableCell>
                      <TableCell className="text-right text-xs whitespace-nowrap">{r.latency_ms}ms</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function RequestLogPanel() {
  const [rows, setRows] = useState<RequestLogRow[]>([]);
  const [q, setQ] = useState("");
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [failed, setFailed] = useState<"all" | "ok" | "fail">("all");
  const [hours, setHours] = useState<number | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const qRef = useRef("");

  const load = async () => {
    setLoading(true); setErr("");
    try {
      const r = await api.requestLogs(
        qRef.current || undefined, page, pageSize,
        failed === "all" ? null : failed === "fail",
        hours,
      );
      setRows(r.data); setTotal(r.total);
    } catch (e: any) {
      setErr(e?.message || "加载失败");
    } finally { setLoading(false); }
  };

  useEffect(() => { load(); }, [page, pageSize, failed, hours]);
  // 卸载时清掉搜索防抖定时器,避免组件销毁后 setState。
  useEffect(() => () => { if (timerRef.current) clearTimeout(timerRef.current); }, []);

  const onSearch = (v: string) => {
    setQ(v);
    qRef.current = v;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      if (page !== 1) setPage(1);
      else load();
    }, 500);
  };

  const onFailed = (v: "all" | "ok" | "fail") => { setFailed(v); setPage(1); };
  const onHours = (v: string) => { setHours(v ? Number(v) : null); setPage(1); };

  const fmtTime = (ts: number) => {
    const d = new Date(ts * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${d.toLocaleTimeString("zh-CN", { hour12: false })}.${String(d.getMilliseconds()).padStart(3, "0")}`;
  };

  const toggle = (id: string) => setExpanded((prev) => prev === id ? null : id);

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">请求链路({loading ? "…" : total})</CardTitle>
            <div className="flex items-center gap-2">
              <div className="relative">
                <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  placeholder="搜索 request_id / 模型…"
                  className="h-8 w-56 pl-7 text-xs"
                  value={q}
                  onChange={(e) => onSearch(e.target.value)}
                />
              </div>
              <select value={failed} onChange={(e) => onFailed(e.target.value as "all" | "ok" | "fail")}
                className="h-8 rounded-lg border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring">
                <option value="all">全部状态</option>
                <option value="fail">仅失败</option>
                <option value="ok">仅成功</option>
              </select>
              <select value={hours ?? ""} onChange={(e) => onHours(e.target.value)}
                className="h-8 rounded-lg border border-input bg-card px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring">
                <option value="">全部时间</option>
                <option value="24">最近 24 小时</option>
                <option value="72">最近 3 天</option>
                <option value="168">最近 7 天</option>
              </select>
              <Button size="sm" variant="outline" className="h-8" onClick={() => load()}>
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {err ? (
            <div className="flex flex-col items-center gap-2 py-8">
              <p className="text-sm text-destructive">{err}</p>
              <Button size="sm" variant="outline" onClick={() => load()}>重试</Button>
            </div>
          ) : loading ? (
            <p className="text-sm text-muted-foreground">加载中…</p>
          ) : rows.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无请求记录</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8" />
                  <TableHead>时间</TableHead>
                  <TableHead>用户</TableHead>
                  <TableHead>模型</TableHead>
                  <TableHead>路径</TableHead>
                  <TableHead>最终上游</TableHead>
                  <TableHead>状态</TableHead>
                  <TableHead className="text-right">Tokens</TableHead>
                  <TableHead className="text-right">延迟</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r) => {
                  const isOpen = expanded === r.request_id;
                  const isFail = r.final_status !== 200;
                  // 命中供应商:取首个成功尝试的 provider,缺省回退 final_kind(旧记录无 provider 字段)
                  const hitProvider = r.attempts.find((a) => a.status === 200)?.provider || r.final_kind || "";
                  return (
                    <React.Fragment key={r.request_id}>
                      <TableRow
                        className={cn(
                          "cursor-pointer transition-colors",
                          isFail && "bg-rose-500/[0.04] hover:bg-rose-500/[0.07]",
                        )}
                        onClick={() => toggle(r.request_id)}
                      >
                        <TableCell className="w-8 px-1">
                          <span className={cn("inline-block text-xs transition-transform", isOpen && "rotate-90")}>▶</span>
                        </TableCell>
                        <TableCell className="mono text-xs whitespace-nowrap">{fmtTime(r.ts)}</TableCell>
                        <TableCell className="mono text-xs text-muted-foreground">{r.user_id.slice(0, 8)}</TableCell>
                        <TableCell className="text-xs">{r.requested_model}</TableCell>
                        <TableCell>
                          <Badge
                            variant={r.path === "chat" ? "default" : "muted"}
                            className={cn(
                              "text-[10px]",
                              r.path === "embeddings" && "bg-emerald-500/10 text-emerald-700 ring-1 ring-emerald-500/20",
                            )}
                          >
                            {r.path}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs">
                          {r.final_upstream_model ? (
                            <span className="inline-flex items-center gap-1">
                              <span className="text-muted-foreground">{hitProvider}</span>
                              <span>{r.final_upstream_model}</span>
                            </span>
                          ) : (
                            <span className="text-muted-foreground">—</span>
                          )}
                        </TableCell>
                        <TableCell>
                          <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold ring-1 ring-inset", attemptStatusColor(r.final_status))}>
                            {attemptStatusLabel(r.final_status)}
                          </span>
                        </TableCell>
                        <TableCell className="text-right mono text-xs whitespace-nowrap">
                          {r.input_tokens > 0 || r.output_tokens > 0 ? (
                            <span>↑{r.input_tokens.toLocaleString()} ↓{r.output_tokens.toLocaleString()}</span>
                          ) : <span className="text-muted-foreground">—</span>}
                        </TableCell>
                        <TableCell className="text-right text-xs whitespace-nowrap">
                          {r.latency_ms > 0 ? <span>{r.latency_ms.toLocaleString()}ms</span> : <span className="text-muted-foreground">—</span>}
                        </TableCell>
                      </TableRow>
                      {isOpen && (
                        <TableRow className="bg-muted/30 hover:bg-muted/30">
                          <TableCell colSpan={9} className="p-0">
                            <div className="space-y-3 px-4 py-3">
                              {/* 候选顺序 */}
                              <div>
                                <p className="mb-1 text-xs font-medium text-muted-foreground">候选顺序（负载策略）</p>
                                <table className="w-full text-xs">
                                  <thead>
                                    <tr className="border-b text-left text-muted-foreground">
                                      <th className="pb-1 font-normal">#</th>
                                      <th className="pb-1 font-normal">供应商</th>
                                      <th className="pb-1 font-normal">上游</th>
                                      <th className="pb-1 font-normal">模型</th>
                                      <th className="pb-1 font-normal">权重</th>
                                      <th className="pb-1 font-normal">状态</th>
                                    </tr>
                                  </thead>
                                  <tbody>
                                    {r.candidates.map((c, i) => (
                                      <tr key={i} className="border-b border-dashed last:border-0">
                                        <td className="py-1 text-muted-foreground">{i + 1}</td>
                                        <td className="py-1 font-medium">{c.provider || "—"}</td>
                                        <td className="py-1">
                                          <span className="text-muted-foreground">{c.kind}</span>
                                          <span className="ml-1 text-muted-foreground/60">{c.base_url.length > 30 ? c.base_url.slice(0, 30) + "…" : c.base_url}</span>
                                        </td>
                                        <td className="py-1">{c.upstream_model}</td>
                                        <td className="py-1 mono">{c.weight}</td>
                                        <td className="py-1">
                                          {(() => {
                                            // 在 attempts 中查找对应候选的状态(熔断跳过条目 kind="skipped",按上游模型兜底匹配)
                                            const att = r.attempts.find((a) => (a.kind === c.kind || a.kind === "skipped") && a.upstream_model === c.upstream_model);
                                            if (att) {
                                              return <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold ring-1 ring-inset", attemptStatusColor(att.status))}>{attemptStatusLabel(att.status)}</span>;
                                            }
                                            // 有 status=0 说明在 candidates 里但未尝试
                                            return <span className="text-muted-foreground">未尝试</span>;
                                          })()}
                                        </td>
                                      </tr>
                                    ))}
                                  </tbody>
                                </table>
                              </div>
                              {/* 实际尝试链:降级时间线 */}
                              {r.attempts.length > 0 && (
                                <div>
                                  <p className="mb-2 text-xs font-medium text-muted-foreground">实际尝试（failover 时间线）</p>
                                  <div className="relative ml-2 space-y-3 border-l border-border pl-4">
                                    {r.attempts.map((a, i) => {
                                      const dot = a.status === 200
                                        ? "bg-success"
                                        : a.status === -1 || a.status === 503
                                          ? "bg-warning"
                                          : "bg-destructive";
                                      return (
                                        <div key={i} className="relative">
                                          <span className={cn("absolute -left-[21px] top-1 h-[9px] w-[9px] rounded-full ring-2 ring-background", dot)} />
                                          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs">
                                            <span className="font-medium">{a.provider || a.kind}</span>
                                            <span className="text-muted-foreground">{a.provider ? `${a.kind} · ` : ""}{a.upstream_model}</span>
                                            <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold ring-1 ring-inset", attemptStatusColor(a.status))}>
                                              {attemptStatusLabel(a.status)}
                                            </span>
                                            <span className="mono text-muted-foreground">{a.latency_ms > 0 ? `${a.latency_ms}ms` : ""}</span>
                                            {a.status === 200 && <span className="text-[10px] font-semibold text-success">✓ 命中</span>}
                                          </div>
                                          {a.error && (
                                            <p className="mt-0.5 max-w-[420px] truncate text-[11px] text-muted-foreground" title={a.error}>{a.error}</p>
                                          )}
                                        </div>
                                      );
                                    })}
                                  </div>
                                </div>
                              )}
                              {/* 请求/响应体预览(跟随存储后端;旧记录或未采集时为空) */}
                              {(r.req_body || r.resp_body) && (
                                <div className="grid gap-3 md:grid-cols-2">
                                  {([["请求体", r.req_body], ["响应体", r.resp_body]] as const).map(([label, body]) => body ? (
                                    <div key={label}>
                                      <p className="mb-1 text-xs font-medium text-muted-foreground">{label}预览</p>
                                      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-lg border bg-background p-2 text-[11px] leading-relaxed text-muted-foreground">{body}</pre>
                                    </div>
                                  ) : null)}
                                </div>
                              )}
                              {/* Tokens 汇总 */}
                              <div className="flex items-center gap-4 text-xs text-muted-foreground">
                                <span>Tokens: <span className="mono text-foreground">↑{r.input_tokens.toLocaleString()} ↓{r.output_tokens.toLocaleString()}</span></span>
                                {r.charged_tokens > 0 && <span>计费: <span className="mono text-foreground">{r.charged_tokens.toLocaleString()}</span></span>}
                                {r.stream && <Badge variant="muted" className="text-[10px]">stream</Badge>}
                              </div>
                            </div>
                          </TableCell>
                        </TableRow>
                      )}
                    </React.Fragment>
                  );
                })}
              </TableBody>
            </Table>
          )}
          <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
        </CardContent>
      </Card>
    </div>
  );
}

