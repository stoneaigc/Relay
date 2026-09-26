const TOKEN_KEY = "relay_admin_token";

// 子路径反代兼容:部署在 https://host/relay/admin/ 这类前缀下时,从地址栏探测出 "/relay"。
// 根路径部署(路径以 /admin 开头)时为空串,fetch 行为与从前完全一致。
const SITE_PREFIX = (() => {
  const m = window.location.pathname.match(/^(.+?)\/admin(?=\/|$)/);
  return m?.[1] ?? "";
})();

export const getToken = () => localStorage.getItem(TOKEN_KEY);
export const setToken = (t: string) => localStorage.setItem(TOKEN_KEY, t);
export const clearToken = () => localStorage.removeItem(TOKEN_KEY);

/** 网关健康与版本(公开端点,无需鉴权;用于界面角落展示服务版本)。 */
export const healthz = (): Promise<{ status: string; version: string }> =>
  fetch(SITE_PREFIX + "/healthz").then((r) => (r.ok ? r.json() : { status: "?", version: "" })).catch(() => ({ status: "?", version: "" }));

async function req(path: string, opts: RequestInit = {}) {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((opts.headers as Record<string, string>) || {}),
  };
  const t = getToken();
  if (t) headers["Authorization"] = `Bearer ${t}`;
  const res = await fetch(SITE_PREFIX + path, { ...opts, headers });
  const text = await res.text();
  const data = text ? JSON.parse(text) : {};
  if (!res.ok) {
    // 凭证失效统一处理：清除本地 token 并广播，由 App 切回登录页（登录接口自身的 401 除外）。
    if (res.status === 401 && !path.includes("/auth/login")) {
      clearToken();
      window.dispatchEvent(new Event("relay:auth-expired"));
    }
    const err: any = new Error(data?.error?.message || res.statusText);
    err.status = res.status;
    throw err;
  }
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
  used_month: number;
  granted: number;
  concurrency_limit: number | null;
  group_id: number;
  budget_daily_tokens: number | null;
  budget_monthly_tokens: number | null;
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
  input_price: number | null;
  output_price: number | null;
  context_length: number | null;
  tags: string | null;
}

export interface ProviderRow {
  name: string;
  kind: string;
  base_url: string;
  display_name?: string | null;
  model_count: number;
}

/** 供应商展开列表里的模型行（含价格，$/1M tokens）。 */
export interface ProviderModelRow {
  id: number;
  upstream_model: string;
  label: string | null;
  input_price: number | null;
  output_price: number | null;
  context_length: number | null;
  tags: string | null;
}

export type ProviderHealthStatus = "ok" | "degraded" | "down" | "broken" | "idle";

export interface ProviderHealthItem {
  provider: string;
  kind: string;
  base_url: string;
  key_fingerprint: string;
  requests: number;
  success: number;
  fail_unavailable: number;
  fail_other: number;
  success_rate: number;
  avg_ms: number;
  p95_ms: number;
  p99_ms: number;
  status: ProviderHealthStatus;
  breaker: {
    fail_count: number;
    threshold: number;
    is_broken: boolean;
    recover_remaining_ms: number;
    window_secs: number;
  } | null;
}

export interface ProviderHealthResp {
  range_secs: number;
  sampled_at: number;
  items: ProviderHealthItem[];
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
  /** L1 路由级语义缓存 opt-in */
  cache_enabled: boolean;
}

export interface ImportRoutePayload {
  public_name: string;
  kind: string;
  base_url: string;
  upstream_model: string;
  weight?: number;
  multiplier?: number;
}

export interface ImportGroupPayload {
  name: string;
  strategy?: string;
  routes: ImportRoutePayload[];
  time_rules?: TimeRulePayload[];
}

export interface ImportPreviewRow {
  name: string;
  action: "create" | "overwrite";
  existing_routes: number;
  routes: number;
  time_rules: number;
  missing_models: { public_name: string; kind: string; base_url: string; upstream_model: string }[];
}

export interface ImportPreviewResp {
  data: ImportPreviewRow[];
  summary: { groups: number; create: number; overwrite: number; routes: number; time_rules: number; skipped_routes: number };
}

