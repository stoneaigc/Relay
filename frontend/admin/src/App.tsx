import React, { useEffect, useRef, useState } from "react";
import { BrowserRouter, Routes, Route, NavLink, Navigate, useLocation } from "react-router-dom";
import { Users as UsersIcon, Boxes, Layers, BarChart3, LayoutDashboard, LogOut, Plus, Power, Menu, X, Trash2, Pencil, TrendingUp, Activity, Star, Gift, Check, ExternalLink, Settings, Send, Zap, RefreshCw, Clock, ShieldAlert, ShieldCheck, Cpu, Search, RotateCcw, AlertTriangle, Link2, GitBranch } from "lucide-react";
import { api, getToken, setToken, clearToken, UserRow, ModelRow, ProviderRow, ProviderHealthItem, GroupRow, RouteRow, RewardClaimRow, RewardTaskRow, RewardTaskBody, EvidenceType, EmailSettingsResp, UpstreamRow, UpstreamsResp, FailureRow, AuditFailuresResp, MetricsSeriesPoint, MetricsDashboardResp, MetricsUpstreamRow, RequestLogRow, RequestAttempt, TimeRuleRow, TimeRulePayload } from "./api";
import { Button } from "@/components/ui/button";
import { RowActions } from "@/components/ui/row-actions";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import PaginationBar from "./PaginationBar";
import { PasswordInput } from "@/components/ui/password-input";
import { Toaster } from "sonner";

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  return (
    <>
      <Toaster position="top-center" richColors />
      {!authed ? (
        <Login onSuccess={() => setAuthed(true)} />
      ) : (
        <BrowserRouter basename="/admin">
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
  const submit = async () => {
    setErr("");
    try { const r = await api.login(u, p); setToken(r.token); onSuccess(); } catch { setErr("用户名或密码错误"); }
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
          <Button className="w-full" onClick={submit}>登录</Button>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </CardContent>
      </Card>
    </div>
  );
}

const NAV: { path: string; label: string; icon: any }[] = [
  { path: "/overview", label: "概览", icon: LayoutDashboard },
  { path: "/users", label: "用户管理", icon: UsersIcon },
  { path: "/models", label: "模型", icon: Boxes },
  { path: "/upstreams", label: "上游治理", icon: Zap },
  { path: "/metrics", label: "接口指标", icon: Activity },
  { path: "/request-logs", label: "请求链路", icon: GitBranch },
  { path: "/groups", label: "模型组", icon: Layers },
  { path: "/rewards", label: "奖励审核", icon: Gift },
  { path: "/reward-tasks", label: "奖励设置", icon: Star },
  { path: "/usage", label: "全局用量", icon: BarChart3 },
  { path: "/settings", label: "设置", icon: Settings },
];

function Console({ onLogout }: { onLogout: () => void }) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const loc = useLocation();
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
      <nav className="flex-1 space-y-1 px-2">
        {NAV.map((n) => (
          <NavLink key={n.path} to={n.path} onClick={() => setMobileOpen(false)}
            className={({ isActive }) => cn("flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
              isActive ? "bg-accent font-medium text-foreground" : "text-muted-foreground hover:bg-accent")}>
            <n.icon className="h-4 w-4 shrink-0" /> {n.label}
          </NavLink>
        ))}
      </nav>
      <div className="px-2 py-3">
        <Button variant="ghost" size="sm" className="w-full justify-start gap-3 text-muted-foreground" onClick={onLogout}>
          <LogOut className="h-4 w-4" />退出登录
        </Button>
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
                <Route path="/rewards" element={<RewardsPanel />} />
                <Route path="/reward-tasks" element={<RewardTasksPanel />} />
                <Route path="/usage" element={<UsagePanel />} />
                <Route path="/settings" element={<SettingsPanel />} />
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

function LineChart({ points }: { points: { ts: number; tokens: number }[] }) {
  const W = 600, H = 160, pad = 10;
  const n = points.length;
  const max = Math.max(...points.map((p) => p.tokens), 1);
  const x = (i: number) => (n <= 1 ? W / 2 : pad + (i * (W - 2 * pad)) / (n - 1));
  const y = (v: number) => H - pad - (v / max) * (H - 2 * pad);
  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(p.tokens).toFixed(1)}`).join(" ");
  const area = n > 0 ? `${line} L${x(n - 1).toFixed(1)},${H - pad} L${x(0).toFixed(1)},${H - pad} Z` : "";
  return (
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="w-full" style={{ height: H }}>
      <path d={area} fill="var(--color-primary)" opacity="0.12" />
      <path d={line} fill="none" stroke="var(--color-primary)" strokeWidth="2" vectorEffect="non-scaling-stroke" />
      {points.map((p, i) => p.tokens > 0 ? <circle key={i} cx={x(i)} cy={y(p.tokens)} r="2.5" fill="var(--color-primary)" /> : null)}
    </svg>
  );
}

function UserChartDialog({ user, onClose }: { user: UserRow | null; onClose: () => void }) {
  const [data, setData] = useState<{ ts: number; tokens: number; calls: number }[]>([]);
  useEffect(() => { if (user) { setData([]); api.userSeries(user.id).then((r) => setData(r.data)); } }, [user]);
  const fmt = (ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; };
  const total = data.reduce((a, p) => a + p.tokens, 0);
  const peak = Math.max(0, ...data.map((d) => d.tokens));
  return (
    <Dialog open={!!user} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{user?.username || user?.phone || "用户"} · 用量趋势</DialogTitle>
          <DialogDescription>最近 30 天 · 共 {total.toLocaleString()} tokens</DialogDescription>
        </DialogHeader>
        {data.length === 0 ? <p className="text-sm text-muted-foreground">加载中或暂无数据…</p> : (
          <div>
            <LineChart points={data} />
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
  const [err, setErr] = useState("");
  useEffect(() => {
    if (open) {
      setUsername(edit?.username ?? ""); setPassword(""); setGroupId(edit?.group_id ?? 0);
      setEmail(edit?.email ?? ""); setPhone(edit?.phone ?? "");
      setConc(String(edit?.concurrency_limit ?? 16));
      setAmount(edit ? "0" : "10000000"); setErr("");
    }
  }, [dlg]);

  const submit = async () => {
    setErr("");
    try {
      if (edit) {
        const body: any = {
          email: email.trim(), phone: phone.trim(),
          group_id: groupId, concurrency_limit: Number(conc),
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
            <LineChart points={data} />
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

  const load = async () => {
    try {
      const [u, g] = await Promise.all([api.users(page, pageSize), api.groups()]);
      setRows(u.data); setGroups(g.data); setTotal(u.total);
    } catch (e: any) { if (String(e.message).includes("auth")) onAuthErr(); }
  };
  useEffect(() => { load(); }, [page, pageSize]);

  const filtered = rows.filter((u) =>
    !q.trim() || (u.username || "").includes(q.trim()) || (u.phone || "").includes(q.trim()) || u.id.includes(q.trim()));
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
            <CardTitle className="text-base">所有用户({filtered.length}/{rows.length})</CardTitle>
            <div className="flex items-center gap-2">
              <Input className="w-56" placeholder="搜索手机号 / ID" value={q} onChange={(e) => { setQ(e.target.value); if (page !== 1) setPage(1); }} />
              <Button size="sm" onClick={() => setUserDlg({ edit: null })}><Plus className="h-4 w-4" />创建用户</Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>用户</TableHead><TableHead>来源</TableHead><TableHead>总量</TableHead><TableHead>已用</TableHead>
                <TableHead>今日</TableHead><TableHead>余额</TableHead><TableHead>并发</TableHead>
                <TableHead>模型组</TableHead><TableHead>状态</TableHead><TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((u) => (
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

function HealthBadge({ h }: { h?: ProviderHealthItem }) {
  if (!h) return null;
  const meta = HEALTH_META[h.status] ?? HEALTH_META.idle;
  const tip = h.status === "idle"
    ? "统计窗口内无请求"
    : `统计窗口内 ${h.requests} 次请求 · 成功率 ${(h.success_rate * 100).toFixed(1)}% · 平均 ${Math.round(h.avg_ms)}ms · P95 ${h.p95_ms}ms${h.breaker?.fail_count ? ` · 熔断计数 ${h.breaker.fail_count}/${h.breaker.threshold}` : ""}`;
  return (
    <span title={tip} className={cn("inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium", meta.cls)}>
      <span className={cn("h-1.5 w-1.5 rounded-full", meta.dot, meta.pulse && "animate-pulse")} />
      {meta.label}
    </span>
  );
}

function ModelsPanel() {
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [providerModels, setProviderModels] = useState<Record<string, { id: number; upstream_model: string; label: string | null }[]>>({});
  const [addDlg, setAddDlg] = useState(false);
  const [editProvider, setEditProvider] = useState<ProviderRow | null>(null);
  const [editModel, setEditModel] = useState<ModelRow | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [test, setTest] = useState<Record<number, "loading" | { ok: boolean; msg: string }>>({});
  const [page, setPage] = useState(1);
  const [pageMeta, setPageMeta] = useState({ total: 0, total_pages: 1 });
  const [providerSearch, setProviderSearch] = useState("");
  const [selected, setSelected] = useState<Set<number>>(new Set());
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
    title: `删除上游「${p.name}」?`,
    desc: `将删除该上游下的 ${p.model_count} 个模型及其所有路由，不可恢复。`,
    action: async () => { await api.deleteProvider(p.name); loadProviders(); },
  });

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

  // 获取 provider 的可读名称
  const providerDisplayName = (url: string) => {
    const h = url.replace(/^https?:\/\//, "").split("/")[0];
    if (h.includes("deepseek")) return "DeepSeek";
    if (h.includes("openai")) return "OpenAI";
    if (h.includes("anthropic")) return "Anthropic";
    if (h.includes("dashscope")) return "通义千问";
    if (h.includes("bigmodel")) return "智谱 GLM";
    if (h.includes("moonshot")) return "Moonshot";
    if (h.includes("localhost") || h.includes("127.0.0.1")) return "本地服务";
    return h;
  };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">上游供应商({pageMeta.total})</CardTitle>
            <Button size="sm" onClick={() => setAddDlg(true)}><Plus className="h-4 w-4" />添加上游</Button>
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
                  <Zap className="h-4 w-4 text-primary" />
                  <div>
                    <div className="flex items-center gap-2 text-sm font-medium">
                      {providerDisplayName(p.base_url)}
                      <HealthBadge h={health[p.name]} />
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
                      {selectedInProvider(p.name).length > 0 && (
                        <Button variant="destructive" size="sm" className="h-7 text-xs" onClick={() => delSelected(p.name, selectedInProvider(p.name))}>
                          <Trash2 className="h-3 w-3" />删除所选({selectedInProvider(p.name).length})
                        </Button>
                      )}
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
                      </div>
                      <div className="flex items-center gap-2">
                        {(() => {
                          const r = test[m.id];
                          if (r === "loading") return <span className="text-xs text-muted-foreground">校验中…</span>;
                          if (r) return <span className={cn("text-xs", r.ok ? "text-success" : "text-destructive")} title={r.msg}>{r.ok ? `✓ ${r.msg}` : `✗ ${r.msg}`}</span>;
                          return null;
                        })()}
                        <Button variant="ghost" size="sm" className="h-7 px-2" onClick={() => runTest(m.id)}><Activity className="h-3 w-3" /></Button>
                        <Button variant="ghost" size="sm" className="h-7 px-2" onClick={() => setEditModel({ id: m.id, label: m.label, kind: p.kind, base_url: p.base_url, upstream_model: m.upstream_model, provider: p.name })}><Pencil className="h-3 w-3" /></Button>
                        <Button variant="ghost" size="sm" className="h-7 px-2 text-destructive" onClick={() => delModel(p.name, m)}><Trash2 className="h-3 w-3" /></Button>
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
      {editModel && <EditModelDialog model={editModel} onClose={() => setEditModel(null)} onSaved={() => { setEditModel(null); loadProviders(); }} />}
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

// ---- 编辑供应商对话框 ----
function EditProviderDialog({ provider, onClose, onSaved }: { provider: ProviderRow; onClose: () => void; onSaved: () => void }) {
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [apiKey, setApiKey] = useState("");
  const [err, setErr] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async () => {
    setErr(""); setSubmitting(true);
    try {
      await api.updateProvider(provider.name, { base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined });
      onSaved();
    } catch (e: any) { setErr(e.message); }
    setSubmitting(false);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader><DialogTitle>编辑供应商</DialogTitle><DialogDescription>{provider.name}</DialogDescription></DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">Base URL</label>
            <Input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">API Key</label>
            <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="留空保持原密钥" /></div>
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

// ---- 添加上游对话框 ----
function AddProviderDialog({ open, onClose, onSaved }: { open: boolean; onClose: () => void; onSaved: () => void }) {
  const [kind, setKind] = useState("openai");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [selectedModels, setSelectedModels] = useState<Set<string>>(new Set());
  const [fetching, setFetching] = useState(false);
  const [err, setErr] = useState("");
  const [submitting, setSubmitting] = useState(false);

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
  const [tpl, setTpl] = useState("custom");
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

  const toggle = (m: string) => {
    setSelectedModels((prev) => { const n = new Set(prev); if (n.has(m)) n.delete(m); else n.add(m); return n; });
  };
  const toggleAll = () => {
    if (selectedModels.size === fetchedModels.length) setSelectedModels(new Set());
    else setSelectedModels(new Set(fetchedModels));
  };

  const submit = async () => {
    setErr(""); setSubmitting(true);
    try {
      if (selectedModels.size > 0) {
        const items = [...selectedModels].map((m) => ({ upstream_model: m, label: m }));
        await api.addModelsBatch({ kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, models: items });
      } else {
        await api.addModel({ kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, upstream_model: "custom-model" });
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
          <Button type="button" variant="outline" size="sm" onClick={doFetch} disabled={fetching || !baseUrl.trim()} className="w-full">
            <Search className="h-4 w-4 mr-1" />{fetching ? "探测中..." : "探测模型列表"}
          </Button>
          {fetchedModels.length > 0 && (
            <div className="rounded-lg border bg-card">
              <div className="flex items-center justify-between border-b px-3 py-2">
                <span className="text-sm font-medium">勾选模型 ({selectedModels.size}/{fetchedModels.length})</span>
                <button className="text-xs text-primary hover:underline" onClick={toggleAll}>{selectedModels.size === fetchedModels.length ? "取消全选" : "全选"}</button>
              </div>
              <div className="max-h-52 overflow-y-auto">
                {fetchedModels.map((m) => (
                  <label key={m} className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm hover:bg-accent">
                    <input type="checkbox" checked={selectedModels.has(m)} onChange={() => toggle(m)} className="h-4 w-4 rounded border-input" />
                    <span className="flex-1">{m}</span>
                    {selectedModels.has(m) && <Check className="h-4 w-4 text-primary" />}
                  </label>
                ))}
              </div>
            </div>
          )}
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
  const [err, setErr] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const submit = async () => {
    setErr("");
    try {
      await api.updateModel(model.id, { kind: model.kind || "openai", base_url: model.base_url || "", upstream_model: upstream.trim(), label: label.trim() || undefined });
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
          <div><label className="mb-1 block text-xs text-muted-foreground">备注</label>
            <Input value={label} onChange={(e) => setLabel(e.target.value)} /></div>
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

function GroupsPanel() {
  const [groups, setGroups] = useState<GroupRow[]>([]);
  const [models, setModels] = useState<ModelRow[]>([]);
  const [sel, setSel] = useState<number | null>(null);
  const [routes, setRoutes] = useState<RouteRow[]>([]);
  const [timeRules, setTimeRules] = useState<TimeRuleRow[]>([]);
  const [gq, setGq] = useState("");
  const [groupOpen, setGroupOpen] = useState(false);
  const [routeDlg, setRouteDlg] = useState<{ edit: RouteRow | null } | null>(null);
  const [timeRuleDlg, setTimeRuleDlg] = useState<{ edit: TimeRuleRow | null } | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const loadGroups = async () => {
    const [g, m] = await Promise.all([api.groups(), api.models()]);
    setGroups(g.data); setModels(m.data);
    if (sel == null && g.data.length) setSel(g.data[0].id);
  };
  const loadRoutes = async (gid: number) => { setRoutes((await api.routes(gid)).data); };
  const loadTimeRules = async (gid: number) => { setTimeRules((await api.timeRules(gid)).data); };
  useEffect(() => { loadGroups(); }, []);
  useEffect(() => { if (sel != null) { loadRoutes(sel); loadTimeRules(sel); } }, [sel]);

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
  };
  const curStrategy = groups.find((g) => g.id === sel)?.strategy ?? "weighted_random";
  const filteredGroups = groups.filter((g) => !gq.trim() || g.name.includes(gq.trim()));

  return (
    <div className="grid h-full min-h-0 gap-5 md:grid-cols-[260px_1fr]">
      <Card className="flex h-full min-h-0 flex-col">
        <CardHeader><CardTitle className="text-base">模型组</CardTitle></CardHeader>
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
                  <button title={g.is_active ? "当前默认组" : "设为默认(新用户注册自动绑定)"}
                    className={cn("transition-colors", g.is_active ? "text-amber-500" : "opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-amber-500")}
                    onClick={(e) => { e.stopPropagation(); setActive(g); }}>
                    <Star className={cn("h-3.5 w-3.5", g.is_active && "fill-current")} />
                  </button>
                  <button className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive" onClick={(e) => { e.stopPropagation(); delGroup(g); }}><Trash2 className="h-3.5 w-3.5" /></button>
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
                  </select>
                </div>
              )}
              {sel != null && <Button size="sm" onClick={() => setRouteDlg({ edit: null })}><Plus className="h-4 w-4" />添加路由</Button>}
            </div>
          </div>
          {curStrategy === "priority" && <p className="mt-1 text-[11px] text-muted-foreground">优先级模式:数字越大越优先;主模型故障时自动切换到下一个。</p>}
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
              <p className="mb-3 text-xs text-muted-foreground">用户请求「对外模型名」→ 路由到指定模型;响应里保留用户传的名字。同名多条按权重/优先级分流。</p>
              <Table>
                <TableHeader><TableRow>
                  <TableHead>对外模型名</TableHead><TableHead>→ 实际模型</TableHead>
                  <TableHead>{curStrategy === "priority" ? "优先级" : "权重"}</TableHead>
                  <TableHead>倍率</TableHead><TableHead className="text-right">操作</TableHead>
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
                        <TableRow key={r.id}>
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
                  {routes.length === 0 && <TableRow><TableCell colSpan={5} className="text-sm text-muted-foreground">该组暂无路由</TableCell></TableRow>}
                </TableBody>
              </Table>
            </>
          )}
        </CardContent>
      </Card>

      {sel != null && <AddRouteDialog dlg={routeDlg} groupId={sel} models={models} strategy={curStrategy} onClose={() => setRouteDlg(null)} onSaved={() => loadRoutes(sel)} />}
      {sel != null && <TimeRuleDialog dlg={timeRuleDlg} groupId={sel} onClose={() => setTimeRuleDlg(null)} onSaved={() => loadTimeRules(sel)} />}
      <AddGroupDialog open={groupOpen} onClose={() => setGroupOpen(false)} onCreate={createGroup} />
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
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

function AddRouteDialog({ dlg, groupId, models, strategy, onClose, onSaved }: {
  dlg: { edit: RouteRow | null } | null; groupId: number; models: ModelRow[]; strategy: string; onClose: () => void; onSaved: () => void;
}) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const [publicName, setPublicName] = useState("");
  const [modelId, setModelId] = useState<number | "">("");
  const [weight, setWeight] = useState("100");
  const [mult, setMult] = useState("1");
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
      } else {
        setPublicName(""); setModelId(models[0]?.id ?? "");
        setWeight("100"); setMult("1");
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
        await api.updateRoute(edit.id, { public_name: publicName.trim(), model_id: Number(modelId), weight: Number(weight) || 100, multiplier: Number(mult) || 1 });
      } else if (selectedModels.size > 0) {
        const routes = [...selectedModels].map(id => {
          const row = routeRows.get(id);
          return { public_name: row?.public_name || "", model_id: id, weight: Number(row?.weight) || 100, multiplier: Number(row?.multiplier) || 1 };
        }).filter(r => r.public_name.trim());
        if (routes.length === 0) { setErr("请至少填写一个对外模型名"); setSubmitting(false); return; }
        await api.addRoutesBatch(groupId, { routes });
      } else {
        await api.addRoute(groupId, { public_name: publicName.trim(), model_id: Number(modelId), weight: Number(weight) || 100, multiplier: Number(mult) || 1 });
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

function UsagePanel() {
  const [rows, setRows] = useState<any[]>([]);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(50);
  const [total, setTotal] = useState(0);
  useEffect(() => { api.usage(page, pageSize).then((r) => { setRows(r.data); setTotal(r.total); }); }, [page, pageSize]);
  return (
    <Card>
      <CardHeader><CardTitle className="text-base">全局用量({total})</CardTitle></CardHeader>
      <CardContent>
        {rows.length === 0 ? <p className="text-sm text-muted-foreground">暂无记录</p> : (
          <Table>
            <TableHeader><TableRow><TableHead>用户</TableHead><TableHead>模型</TableHead><TableHead>供应商</TableHead><TableHead>输入</TableHead><TableHead>输出</TableHead><TableHead>计费</TableHead></TableRow></TableHeader>
            <TableBody>
              {rows.map((r, i) => (
                <TableRow key={i}>
                  <TableCell className="mono text-xs">{String(r.user_id).slice(0, 8)}</TableCell>
                  <TableCell className="mono">{r.model}</TableCell><TableCell>{r.provider}</TableCell>
                  <TableCell>{r.input_tokens}</TableCell><TableCell>{r.output_tokens}</TableCell><TableCell className="mono">{r.charged_tokens}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <PaginationBar page={page} pageSize={pageSize} total={total} onPageChange={setPage} onPageSizeChange={(s) => { setPage(1); setPageSize(s); }} />
      </CardContent>
    </Card>
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
    </div>
  );
}

// ============================================================
// 上游治理仪表盘(熔断器状态 + 并发槽占用率)
// ============================================================

type BreakerTone = "healthy" | "warn" | "broken";
function breakerTone(fc: number, threshold: number, broken: boolean): BreakerTone {
  if (broken) return "broken";
  if (fc === 0) return "healthy";
  if (fc >= threshold - 1) return "warn";
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
                            {new Date(r.ts_ms).toLocaleTimeString()}
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
function ReqLatencyChart({ points }: { points: MetricsSeriesPoint[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 260 });
  const [hover, setHover] = useState<number | null>(null);
  const PAD = { l: 52, r: 60, t: 16, b: 28 };

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver((es) => {
      const r = es[0].contentRect;
      setSize({ w: Math.max(320, Math.floor(r.width)), h: 260 });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const plotW = size.w - PAD.l - PAD.r;
  const plotH = size.h - PAD.t - PAD.b;
  const n = points.length;
  const maxReq = Math.max(1, ...points.map((p) => p.requests));
  const maxLat = Math.max(1, ...points.map((p) => p.p95_ms));

  const xAt = (i: number) => PAD.l + (n <= 1 ? plotW / 2 : (plotW * i) / (n - 1));
  const yReq = (v: number) => PAD.t + plotH - (plotH * v) / maxReq;
  const yLat = (v: number) => PAD.t + plotH - (plotH * v) / maxLat;

  const barW = Math.max(2, Math.min(22, (plotW / Math.max(1, n)) * 0.7));
  const barsPath = points
    .map((p, i) => {
      const x = xAt(i);
      const x0 = x - barW / 2;
      const y = yReq(p.requests);
      return `M${x0.toFixed(2)},${(PAD.t + plotH).toFixed(2)} L${x0.toFixed(2)},${y.toFixed(
        2
      )} L${(x0 + barW).toFixed(2)},${y.toFixed(2)} L${(x0 + barW).toFixed(2)},${(PAD.t + plotH).toFixed(2)} Z`;
    })
    .join(" ");
  const linePath = points
    .map((p, i) => `${i === 0 ? "M" : "L"}${xAt(i).toFixed(2)},${yLat(p.p95_ms).toFixed(2)}`)
    .join(" ");

  const ticksY = 4;
  const ticksX = 5;
  const formatTs = (ts: number) => {
    const d = new Date(ts * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };

  return (
    <div ref={ref} className="relative w-full">
      <svg width={size.w} height={size.h}>
        {Array.from({ length: ticksY + 1 }).map((_, i) => {
          const y = PAD.t + (plotH * i) / ticksY;
          return (
            <line key={i} x1={PAD.l} y1={y} x2={PAD.l + plotW} y2={y} stroke="#e2e8f0" strokeDasharray="3 3" />
          );
        })}
        {Array.from({ length: ticksY + 1 }).map((_, i) => {
          const y = PAD.t + (plotH * i) / ticksY;
          const v = (maxReq * (ticksY - i)) / ticksY;
          return (
            <text key={`yl${i}`} x={PAD.l - 6} y={y + 4} textAnchor="end" fontSize={10} fill="#64748b">
              {fmtInt(v)}
            </text>
          );
        })}
        {Array.from({ length: ticksY + 1 }).map((_, i) => {
          const y = PAD.t + (plotH * i) / ticksY;
          const v = (maxLat * (ticksY - i)) / ticksY;
          return (
            <text key={`yr${i}`} x={PAD.l + plotW + 6} y={y + 4} textAnchor="start" fontSize={10} fill="#0ea5e9">
              {fmtInt(v)}
            </text>
          );
        })}
        {Array.from({ length: ticksX + 1 }).map((_, i) => {
          const idx = Math.min(n - 1, Math.floor(((n - 1) * i) / ticksX));
          const x = xAt(idx);
          const t = points[idx];
          return (
            <g key={`x${i}`}>
              <line x1={x} y1={PAD.t + plotH} x2={x} y2={PAD.t + plotH + 4} stroke="#cbd5e1" />
              <text x={x} y={PAD.t + plotH + 18} textAnchor="middle" fontSize={10} fill="#64748b">
                {t ? formatTs(t.ts) : ""}
              </text>
            </g>
          );
        })}
        <path d={barsPath} fill="#6366f1" opacity={0.55} />
        {n >= 2 ? <path d={linePath} stroke="#0ea5e9" strokeWidth={2} fill="none" /> : null}
        {/* Y 轴标签 */}
        <text x={8} y={PAD.t + plotH / 2} textAnchor="middle" fontSize={10} fill="#6366f1"
              transform={`rotate(-90, 8, ${PAD.t + plotH / 2})`}>
          请求数
        </text>
        <text x={size.w - 8} y={PAD.t + plotH / 2} textAnchor="middle" fontSize={10} fill="#0ea5e9"
              transform={`rotate(90, ${size.w - 8}, ${PAD.t + plotH / 2})`}>
          P95 (ms)
        </text>
        {points.map((_, i) => {
          const w = n <= 1 ? plotW : plotW / n;
          const x = PAD.l + i * w;
          return (
            <rect
              key={`hit${i}`}
              x={x}
              y={PAD.t}
              width={w}
              height={plotH}
              fill="transparent"
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover((v) => (v === i ? null : v))}
            />
          );
        })}
        {hover !== null && points[hover] ? (
          <line
            x1={xAt(hover)}
            y1={PAD.t}
            x2={xAt(hover)}
            y2={PAD.t + plotH}
            stroke="#94a3b8"
            strokeDasharray="3 3"
          />
        ) : null}
      </svg>
      {hover !== null && points[hover] ? (
        <div
          className="pointer-events-none absolute z-10 w-52 rounded-md border border-slate-200 bg-white/95 px-3 py-2 text-xs shadow-md"
          style={{
            left: Math.min(size.w - 210, Math.max(0, xAt(hover) + 6)),
            top: 8,
          }}
        >
          <div className="font-semibold text-slate-800">
            {new Date(points[hover].ts * 1000).toLocaleString()}
          </div>
          <div className="mt-1 grid grid-cols-2 gap-x-3 gap-y-0.5 tabular-nums text-slate-600">
            <div>请求数</div>
            <div className="text-right">{fmtInt(points[hover].requests)}</div>
            <div>成功</div>
            <div className="text-right">{fmtInt(points[hover].success)}</div>
            <div>不可用</div>
            <div className="text-right text-rose-600">{fmtInt(points[hover].fail_unavailable)}</div>
            <div>P50</div>
            <div className="text-right">{fmtMs(points[hover].p50_ms)}</div>
            <div>P95</div>
            <div className="text-right text-sky-700">{fmtMs(points[hover].p95_ms)}</div>
            <div>P99</div>
            <div className="text-right">{fmtMs(points[hover].p99_ms)}</div>
            <div>IN tok</div>
            <div className="text-right">{fmtInt(points[hover].input_tokens)}</div>
            <div>OUT tok</div>
            <div className="text-right">{fmtInt(points[hover].output_tokens)}</div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** TPM 时序图 */
function TpmChart({ points }: { points: MetricsSeriesPoint[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 220 });
  const PAD = { l: 52, r: 16, t: 20, b: 28 };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver((es) => {
      const r = es[0].contentRect;
      setSize({ w: Math.max(320, Math.floor(r.width)), h: 220 });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const plotW = size.w - PAD.l - PAD.r;
  const plotH = size.h - PAD.t - PAD.b;
  const n = points.length;
  // 近似:桶长未知,用 max(input+output) 做 scale 即可(展示趋势)
  const maxTot = Math.max(1, ...points.map((p) => p.input_tokens + p.output_tokens));
  const xAt = (i: number) => PAD.l + (n <= 1 ? plotW / 2 : (plotW * i) / (n - 1));
  const yV = (v: number) => PAD.t + plotH - (plotH * v) / maxTot;
  const totalLine = points
    .map((p, i) => `${i === 0 ? "M" : "L"}${xAt(i).toFixed(2)},${yV(p.input_tokens + p.output_tokens).toFixed(2)}`)
    .join(" ");
  const inputLine = points
    .map((p, i) => `${i === 0 ? "M" : "L"}${xAt(i).toFixed(2)},${yV(p.input_tokens).toFixed(2)}`)
    .join(" ");

  const formatTs = (ts: number) => {
    const d = new Date(ts * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };
  const ticksX = 5;
  const ticksY = 4;

  return (
    <div ref={ref} className="w-full">
      <svg width={size.w} height={size.h}>
        {Array.from({ length: ticksY + 1 }).map((_, i) => {
          const y = PAD.t + (plotH * i) / ticksY;
          return (
            <line key={i} x1={PAD.l} y1={y} x2={PAD.l + plotW} y2={y} stroke="#e2e8f0" strokeDasharray="3 3" />
          );
        })}
        {Array.from({ length: ticksY + 1 }).map((_, i) => {
          const y = PAD.t + (plotH * i) / ticksY;
          const v = (maxTot * (ticksY - i)) / ticksY;
          return (
            <text key={`yl${i}`} x={PAD.l - 6} y={y + 4} textAnchor="end" fontSize={10} fill="#64748b">
              {fmtBuckets(v, "")}
            </text>
          );
        })}
        {Array.from({ length: ticksX + 1 }).map((_, i) => {
          const idx = Math.min(n - 1, Math.floor(((n - 1) * i) / ticksX));
          const x = xAt(idx);
          const t = points[idx];
          return (
            <text key={`x${i}`} x={x} y={PAD.t + plotH + 18} textAnchor="middle" fontSize={10} fill="#64748b">
              {t ? formatTs(t.ts) : ""}
            </text>
          );
        })}
        {/* 总面积:输入+输出 */}
        <path
          d={`${totalLine} L${xAt(n - 1)},${PAD.t + plotH} L${xAt(0)},${PAD.t + plotH} Z`}
          fill="#10b981"
          opacity={0.18}
        />
        <path d={totalLine} stroke="#10b981" strokeWidth={2} fill="none" />
        <path
          d={`${inputLine} L${xAt(n - 1)},${PAD.t + plotH} L${xAt(0)},${PAD.t + plotH} Z`}
          fill="#6366f1"
          opacity={0.14}
        />
        <path d={inputLine} stroke="#6366f1" strokeWidth={2} fill="none" opacity={0.9} />
        {/* 图例 */}
        <g>
          <rect x={PAD.l + plotW - 180} y={PAD.t - 14} width={10} height={10} fill="#10b981" opacity={0.35} />
          <text x={PAD.l + plotW - 166} y={PAD.t - 5} fontSize={10} fill="#0f766e">
            总 tokens (IN+OUT)
          </text>
          <rect x={PAD.l + plotW - 180} y={PAD.t + 2} width={10} height={10} fill="#6366f1" opacity={0.35} />
          <text x={PAD.l + plotW - 166} y={PAD.t + 11} fontSize={10} fill="#4338ca">
            仅 IN tokens
          </text>
        </g>
      </svg>
    </div>
  );
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

function RequestLogPanel() {
  const [rows, setRows] = useState<RequestLogRow[]>([]);
  const [q, setQ] = useState("");
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const qRef = useRef("");

  const load = async () => {
    setLoading(true);
    try {
      const r = await api.requestLogs(qRef.current || undefined, page, pageSize);
      setRows(r.data); setTotal(r.total);
    } finally { setLoading(false); }
  };

  useEffect(() => { load(); }, [page, pageSize]);

  const onSearch = (v: string) => {
    setQ(v);
    qRef.current = v;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      if (page !== 1) setPage(1);
      else load();
    }, 500);
  };

  const fmtTime = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString("zh-CN", { hour12: false }) + "." + String(d.getMilliseconds()).padStart(3, "0");
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
              <Button size="sm" variant="outline" className="h-8" onClick={() => load()}>
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? (
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
                          <Badge variant={r.path === "chat" ? "default" : "muted"} className="text-[10px]">
                            {r.path}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs">
                          {r.final_upstream_model ? (
                            <span className="inline-flex items-center gap-1">
                              <span className="text-muted-foreground">{r.final_kind}</span>
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
                                        <td className="py-1">
                                          <span className="text-muted-foreground">{c.kind}</span>
                                          <span className="ml-1 text-muted-foreground/60">{c.base_url.length > 30 ? c.base_url.slice(0, 30) + "…" : c.base_url}</span>
                                        </td>
                                        <td className="py-1">{c.upstream_model}</td>
                                        <td className="py-1 mono">{c.weight}</td>
                                        <td className="py-1">
                                          {(() => {
                                            // 在 attempts 中查找对应候选的状态
                                            const att = r.attempts.find((a) => a.kind === c.kind && a.upstream_model === c.upstream_model);
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
                              {/* 实际尝试链 */}
                              {r.attempts.length > 0 && (
                                <div>
                                  <p className="mb-1 text-xs font-medium text-muted-foreground">实际尝试（failover 链）</p>
                                  <table className="w-full text-xs">
                                    <thead>
                                      <tr className="border-b text-left text-muted-foreground">
                                        <th className="pb-1 font-normal">上游</th>
                                        <th className="pb-1 font-normal">模型</th>
                                        <th className="pb-1 font-normal">状态</th>
                                        <th className="pb-1 font-normal">延迟</th>
                                        <th className="pb-1 font-normal">错误</th>
                                      </tr>
                                    </thead>
                                    <tbody>
                                      {r.attempts.map((a, i) => (
                                        <tr key={i} className="border-b border-dashed last:border-0">
                                          <td className="py-1"><span className="text-muted-foreground">{a.kind}</span></td>
                                          <td className="py-1">{a.upstream_model}</td>
                                          <td className="py-1">
                                            <span className={cn("inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-semibold ring-1 ring-inset", attemptStatusColor(a.status))}>
                                              {attemptStatusLabel(a.status)}
                                            </span>
                                          </td>
                                          <td className="py-1 mono">{a.latency_ms > 0 ? `${a.latency_ms}ms` : "—"}</td>
                                          <td className="py-1 max-w-[300px] truncate text-muted-foreground">{a.error || "—"}</td>
                                        </tr>
                                      ))}
                                    </tbody>
                                  </table>
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

