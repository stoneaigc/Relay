import { useEffect, useRef, useState, ReactNode } from "react";
import { BrowserRouter, NavLink, useLocation } from "react-router-dom";
import {
  Wallet, KeyRound, Copy, RefreshCw, Plus, LogOut, Check,
  LayoutDashboard, Receipt, MessageSquare, Menu, X, SendHorizontal, Boxes,
  Gift, Star, Code2, Clock, CheckCircle2, XCircle, ExternalLink, ImagePlus, Lightbulb, BookOpen,
} from "lucide-react";
import { api, getToken, setToken, clearToken, KeyInfo, chatStream, ChatMsg, RewardInfo, RewardClaim, RewardTask, EvidenceType } from "./api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { PasswordInput } from "@/components/ui/password-input";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import PaginationBar from "./PaginationBar";
import DocsView from "./ApiDocs";

const GRANT = 10_000_000;

const fmtDay = (ts: number) => { const d = new Date(ts * 1000); return `${d.getMonth() + 1}/${d.getDate()}`; };

/** 复制到剪贴板,并给出 ~1.4s 的“已复制”反馈状态。 */
function useCopy(): [boolean, (t: string) => void] {
  const [copied, setCopied] = useState(false);
  const copy = (t: string) => {
    navigator.clipboard?.writeText(t);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
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

/** 可用模型名的可复制小标签:点击即复制,即时显示“已复制”。 */
function ModelChip({ name }: { name: string }) {
  const [copied, copy] = useCopy();
  return (
    <button type="button" onClick={() => copy(name)}
      className={cn(
        "mono inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs transition-colors",
        copied
          ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-600"
          : "border-border bg-muted text-foreground hover:bg-accent"
      )}>
      {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3 opacity-40" />}
      {name}
      {copied && <span>已复制</span>}
    </button>
  );
}

function LineChart({ points }: { points: { ts: number; tokens: number }[] }) {
  const W = 600, H = 150, pad = 10;
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
    <BrowserRouter basename="/portal">
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

type Section = "overview" | "chat" | "rewards" | "usage" | "docs";
const NAV: { path: string; label: string; icon: any }[] = [
  { path: "/", label: "概览", icon: LayoutDashboard },
  { path: "/chat", label: "对话", icon: MessageSquare },
  { path: "/rewards", label: "奖励", icon: Gift },
  { path: "/usage", label: "用量明细", icon: Receipt },
  { path: "/docs", label: "API 文档", icon: BookOpen },
];

function Dashboard({ onLogout }: { onLogout: () => void }) {
  const loc = useLocation();
  const section: Section = loc.pathname.startsWith("/chat") ? "chat"
    : loc.pathname.startsWith("/rewards") ? "rewards"
    : loc.pathname.startsWith("/usage") ? "usage"
    : loc.pathname.startsWith("/docs") ? "docs" : "overview";
  const title = section === "chat" ? "对话" : section === "rewards" ? "奖励"
    : section === "usage" ? "用量明细" : section === "docs" ? "API 文档" : "概览";
  const [mobileOpen, setMobileOpen] = useState(false);
  const [summary, setSummary] = useState<{ granted: number; used: number; balance: number } | null>(null);
  const [phone, setPhone] = useState("");
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [usage, setUsage] = useState<any[]>([]);
  const [usageTotal, setUsageTotal] = useState(0);
  const [usagePage, setUsagePage] = useState(1);
  const [usagePageSize, setUsagePageSize] = useState(20);
  const [models, setModels] = useState<string[]>([]);
  const [series, setSeries] = useState<{ ts: number; tokens: number; calls: number }[]>([]);
  const [seriesDays, setSeriesDays] = useState(30);
  const [reveal, setReveal] = useState<{ kind: string; key: string } | null>(null);

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
        {NAV.map((n) => (
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

        {section === "chat" ? (
          <ChatView models={models} onSent={refresh} />
        ) : section === "docs" ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              <DocsView />
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-auto">
            <div className="p-4 md:p-6">
              {section === "overview" ? (
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
                      <CardTitle className="flex items-center gap-2 text-base">
                        <Boxes className="h-4 w-4" />可用模型{models.length > 0 && <span className="text-muted-foreground">· {models.length}</span>}
                        {models.length > 0 && <span className="ml-auto text-xs font-normal text-muted-foreground">点击复制模型名</span>}
                      </CardTitle>
                    </CardHeader>
                    <CardContent>
                      {models.length === 0 ? (
                        <p className="text-sm text-muted-foreground">暂无可用模型,请联系管理员为你分配模型组。</p>
                      ) : (
                        <div className="flex flex-wrap gap-2">
                          {models.map((m) => <ModelChip key={m} name={m} />)}
                        </div>
                      )}
                    </CardContent>
                  </Card>
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
                            <span>峰值 {Math.max(0, ...series.map((d) => d.tokens)).toLocaleString()} / 天</span>
                            <span>{fmtDay(series[series.length - 1].ts)}</span>
                          </div>
                        </>
                      )}
                    </CardContent>
                  </Card>
                </div>
              ) : section === "rewards" ? (
                <RewardsView />
              ) : (
                <Card>
                  <CardHeader><CardTitle className="text-base">用量明细({usageTotal})</CardTitle></CardHeader>
                  <CardContent>
                    {usage.length === 0 ? <p className="text-sm text-muted-foreground">暂无调用记录</p> : (
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>模型</TableHead><TableHead>供应商</TableHead>
                            <TableHead>输入</TableHead><TableHead>输出</TableHead><TableHead>计费</TableHead>
                            <TableHead>缓存</TableHead><TableHead>状态</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {usage.map((r, i) => (
                            <TableRow key={i}>
                              <TableCell className="mono">{r.model}</TableCell><TableCell>{r.provider}</TableCell>
                              <TableCell>{r.input_tokens}</TableCell><TableCell>{r.output_tokens}</TableCell>
                              <TableCell className="mono">{r.charged_tokens}</TableCell>
                              <TableCell>
                                {r.kind === "cache"
                                  ? <Badge variant="success" title="精确命中缓存,按折扣率计费">精确</Badge>
                                  : r.kind === "semantic"
                                    ? <Badge variant="default" title="语义相似命中缓存,按折扣率计费">语义</Badge>
                                    : <span className="text-xs text-muted-foreground">—</span>}
                              </TableCell>
                              <TableCell>{r.status}</TableCell>
                            </TableRow>
                          ))}
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
          <div className="mono rounded-lg border bg-muted/50 px-3 py-2 text-xs break-all">{active ? active.key_prefix : "—"}</div>
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
  const [copied, setCopied] = useState(false);
  const copy = () => { navigator.clipboard.writeText(apiKey); setCopied(true); };
  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>你的新 {kind === "openai" ? "OpenAI" : "Anthropic"} Key</DialogTitle>
        <DialogDescription className="text-destructive">⚠️ 仅此一次显示,关闭后无法再次查看,请立即复制保存。</DialogDescription>
      </DialogHeader>
      <div className="mono rounded-lg border bg-muted/50 px-3 py-3 text-sm break-all">{apiKey}</div>
      <Button onClick={copy} variant={copied ? "secondary" : "default"}>
        {copied ? <><Check className="h-4 w-4" />已复制</> : <><Copy className="h-4 w-4" />复制</>}
      </Button>
    </DialogContent>
  );
}