export const api = {
  login: (username: string, password: string) =>
    req("/admin/api/auth/login", { method: "POST", body: JSON.stringify({ username, password }) }),

  overview: () => req("/admin/api/overview"),
  overviewSeries: (granularity: "day" | "week" | "month"): Promise<{ granularity: string; data: { ts: number; tokens: number; calls: number }[] }> =>
    req(`/admin/api/overview/series?granularity=${granularity}`),

  users: (page?: number, pageSize?: number, q?: string): Promise<{ data: UserRow[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    if (q && q.trim()) qs.set("q", q.trim());
    const s = qs.toString();
    return req(`/admin/api/users${s ? "?" + s : ""}`);
  },
  createUser: (body: { username: string; password: string; email?: string; phone?: string; group_id?: number; grant_tokens?: number; concurrency_limit?: number; budget_daily_tokens?: number; budget_monthly_tokens?: number }) =>
    req("/admin/api/users", { method: "POST", body: JSON.stringify(body) }),
  patchUser: (id: string, body: any) =>
    req(`/admin/api/users/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteUser: (id: string) => req(`/admin/api/users/${id}`, { method: "DELETE" }),
  userSeries: (id: string, days?: number): Promise<{ data: { ts: number; tokens: number; calls: number }[] }> =>
    req(`/admin/api/users/${id}/series${typeof days === "number" ? `?days=${days}` : ""}`),
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
  providerModels: (name: string): Promise<{ data: ProviderModelRow[] }> => req(`/admin/api/providers/${name}/models`),
  deleteProvider: (name: string) => req(`/admin/api/providers/${name}`, { method: "DELETE" }),
  updateProvider: (name: string, body: { base_url: string; api_key?: string; display_name?: string | null }) =>
    req(`/admin/api/providers/${name}`, { method: "PUT", body: JSON.stringify(body) }),

  models: (): Promise<{ data: ModelRow[] }> => req("/admin/api/models"),
  addModel: (body: { label?: string; kind: string; base_url: string; api_key?: string; upstream_model: string; input_price?: number | null; output_price?: number | null; display_name?: string }) =>
    req("/admin/api/models", { method: "POST", body: JSON.stringify(body) }),
  updateModel: (id: number, body: { kind: string; base_url: string; api_key?: string; upstream_model: string; label?: string; input_price?: number | null; output_price?: number | null; context_length?: number | null; tags?: string }) =>
    req(`/admin/api/models/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  versionCheck: (): Promise<{ current: string; latest: string | null; has_update: boolean; release_url?: string; error?: string }> =>
    req("/admin/api/version/check"),
  deleteModel: (id: number) => req(`/admin/api/models/${id}`, { method: "DELETE" }),
  deleteModelsBatch: (ids: number[]): Promise<{ ok: boolean; deleted: number }> =>
    req("/admin/api/models/batch-delete", { method: "POST", body: JSON.stringify({ ids }) }),
  testModel: (id: number): Promise<{ ok: boolean; latency_ms?: number; error?: string }> =>
    req(`/admin/api/models/${id}/test`, { method: "POST" }),
  fetchModelList: (body: { kind: string; base_url: string; api_key?: string; provider?: string }): Promise<{ ok: boolean; models?: string[]; error?: string }> =>
    req("/admin/api/models/fetch-list", { method: "POST", body: JSON.stringify(body) }),
  addModelsBatch: (body: { kind: string; base_url: string; api_key?: string; display_name?: string; models: { upstream_model: string; label?: string; input_price?: number | null; output_price?: number | null }[] }): Promise<{ ok: boolean; provider?: string; provider_created?: boolean; added?: number; skipped?: number }> =>
    req("/admin/api/models/batch", { method: "POST", body: JSON.stringify(body) }),
  addModelsToProvider: (name: string, models: { upstream_model: string; label?: string; input_price?: number | null; output_price?: number | null }[]): Promise<{ ok: boolean; added?: number; skipped?: number }> =>
    req(`/admin/api/providers/${encodeURIComponent(name)}/models/batch`, { method: "POST", body: JSON.stringify({ models }) }),
  providerExists: (base_url: string, api_key?: string): Promise<{ exists: boolean; name?: string; models?: string[] }> =>
    req("/admin/api/providers/exists", { method: "POST", body: JSON.stringify({ base_url, api_key }) }),
  providerHealth: (rangeSecs = 3600): Promise<ProviderHealthResp> =>
    req(`/admin/api/providers/health?range_secs=${rangeSecs}`),

  groups: (): Promise<{ data: GroupRow[] }> => req("/admin/api/groups"),
  addGroup: (name: string) => req("/admin/api/groups", { method: "POST", body: JSON.stringify({ name }) }),
  deleteGroup: (id: number) => req(`/admin/api/groups/${id}`, { method: "DELETE" }),
  activateGroup: (id: number) => req(`/admin/api/groups/${id}/activate`, { method: "POST" }),
  renameGroup: (id: number, name: string) =>
    req(`/admin/api/groups/${id}`, { method: "PATCH", body: JSON.stringify({ name }) }),
  setGroupStrategy: (id: number, strategy: string) =>
    req(`/admin/api/groups/${id}/strategy`, { method: "POST", body: JSON.stringify({ strategy }) }),

  timeRules: (groupId: number): Promise<{ data: TimeRuleRow[] }> => req(`/admin/api/groups/${groupId}/time-rules`),
  addTimeRule: (groupId: number, body: TimeRulePayload) =>
    req(`/admin/api/groups/${groupId}/time-rules`, { method: "POST", body: JSON.stringify(body) }),
  updateTimeRule: (groupId: number, ruleId: number, body: TimeRulePayload) =>
    req(`/admin/api/groups/${groupId}/time-rules/${ruleId}`, { method: "PUT", body: JSON.stringify(body) }),
  deleteTimeRule: (groupId: number, ruleId: number) => req(`/admin/api/groups/${groupId}/time-rules/${ruleId}`, { method: "DELETE" }),

  routes: (groupId: number): Promise<{ data: RouteRow[] }> => req(`/admin/api/groups/${groupId}/routes`),
  addRoute: (groupId: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number; cache?: boolean }) =>
    req(`/admin/api/groups/${groupId}/routes`, { method: "POST", body: JSON.stringify(body) }),
  addRoutesBatch: (groupId: number, body: { routes: { public_name: string; model_id: number; weight?: number; multiplier?: number }[]; cache?: boolean }): Promise<{ ok: boolean; ids?: number[] }> =>
    req(`/admin/api/groups/${groupId}/routes/batch`, { method: "POST", body: JSON.stringify(body) }),
  updateRoute: (id: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number; cache?: boolean }) =>
    req(`/admin/api/routes/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteRoute: (id: number) => req(`/admin/api/routes/${id}`, { method: "DELETE" }),
  batchDeleteRoutes: (ids: number[]) =>
    req("/admin/api/routes/batch-delete", { method: "POST", body: JSON.stringify({ ids }) }),
  batchUpdateRoutes: (ids: number[], body: { weight?: number; multiplier?: number }) =>
    req("/admin/api/routes/batch-update", { method: "POST", body: JSON.stringify({ ids, ...body }) }),

  groupsExport: () => req("/admin/api/groups/export"),
  groupExport: (id: number) => req(`/admin/api/groups/${id}/export`),
  importPreview: (body: { groups: ImportGroupPayload[] }): Promise<ImportPreviewResp> =>
    req("/admin/api/groups/import/preview", { method: "POST", body: JSON.stringify(body) }),
  groupsImport: (body: { groups: ImportGroupPayload[] }): Promise<{ ok: boolean; imported: { id: number; name: string; action: string; routes: number; time_rules: number; skipped_routes: number }[] }> =>
    req("/admin/api/groups/import", { method: "POST", body: JSON.stringify(body) }),

  usage: (page?: number, pageSize?: number): Promise<{ data: any[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/usage${s ? "?" + s : ""}`);
  },

  // ---- 用量分布(按供应商/用户/密钥聚合,top10) ----
  usageBreakdown: (): Promise<UsageBreakdownResp> => req("/admin/api/usage/breakdown"),

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

  fallbackSettings: (): Promise<FallbackSettingsResp> => req("/admin/api/settings/fallback"),
  saveFallbackSettings: (body: FallbackSettingsBody) =>
    req("/admin/api/settings/fallback", { method: "POST", body: JSON.stringify(body) }),

  // ---- 语义缓存 ----
  cacheStats: (): Promise<CacheStatsResp> => req("/admin/api/cache/stats"),
  cacheHits: (limit?: number): Promise<{ hits: CacheHitRow[] }> =>
    req(`/admin/api/cache/hits${typeof limit === "number" ? `?limit=${limit}` : ""}`),
  clearCache: (): Promise<{ ok: boolean }> => req("/admin/api/cache/clear", { method: "POST" }),
  cacheSettings: (): Promise<CacheSettingsResp> => req("/admin/api/settings/cache"),
  saveCacheSettings: (body: CacheSettingsBody) =>
    req("/admin/api/settings/cache", { method: "POST", body: JSON.stringify(body) }),

  // ---- L2 语义缓存 embedding 供应商 ----
  embeddingSettings: (): Promise<EmbeddingSettingsResp> => req("/admin/api/settings/embedding"),
  saveEmbeddingSettings: (body: EmbeddingSettingsBody) =>
    req("/admin/api/settings/embedding", { method: "POST", body: JSON.stringify(body) }),
  testEmbedding: (
    body: EmbeddingSettingsBody & { test_text?: string },
  ): Promise<{ ok: boolean; dim?: number; error?: string }> =>
    req("/admin/api/settings/embedding/test", { method: "POST", body: JSON.stringify(body) }),

  // ---- 日志设置 ----
  loggingSettings: (): Promise<LoggingSettingsResp> => req("/admin/api/settings/logging"),
  saveLoggingSettings: (body: LoggingSettingsBody) =>
    req("/admin/api/settings/logging", { method: "POST", body: JSON.stringify(body) }),

  // ---- 门户设置 ----
  portalSettings: (): Promise<{ rewards_enabled: boolean }> => req("/admin/api/settings/portal"),
  savePortalSettings: (body: { rewards_enabled: boolean }) =>
    req("/admin/api/settings/portal", { method: "POST", body: JSON.stringify(body) }),

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
  requestLogs: (q?: string, page?: number, pageSize?: number, failed?: boolean | null, hours?: number | null): Promise<{ data: RequestLogRow[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (q) qs.set("q", q);
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    if (typeof failed === "boolean") qs.set("failed", String(failed));
    if (typeof hours === "number") qs.set("hours", String(hours));
    const s = qs.toString();
    return req(`/admin/api/request-logs${s ? "?" + s : ""}`);
  },

  // ---- 管理操作审计 ----
  auditLogs: (page?: number, pageSize?: number): Promise<{ data: AuditLogRow[]; total: number; page: number; page_size: number; total_pages: number }> => {
    const qs = new URLSearchParams();
    if (typeof page === "number") qs.set("page", String(page));
    if (typeof pageSize === "number") qs.set("page_size", String(pageSize));
    const s = qs.toString();
    return req(`/admin/api/audit-logs${s ? "?" + s : ""}`);
  },
};

