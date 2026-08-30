const TOKEN_KEY = "relay_admin_token";

export const getToken = () => localStorage.getItem(TOKEN_KEY);
export const setToken = (t: string) => localStorage.setItem(TOKEN_KEY, t);
export const clearToken = () => localStorage.removeItem(TOKEN_KEY);

async function req(path: string, opts: RequestInit = {}) {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((opts.headers as Record<string, string>) || {}),
  };
  const t = getToken();
  if (t) headers["Authorization"] = `Bearer ${t}`;
  const res = await fetch(path, { ...opts, headers });
  const text = await res.text();
  const data = text ? JSON.parse(text) : {};
  if (!res.ok) throw new Error(data?.error?.message || res.statusText);
  return data;
}

export interface UserRow {
  id: string;
  username: string | null;
  email: string | null;
  phone: string | null;
  status: number;
  balance: number;
  used: number;
  today: number;
  granted: number;
  concurrency_limit: number | null;
  group_id: number;
  source: string | null;
  created_at: string;
}

export interface ModelRow {
  id: number;
  label: string | null;
  kind: string | null;
  base_url: string | null;
  upstream_model: string;
  provider?: string | null;
  api_key?: string | null;
}

export interface ProviderRow {
  name: string;
  kind: string;
  base_url: string;
  model_count: number;
}

export interface GroupRow { id: number; name: string; is_active: boolean; strategy: string }

export interface TimeRuleRow {
  id: number;
  group_id: number;
  name: string;
  weekdays: string;
  start_time: string;
  end_time: string;
  multiplier: number;
  weight_map: string | null;
  active: boolean;
}

export interface TimeRulePayload {
  name: string;
  weekdays?: string;
  start_time: string;
  end_time: string;
  multiplier?: number;
  weight_map?: string | null;
  active?: boolean;
}

export interface RouteRow {
  id: number;
  public_name: string;
  model_id: number;
  weight: number;
  multiplier: number;
  provider: string;
  upstream_model: string;
  label: string | null;
}

