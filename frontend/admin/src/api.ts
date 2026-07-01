const TOKEN_KEY = "runapi_admin_token";

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
}

export interface GroupRow { id: number; name: string; is_active: boolean }

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

  users: (): Promise<{ data: UserRow[] }> => req("/admin/api/users"),
  createUser: (body: { username: string; password: string; email?: string; phone?: string; group_id?: number; grant_tokens?: number; concurrency_limit?: number }) =>
    req("/admin/api/users", { method: "POST", body: JSON.stringify(body) }),
  patchUser: (id: string, body: any) =>
    req(`/admin/api/users/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteUser: (id: string) => req(`/admin/api/users/${id}`, { method: "DELETE" }),
  userSeries: (id: string): Promise<{ data: { ts: number; tokens: number; calls: number }[] }> =>
    req(`/admin/api/users/${id}/series`),

  models: (): Promise<{ data: ModelRow[] }> => req("/admin/api/models"),
  addModel: (body: { label?: string; kind: string; base_url: string; api_key?: string; upstream_model: string }) =>
    req("/admin/api/models", { method: "POST", body: JSON.stringify(body) }),
  updateModel: (id: number, body: { kind: string; base_url: string; api_key?: string; upstream_model: string; label?: string }) =>
    req(`/admin/api/models/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteModel: (id: number) => req(`/admin/api/models/${id}`, { method: "DELETE" }),
  testModel: (id: number): Promise<{ ok: boolean; latency_ms?: number; error?: string }> =>
    req(`/admin/api/models/${id}/test`, { method: "POST" }),

  groups: (): Promise<{ data: GroupRow[] }> => req("/admin/api/groups"),
  addGroup: (name: string) => req("/admin/api/groups", { method: "POST", body: JSON.stringify({ name }) }),
  deleteGroup: (id: number) => req(`/admin/api/groups/${id}`, { method: "DELETE" }),
  activateGroup: (id: number) => req(`/admin/api/groups/${id}/activate`, { method: "POST" }),

  routes: (groupId: number): Promise<{ data: RouteRow[] }> => req(`/admin/api/groups/${groupId}/routes`),
  addRoute: (groupId: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number }) =>
    req(`/admin/api/groups/${groupId}/routes`, { method: "POST", body: JSON.stringify(body) }),
  updateRoute: (id: number, body: { public_name: string; model_id: number; weight?: number; multiplier?: number }) =>
    req(`/admin/api/routes/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteRoute: (id: number) => req(`/admin/api/routes/${id}`, { method: "DELETE" }),

  usage: () => req("/admin/api/usage"),

  rewards: (status?: "pending" | "approved" | "rejected"): Promise<RewardListResp> =>
    req(`/admin/api/rewards${status ? `?status=${status}` : ""}`),
  reviewReward: (id: string, approve: boolean, note?: string, reward_tokens?: number) =>
    req(`/admin/api/rewards/${id}/review`, { method: "POST", body: JSON.stringify({ approve, note, reward_tokens }) }),

  rewardTasks: (): Promise<{ data: RewardTaskRow[] }> => req("/admin/api/reward-tasks"),
  createRewardTask: (body: RewardTaskBody) =>
    req("/admin/api/reward-tasks", { method: "POST", body: JSON.stringify(body) }),
  updateRewardTask: (id: number, body: RewardTaskBody) =>
    req(`/admin/api/reward-tasks/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteRewardTask: (id: number) => req(`/admin/api/reward-tasks/${id}`, { method: "DELETE" }),
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
}
