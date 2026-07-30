import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route, NavLink, Navigate, useLocation } from "react-router-dom";
import { Users as UsersIcon, Boxes, Layers, BarChart3, LayoutDashboard, LogOut, Plus, Power, Menu, X, Trash2, Pencil, TrendingUp, Activity, Star, Gift, Check, ExternalLink, Settings, Send } from "lucide-react";
import { api, getToken, setToken, clearToken, UserRow, ModelRow, GroupRow, RouteRow, RewardClaimRow, RewardTaskRow, RewardTaskBody, EvidenceType, EmailSettingsResp } from "./api";
import { Button } from "@/components/ui/button";
import { RowActions } from "@/components/ui/row-actions";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  if (!authed) return <Login onSuccess={() => setAuthed(true)} />;
  return (
    <BrowserRouter basename="/admin">
      <Console onLogout={() => { clearToken(); setAuthed(false); }} />
    </BrowserRouter>
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
        <CardHeader><CardTitle className="text-xl">Run<span className="text-primary">API</span> 管理后台</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <Input placeholder="用户名" value={u} onChange={(e) => setU(e.target.value)} />
          <Input placeholder="密码" type="password" value={p} onChange={(e) => setP(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
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
        <div className="text-lg font-bold">Run<span className="text-primary">API</span></div>
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
  admin: "管理员", phone: "手机号", wechat: "微信", alipay: "支付宝",
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
  const [userDlg, setUserDlg] = useState<{ edit: UserRow | null } | null>(null);
  const [chart, setChart] = useState<UserRow | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const load = async () => {
    try {
      const [u, g] = await Promise.all([api.users(), api.groups()]);
      setRows(u.data); setGroups(g.data);
    } catch (e: any) { if (String(e.message).includes("auth")) onAuthErr(); }
  };
  useEffect(() => { load(); }, []);

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
              <Input className="w-56" placeholder="搜索手机号 / ID" value={q} onChange={(e) => setQ(e.target.value)} />
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
        </CardContent>
      </Card>

      <UserDialog dlg={userDlg} groups={groups} onClose={() => setUserDlg(null)} onSaved={load} />
      <UserChartDialog user={chart} onClose={() => setChart(null)} />
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}


function ModelsPanel() {
  const [models, setModels] = useState<ModelRow[]>([]);
  const [dlg, setDlg] = useState<{ edit: ModelRow | null } | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [test, setTest] = useState<Record<number, "loading" | { ok: boolean; msg: string }>>({});
  const load = async () => { setModels((await api.models()).data); };
  useEffect(() => { load(); }, []);
  const del = (m: ModelRow) => setConfirm({
    title: `删除模型「${m.label || m.upstream_model}」?`,
    desc: "引用该模型的组内路由会一并删除,使用它的调用将失败。",
    action: async () => { await api.deleteModel(m.id); load(); },
  });
  const runTest = async (id: number) => {
    setTest((t) => ({ ...t, [id]: "loading" }));
    try {
      const r = await api.testModel(id);
      setTest((t) => ({ ...t, [id]: { ok: r.ok, msg: r.ok ? `${r.latency_ms}ms` : (r.error || "失败") } }));
    } catch (e: any) {
      setTest((t) => ({ ...t, [id]: { ok: false, msg: e.message } }));
    }
  };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="text-base">模型({models.length})</CardTitle>
            <Button size="sm" onClick={() => setDlg({ edit: null })}><Plus className="h-4 w-4" />添加模型</Button>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader><TableRow>
              <TableHead>备注</TableHead><TableHead>协议</TableHead><TableHead>上游模型</TableHead><TableHead>Base URL</TableHead><TableHead className="text-right">操作</TableHead>
            </TableRow></TableHeader>
            <TableBody>
              {models.map((m) => (
                <TableRow key={m.id}>
                  <TableCell>{m.label || <span className="text-muted-foreground">—</span>}</TableCell>
                  <TableCell><Badge variant="muted">{m.kind}</Badge></TableCell>
                  <TableCell className="mono">{m.upstream_model}</TableCell>
                  <TableCell className="mono text-xs">{m.base_url}</TableCell>
                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-2">
                      {(() => {
                        const r = test[m.id];
                        if (r === "loading") return <span className="text-xs text-muted-foreground">校验中…</span>;
                        if (r) return <span className={cn("max-w-[180px] truncate text-xs", r.ok ? "text-success" : "text-destructive")} title={r.msg}>{r.ok ? `✓ ${r.msg}` : `✗ ${r.msg}`}</span>;
                        return null;
                      })()}
                      <RowActions actions={[
                        { label: "校验", icon: <Activity className="h-4 w-4" />, onClick: () => runTest(m.id) },
                        { label: "编辑", icon: <Pencil className="h-4 w-4" />, onClick: () => setDlg({ edit: m }) },
                        { label: "删除", icon: <Trash2 className="h-4 w-4" />, variant: "destructive", onClick: () => del(m) },
                      ]} />
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      <AddModelDialog dlg={dlg} onClose={() => setDlg(null)} onSaved={load} />
      <ConfirmDialog confirm={confirm} onClose={() => setConfirm(null)} />
    </div>
  );
}

function AddModelDialog({ dlg, onClose, onSaved }: { dlg: { edit: ModelRow | null } | null; onClose: () => void; onSaved: () => void }) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const [label, setLabel] = useState("");
  const [kind, setKind] = useState("openai");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [upstream, setUpstream] = useState("");
  const [err, setErr] = useState("");
  useEffect(() => {
    if (open) {
      setLabel(edit?.label ?? ""); setKind(edit?.kind ?? "openai"); setBaseUrl(edit?.base_url ?? "");
      setApiKey(""); setUpstream(edit?.upstream_model ?? ""); setErr("");
    }
  }, [dlg]);

  const submit = async () => {
    setErr("");
    try {
      const body = { label: label.trim() || undefined, kind, base_url: baseUrl.trim(), api_key: apiKey.trim() || undefined, upstream_model: upstream.trim() };
      if (edit) await api.updateModel(edit.id, body); else await api.addModel(body);
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
  };
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{edit ? "编辑模型" : "添加三方模型"}</DialogTitle>
          <DialogDescription>填该模型的上游连接,密钥直接保存,重启不丢。{edit && "(密钥留空则保持不变)"}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">备注/显示名(可选)</label>
            <Input placeholder="如 智谱 GLM-4" value={label} onChange={(e) => setLabel(e.target.value)} autoFocus /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">协议</label>
            <select value={kind} onChange={(e) => setKind(e.target.value)} className={sel}>
              <option value="openai">openai 兼容</option><option value="anthropic">anthropic</option>
            </select></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">Base URL</label>
            <Input placeholder="如 https://api.deepseek.com/v1" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">API 密钥{edit ? "(留空保持不变)" : "(可选,留空则不加鉴权头)"}</label>
            <Input type="password" placeholder={edit ? "留空保持原密钥" : "sk-...(本地无鉴权服务可留空)"} value={apiKey} onChange={(e) => setApiKey(e.target.value)} /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">上游模型名(供应商真实模型名)</label>
            <Input placeholder="如 deepseek-chat" value={upstream} onChange={(e) => setUpstream(e.target.value)} /></div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!baseUrl.trim() || !upstream.trim()}><Plus className="h-4 w-4" />{edit ? "保存" : "添加"}</Button>
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
  const [gq, setGq] = useState("");
  const [groupOpen, setGroupOpen] = useState(false);
  const [routeDlg, setRouteDlg] = useState<{ edit: RouteRow | null } | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const loadGroups = async () => {
    const [g, m] = await Promise.all([api.groups(), api.models()]);
    setGroups(g.data); setModels(m.data);
    if (sel == null && g.data.length) setSel(g.data[0].id);
  };
  const loadRoutes = async (gid: number) => { setRoutes((await api.routes(gid)).data); };
  useEffect(() => { loadGroups(); }, []);
  useEffect(() => { if (sel != null) loadRoutes(sel); }, [sel]);

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
            {sel != null && <Button size="sm" onClick={() => setRouteDlg({ edit: null })}><Plus className="h-4 w-4" />添加路由</Button>}
          </div>
        </CardHeader>
        <CardContent className="min-h-0 flex-1 overflow-auto">
          {sel == null ? <p className="text-sm text-muted-foreground">先选择左侧一个模型组</p> : (
            <>
              <p className="mb-3 text-xs text-muted-foreground">用户请求「对外模型名」→ 路由到指定模型;响应里保留用户传的名字。同名多条按权重分流。</p>
              <Table>
                <TableHeader><TableRow>
                  <TableHead>对外模型名</TableHead><TableHead>→ 实际模型</TableHead><TableHead>权重</TableHead><TableHead>倍率</TableHead><TableHead className="text-right">操作</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {routes.map((r) => (
                    <TableRow key={r.id}>
                      <TableCell className="mono">{r.public_name}</TableCell>
                      <TableCell className="text-xs text-muted-foreground">{(r.label ? r.label + " · " : "") }<span className="mono">{r.provider}/{r.upstream_model}</span></TableCell>
                      <TableCell>{r.weight}</TableCell>
                      <TableCell><Badge variant={r.multiplier === 1 ? "muted" : "default"}>×{r.multiplier}</Badge></TableCell>
                      <TableCell className="text-right">
                        <RowActions actions={[
                          { label: "编辑", icon: <Pencil className="h-4 w-4" />, onClick: () => setRouteDlg({ edit: r }) },
                          { label: "删除", icon: <Trash2 className="h-4 w-4" />, variant: "destructive", onClick: () => delRoute(r.id) },
                        ]} />
                      </TableCell>
                    </TableRow>
                  ))}
                  {routes.length === 0 && <TableRow><TableCell colSpan={5} className="text-sm text-muted-foreground">该组暂无路由</TableCell></TableRow>}
                </TableBody>
              </Table>
            </>
          )}
        </CardContent>
      </Card>

      {sel != null && <AddRouteDialog dlg={routeDlg} groupId={sel} models={models} onClose={() => setRouteDlg(null)} onSaved={() => loadRoutes(sel)} />}
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

function AddRouteDialog({ dlg, groupId, models, onClose, onSaved }: {
  dlg: { edit: RouteRow | null } | null; groupId: number; models: ModelRow[]; onClose: () => void; onSaved: () => void;
}) {
  const open = !!dlg;
  const edit = dlg?.edit ?? null;
  const [publicName, setPublicName] = useState("");
  const [modelId, setModelId] = useState<number | "">("");
  const [weight, setWeight] = useState("100");
  const [mult, setMult] = useState("1");
  const [err, setErr] = useState("");
  useEffect(() => {
    if (open) {
      setPublicName(edit?.public_name ?? ""); setModelId(edit?.model_id ?? models[0]?.id ?? "");
      setWeight(String(edit?.weight ?? 100)); setMult(String(edit?.multiplier ?? 1)); setErr("");
    }
  }, [dlg]);

  const submit = async () => {
    setErr("");
    try {
      const body = { public_name: publicName.trim(), model_id: Number(modelId), weight: Number(weight) || 100, multiplier: Number(mult) || 1 };
      if (edit) await api.updateRoute(edit.id, body); else await api.addRoute(groupId, body);
      onSaved(); onClose();
    } catch (e: any) { setErr(e.message); }
  };
  const sel = "h-9 w-full rounded-lg border border-input bg-card px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{edit ? "编辑路由" : "添加路由"}</DialogTitle>
          <DialogDescription>对外模型名 → 实际模型;用户请求对外名,响应里保持这个名字。</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div><label className="mb-1 block text-xs text-muted-foreground">对外模型名(用户请求用)</label>
            <Input placeholder="如 deepseek-chat / claude-3-5-sonnet" value={publicName} onChange={(e) => setPublicName(e.target.value)} autoFocus /></div>
          <div><label className="mb-1 block text-xs text-muted-foreground">实际路由到的模型</label>
            <select value={modelId} onChange={(e) => setModelId(Number(e.target.value))} className={sel}>
              {models.map((m) => <option key={m.id} value={m.id}>{(m.label ? m.label + " · " : "") + m.upstream_model + " (" + (m.kind ?? "") + ")"}</option>)}
            </select>
            {models.length === 0 && <p className="mt-1 text-xs text-destructive">请先在「模型」里添加模型</p>}</div>
          <div className="grid grid-cols-2 gap-3">
            <div><label className="mb-1 block text-xs text-muted-foreground">权重</label><Input value={weight} onChange={(e) => setWeight(e.target.value)} /></div>
            <div><label className="mb-1 block text-xs text-muted-foreground">倍率</label><Input value={mult} onChange={(e) => setMult(e.target.value)} /></div>
          </div>
          {err && <p className="text-sm text-destructive">{err}</p>}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button onClick={submit} disabled={!publicName.trim() || modelId === ""}><Plus className="h-4 w-4" />{edit ? "保存" : "添加"}</Button>
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

  const load = async () => {
    setLoading(true);
    try {
      const r = await api.rewards(filter === "all" ? undefined : filter);
      setRows(r.data);
    } finally { setLoading(false); }
  };
  useEffect(() => { load(); }, [filter]);

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
            <CardTitle className="text-base">奖励申领({rows.length})</CardTitle>
            <div className="flex items-center gap-1 rounded-lg bg-muted p-0.5">
              {(Object.keys(FILTER_LABEL) as RewardFilter[]).map((f) => (
                <button key={f} onClick={() => setFilter(f)}
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
            <Input value={form.title} onChange={(e) => set("title", e.target.value)} placeholder="如:给 runify 点 Star" autoFocus />
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
  useEffect(() => { api.usage().then((r) => setRows(r.data)); }, []);
  return (
    <Card>
      <CardHeader><CardTitle className="text-base">全局用量(最近 200 条)</CardTitle></CardHeader>
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