export const api = {
  login: (username: string, password: string) =>
    req("/admin/api/auth/login", { method: "POST", body: JSON.stringify({ username, password }) }),

  overview: () => req("/admin/api/overview"),
  overviewSeries: (granularity: "day" | "week" | "month"): Promise<{ granularity: string; data: { ts: number; tokens: number; calls: number }[] }> =>
    req(`/admin/api/overview/series?granularity=${granularity}`),

  users: (page?: number, pageSize?: number): Promise<{ data: UserRow[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/users${s ? "?" + s : ""}`);
  },
  createUser: (body: { username: string; password: string; email?: string; phone?: string; group_id?: number; grant_tokens?: number; concurrency_limit?: number }) =>
    req("/admin/api/users", { method: "POST", body: JSON.stringify(body) }),
  patchUser: (id: string, body: any) =>
    req(`/admin/api/users/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteUser: (id: string) => req(`/admin/api/users/${id}`, { method: "DELETE" }),
  userSeries: (id: string): Promise<{ data: { ts: number; tokens: number; calls: number }[] }> =>
    req(`/admin/api/users/${id}/series`),
  userUsage: (id: string, page?: number, pageSize?: number): Promise<{ data: any[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/users/${id}/usage${s ? "?" + s : ""}`);
  },

  // ---- 上游供应商(Provider) ----
  providers: (page?: number, page_size?: number): Promise<{ data: ProviderRow[]; page: number; page_size: number; total: number; total_pages: number }> =>
    req(`/admin/api/providers?page=${page || 1}&page_size=${page_size || 20}`),
  providerModels: (name: string): Promise<{ data: { id: number; upstream_model: string; label: string | null }[] }> => req(`/admin/api/providers/${name}/models`),
  deleteProvider: (name: string) => req(`/admin/api/providers/${name}`, { method: "DELETE" }),
  updateProvider: (name: string, body: { base_url: string; api_key?: string }) =>
    req(`/admin/api/providers/${name}`, { method: "PUT", body: JSON.stringify(body) }),

  models: (): Promise<{ data: ModelRow[] }> => req("/admin/api/models"),
  addModel: (body: { label?: string; kind: string; base_url: string; api_key?: string; upstream_model: string }) =>
    req("/admin/api/models", { method: "POST", body: JSON.stringify(body) }),
  updateModel: (id: number, body: { kind: string; base_url: string; api_key?: string; upstream_model: string; label?: string }) =>
    req(`/admin/api/models/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteModel: (id: number) => req(`/admin/api/models/${id}`, { method: "DELETE" }),
  testModel: (id: number): Promise<{ ok: boolean; latency_ms?: number; error?: string }> =>
    req(`/admin/api/models/${id}/test`, { method: "POST" }),
  fetchModelList: (body: { kind: string; base_url: string; api_key?: string }): Promise<{ ok: boolean; models?: string[]; error?: string }> =>
    req("/admin/api/models/fetch-list", { method: "POST", body: JSON.stringify(body) }),
  addModelsBatch: (body: { kind: string; base_url: string; api_key?: string; models: { upstream_model: string; label?: string }[] }): Promise<{ ok: boolean; provider?: string; provider_created?: boolean; added?: number; skipped?: number }> =>
    req("/admin/api/models/batch", { method: "POST", body: JSON.stringify(body) }),
  providerExists: (base_url: string, api_key?: string): Promise<{ exists: boolean; name?: string; models?: string[] }> =>
    req("/admin/api/providers/exists", { method: "POST", body: JSON.stringify({ base_url, api_key }) }),

  groups: (): Promise<{ data: GroupRow[] }> => req("/admin/api/groups"),
  addGroup: (name: string) => req("/admin/api/groups", { method: "POST", body: JSON.stringify({ name }) }),
  deleteGroup: (id: number) => req(`/admin/api/groups/${id}`, { method: "DELETE" }),
  activateGroup: (id: number) => req(`/admin/api/groups/${id}/activate`, { method: "POST" }),
  setGroupStrategy: (id: number, strategy: string) =>
    req(`/admin/api/groups/${id}/strategy`, { method: "POST", body: JSON.stringify({ strategy }) }),

  timeRules: (groupId: number): Promise<{ data: TimeRuleRow[] }> => req(`/admin/api/groups/${groupId}/time-rules`),
  addTimeRule: (groupId: number, body: TimeRulePayload) =>
    req(`/admin/api/groups/${groupId}/time-rules`, { method: "POST", body: JSON.stringify(body) }),
  updateTimeRule: (groupId: number, ruleId: number, body: TimeRulePayload) =>
    req(`/admin/api/groups/${groupId}/time-rules/${ruleId}`, { method: "PUT", body: JSON.stringify(body) }),
  deleteTimeRule: (groupId: number, ruleId: number) => req(`/admin/api/groups/${groupId}/time-rules/${ruleId}`, { method: "DELETE" }),

  routes: (groupId: number): Promise<{ data: RouteRow[] }> => req(`/admin/api/groups/${groupId}/routes`),
  addRoute: (groupId: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number }) =>
    req(`/admin/api/groups/${groupId}/routes`, { method: "POST", body: JSON.stringify(body) }),
  addRoutesBatch: (groupId: number, body: { routes: { public_name: string; model_id: number; weight?: number; multiplier?: number }[] }): Promise<{ ok: boolean; ids?: number[] }> =>
    req(`/admin/api/groups/${groupId}/routes/batch`, { method: "POST", body: JSON.stringify(body) }),
  updateRoute: (id: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number }) =>
    req(`/admin/api/routes/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteRoute: (id: number) => req(`/admin/api/routes/${id}`, { method: "DELETE" }),

  usage: (page?: number, pageSize?: number): Promise<{ data: any[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/usage${s ? "?" + s : ""}`);
  },

  rewards: (status?: "pending" | "approved" | "rejected", page?: number, pageSize?: number): Promise<RewardListResp> => {
    const qs = new URLSearchParams();
    if (status) qs.set("status", status);
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const q = qs.toString();
    return req(`/admin/api/rewards${q ? "?" + q : ""}`);
  },
  reviewReward: (id: string, approve: boolean, note?: string, reward_tokens?: number) =>
    req(`/admin/api/rewards/${id}/review`, { method: "POST", body: JSON.stringify({ approve, note, reward_tokens }) }),

  rewardTasks: (): Promise<{ data: RewardTaskRow[] }> => req("/admin/api/reward-tasks"),
  createRewardTask: (body: RewardTaskBody) =>
    req("/admin/api/reward-tasks", { method: "POST", body: JSON.stringify(body) }),
  updateRewardTask: (id: number, body: RewardTaskBody) =>
    req(`/admin/api/reward-tasks/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteRewardTask: (id: number) => req(`/admin/api/reward-tasks/${id}`, { method: "DELETE" }),

  emailSettings: (): Promise<EmailSettingsResp> => req("/admin/api/settings/email"),
  saveEmailSettings: (body: EmailSettingsBody) =>
    req("/admin/api/settings/email", { method: "POST", body: JSON.stringify(body) }),
  testEmail: (body: EmailSettingsBody & { test_to: string }): Promise<{ ok: boolean; error?: string }> =>
    req("/admin/api/settings/email/test", { method: "POST", body: JSON.stringify(body) }),

  // ---- 上游治理(熔断器 + 并发槽仪表盘) ----
  listUpstreams: (): Promise<UpstreamsResp> => req("/admin/api/upstreams"),
  resetAllBreakers: (): Promise<{ ok: true; cleared: number }> => req("/admin/api/upstreams/reset", { method: "POST" }),
  resetOneBreaker: (id: string): Promise<{ ok: true; matched: boolean }> => req(`/admin/api/upstreams/reset/${encodeURIComponent(id)}`, { method: "POST" }),

  // ---- 路由失败审计 ----
  listFailures: (limit?: number): Promise<AuditFailuresResp> =>
    req(`/admin/api/audit/failures${typeof limit === "number" ? `?limit=${limit}` : ""}`),

  // ---- 接口指标仪表盘 ----
  metricsDashboard: (range_secs?: number, top_n?: number): Promise<MetricsDashboardResp> => {
    const qs = new URLSearchParams();
    if (typeof range_secs === "number") qs.set("range_secs", String(range_secs));
    if (typeof top_n === "number") qs.set("top_n", String(top_n));
    const q = qs.toString();
    return req(`/admin/api/metrics${q ? "?" + q : ""}`);
  },
  metricsOverview: (range_secs?: number, top_n?: number): Promise<MetricsDashboardResp> => {
    const qs = new URLSearchParams();
    if (typeof range_secs === "number") qs.set("range_secs", String(range_secs));
    if (typeof top_n === "number") qs.set("top_n", String(top_n));
    const q = qs.toString();
    return req(`/admin/api/metrics/overview${q ? "?" + q : ""}`);
  },

  // ---- 请求链路追踪 ----
  requestLogs: (q?: string, page?: number, pageSize?: number): Promise<{ data: RequestLogRow[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (q) qs.set("q", q);
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/request-logs${s ? "?" + s : ""}`);
  },
};