export interface AuditLogRow {
  actor: string;
  method: string;
  path: string;
  status: number;
  ip: string;
  latency_ms: number;
  ts: number;
}

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

export interface FallbackSettingsResp {
  /** failover 总开关:关闭后仅尝试首个候选 */
  fallback_enabled: boolean;
  /** 最多尝试的候选数(含首个;0 = 不限) */
  max_retries: number;
}

export interface FallbackSettingsBody extends FallbackSettingsResp {}

// ============================================================
// 语义缓存
// ============================================================

/** 5 分钟一桶的命中/未命中趋势(共 12 桶,oldest → newest) */
export interface CacheTrendPoint { ts: number; hits: number; misses: number }

export interface CacheStatsResp {
  hits: number;
  misses: number;
  /** 命中率 0~1,3 位小数 */
  hit_rate: number;
  /** 累计节省的 tokens(输入+输出) */
  tokens_saved: number;
  /** 当前缓存条数 */
  entries: number;
  trend: CacheTrendPoint[];
}

export interface CacheHitRow {
  ts: number;
  /** exact = 精确哈希命中;semantic = 语义相似命中(批 D) */
  hit_type: string;
  /** 语义命中时的相似度(0~1),精确命中为 null */
  similarity: number | null;
  model: string;
  provider: string;
  tokens_saved: number;
}