export type EvidenceType = "screenshot" | "link" | "text" | "none";

export interface RewardTaskRow {
  id: number;
  title: string;
  description: string | null;
  evidence_type: EvidenceType;
  variable: boolean;
  reward_tokens: number;
  reward_min: number;
  reward_max: number;
  link_url: string | null;
  enabled: boolean;
  sort: number;
}

export interface RewardTaskBody {
  title: string;
  description?: string;
  evidence_type: EvidenceType;
  variable: boolean;
  reward_tokens: number;
  reward_min: number;
  reward_max: number;
  link_url?: string;
  enabled: boolean;
  sort: number;
}

export interface RewardClaimRow {
  id: string;
  user_id: string;
  name: string | null;
  task_id: number | null;
  title: string | null;
  evidence_type: EvidenceType | null;
  variable: boolean;
  reward_min: number;
  reward_max: number;
  evidence: string | null;
  reward_tokens: number;
  status: number; // 0=pending 1=approved 2=rejected
  review_note: string | null;
  created_at: string;
  reviewed_at: string | null;
}

export interface RewardListResp {
  data: RewardClaimRow[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface EmailSettingsBody {
  smtp_host: string;
  smtp_port: number;
  username: string;
  password?: string; // 留空=保持不变
  from: string;
}

export interface EmailSettingsResp extends EmailSettingsBody {
  has_password: boolean;
  enabled: boolean;
}

// ============================================================
// 上游治理仪表盘
// ============================================================

export interface BreakerStatus {
  fail_count: number;
  threshold: number;
  is_broken: boolean;
  /** 还剩多少毫秒进入自动恢复,0 = 不在熔断 */
  recover_remaining_ms: number;
  window_secs: number;
}

export interface ConcurrencyStatus {
  limit: number;
  in_flight: number;
  utilization_pct: number;
}

export interface UpstreamRow {
  kind: "openai" | "anthropic" | string;
  base_url: string;
  has_key: boolean;
  /** 16 hex 字符串, "" = 无 key */
  key_fingerprint: string;
  breaker: BreakerStatus;
  concurrency: ConcurrencyStatus;
}

export interface UpstreamsResp {
  total: number;
  broken_count: number;
  items: UpstreamRow[];
}

export interface FailureRow {
  ts_ms: number;
  kind: "openai" | "anthropic" | string;
  base_url: string;
  key_fingerprint: string;
  /** `kind|base_url|key_fingerprint`,与「清零单个」的 ID 格式一致 */
  upstream_id: string;
  requested_model: string;
  path: "chat" | "messages" | string;
  error_summary: string;
  fail_count_after: number;
  threshold: number;
  /** 这一条是否刚好触发了熔断(前端红色高亮) */
  triggered_break: boolean;
}

export interface AuditFailuresResp {
  /** 实际存储中的总条数(ring buffer 容量 ≤ capacity) */
  stored: number;
  /** 环形 buffer 总容量 */
  capacity: number;
  items: FailureRow[];
}

// ================== 接口指标仪表盘 ==================

/** 时间桶(秒或分钟)聚合结果 */
export interface MetricsSeriesPoint {
  /** 桶起点 unix 秒 */
  ts: number;
  /** 桶内总请求数 */
  requests: number;
  /** 桶内成功数 */
  success: number;
  /** 桶内不可用失败数 */
  fail_unavailable: number;
  /** 桶内 4xx 业务失败数 */
  fail_other: number;
  /** 桶内输入 tokens */
  input_tokens: number;
  /** 桶内输出 tokens */
  output_tokens: number;
  /** 延迟分位(毫秒):只有当 samples 非空时才有值 */
  avg_ms: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
}

/** KPI 汇总 */
export interface MetricsSummary {
  /** 采样窗口长度(秒) */
  range_secs: number;
  /** 采样时间 */
  sampled_at: number;
  requests: number;
  success_rate: number;
  /** 请求速率(RPS) */
  rps: number;
  /** 吞吐量(TPM = (input+output) / range_secs * 60) */
  tpm: number;
  input_tokens: number;
  output_tokens: number;
  fail_unavailable: number;
  fail_other: number;
  avg_ms: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
}

/** 单个上游排行条目 */
export interface MetricsUpstreamRow {
  /** 上游 key 展示 */
  kind: string;
  base_url: string;
  key_fingerprint: string;
  upstream_id: string;
  /** 核心指标 */
  requests: number;
  success_rate: number;
  tpm: number;
  avg_ms: number;
  p99_ms: number;
  fail_unavailable: number;
}

export interface MetricsDashboardResp {
  range_secs: number;
  sampled_at: number;
  summary: MetricsSummary;
  /** 全局时序(前端柱状图/折线图用) */
  series: MetricsSeriesPoint[];
  /** Top N 上游排行榜 */
  upstreams: MetricsUpstreamRow[];
  /** 每个 Top 上游对应的时序(可选,前端画多线对比) */
  upstream_series?: {
    upstream_id: string;
    series: MetricsSeriesPoint[];
  }[];
}

// ================== 请求链路追踪 ==================

export interface RequestAttempt {
  kind: string;
  base_url: string;
  upstream_model: string;
  weight: number;
  /** 200=成功; 503=不可用; -1=熔断跳过; 400/500=其它错误; 0=初始状态(候选列表) */
  status: number;
  latency_ms: number;
  error: string;
}

export interface RequestLogRow {
  request_id: string;
  user_id: string;
  path: string;
  requested_model: string;
  stream: boolean;
  candidates: RequestAttempt[];
  attempts: RequestAttempt[];
  final_kind: string | null;
  final_upstream_model: string | null;
  final_status: number;
  latency_ms: number;
  input_tokens: number;
  output_tokens: number;
  charged_tokens: number;
  /** unix 秒 */
  ts: number;
}