export interface CacheSettingsResp {
  enabled: boolean;
  /** 缓存有效期(秒) */
  ttl_secs: number;
  /** 语义相似度阈值 0.5~0.95 */
  similarity_threshold: number;
  /** 消息条数超过该值的多轮对话跳过缓存 */
  multi_turn_max: number;
  /** L3 命中计费折扣率 0.0~1.0(0=命中免费,1=照常计费) */
  billing_ratio: number;
}

export interface CacheSettingsBody extends CacheSettingsResp {}

export interface EmbeddingSettingsResp {
  enabled: boolean;
  /** OpenAI 兼容接口基础地址,如 https://api.openai.com/v1 */
  base_url: string;
  /** embedding 模型名,如 text-embedding-3-small */
  model: string;
  /** 是否已配置 api_key(不回显明文) */
  has_key: boolean;
}

export interface LoggingSettingsResp {
  /** 请求/响应体预览采集上限(字节);0=不采集 */
  body_preview_max_bytes: number;
  /** 日志保留天数;0=永久保留 */
  retention_days: number;
}

export interface LoggingSettingsBody extends LoggingSettingsResp {}

export interface EmbeddingSettingsBody {
  enabled: boolean;
  base_url: string;
  model: string;
  /** api_key;留空/不传 = 保持已存值 */
  api_key?: string | null;
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
  /** 命中的供应商名(展示用);旧日志可能缺省 */
  provider?: string;
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
  /** 请求体预览(跟随存储后端:SQLite/PG 落列,ES 落文档字段;可能为 null) */
  req_body: string | null;
  /** 响应体预览(同 req_body;可能为 null) */
  resp_body: string | null;
}

// ================== 用量分布 ==================

/** 单个聚合行(按供应商/用户/密钥任一维度) */
export interface UsageBreakdownRow {
  label: string;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  charged_tokens: number;
  /** 计费口径成本(USD),消费端折算;未定价为 0 */
  cost_usd: number;
}

export interface UsageBreakdownResp {
  providers: UsageBreakdownRow[];
  users: UsageBreakdownRow[];
  keys: UsageBreakdownRow[];
}
