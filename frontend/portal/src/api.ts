const TOKEN_KEY = "relay_portal_token";

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
  if (!res.ok) {
    throw new Error(data?.error?.message || res.statusText);
  }
  return data;
}

export interface KeyInfo {
  id: string;
  interface_kind: string;
  key_prefix: string;
  revoked: boolean;
  created_at: string;
}

export const api = {
  login: (username: string, password: string) =>
    req("/portal/api/auth/login", { method: "POST", body: JSON.stringify({ username, password }) }),
  sendEmailCode: (email: string): Promise<{ sent: boolean; dev_code?: string }> =>
    req("/portal/api/auth/email/send_code", { method: "POST", body: JSON.stringify({ email }) }),
  register: (email: string, code: string, password: string) =>
    req("/portal/api/auth/register", { method: "POST", body: JSON.stringify({ email, code, password }) }),
  resetPassword: (email: string, code: string, password: string) =>
    req("/portal/api/auth/reset_password", { method: "POST", body: JSON.stringify({ email, code, password }) }),
  me: () => req("/portal/api/me"),
  balance: () => req("/portal/api/balance"),
  keys: (): Promise<{ data: KeyInfo[] }> => req("/portal/api/keys"),
  createKey: (interface_kind: string) =>
    req("/portal/api/keys", { method: "POST", body: JSON.stringify({ interface_kind }) }),
  rotateKey: (interface_kind: string) =>
    req("/portal/api/keys/rotate", { method: "POST", body: JSON.stringify({ interface_kind }) }),
  models: (): Promise<{ data: string[] }> => req("/portal/api/models"),
  summary: (): Promise<{ granted: number; used: number; balance: number }> => req("/portal/api/summary"),
  series: (): Promise<{ data: { ts: number; tokens: number; calls: number }[] }> => req("/portal/api/series"),
  usage: () => req("/portal/api/usage"),
  rewards: (): Promise<RewardInfo> => req("/portal/api/rewards"),
  claimReward: (task_id: number, evidence?: string) =>
    req("/portal/api/rewards", { method: "POST", body: JSON.stringify({ task_id, evidence }) }),
};

export type EvidenceType = "screenshot" | "link" | "text" | "none";

export interface RewardTask {
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

export interface RewardClaim {
  id: string;
  task_id: number | null;
  title: string | null;
  evidence: string | null;
  reward_tokens: number;
  status: number; // 0=待审 1=已通过 2=已驳回
  review_note: string | null;
  created_at: string;
  reviewed_at: string | null;
}

export interface RewardInfo {
  tasks: RewardTask[];
  claims: RewardClaim[];
}

export interface ChatMsg {
  role: string;
  content: string;
  reasoning?: string;
}

export interface ChatDelta {
  content?: string;
  reasoning?: string;
}

/** 流式对话:逐段回调 onDelta(content / reasoning 思考过程);经门户 JWT 调用网关数据面。 */
export async function chatStream(
  model: string,
  messages: ChatMsg[],
  onDelta: (d: ChatDelta) => void,
  signal?: AbortSignal
): Promise<void> {
  const res = await fetch("/portal/api/chat", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${getToken()}`,
    },
    body: JSON.stringify({ model, stream: true, messages }),
    signal,
  });
  if (!res.ok || !res.body) {
    const t = await res.text();
    try { throw new Error(JSON.parse(t)?.error?.message || t); } catch { throw new Error(t || res.statusText); }
  }
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let idx: number;
    while ((idx = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, idx).trim();
      buf = buf.slice(idx + 1);
      if (!line.startsWith("data:")) continue;
      const payload = line.slice(5).trim();
      if (!payload || payload === "[DONE]") continue;
      try {
        const j = JSON.parse(payload);
        const d = j?.choices?.[0]?.delta;
        if (!d) continue;
        // 思考过程:兼容主流字段名 reasoning_content(DeepSeek 等)/ reasoning(OpenRouter 等)/ thinking。
        const reasoning = d.reasoning_content ?? d.reasoning ?? d.thinking;
        if (d.content || reasoning) onDelta({ content: d.content || undefined, reasoning: reasoning || undefined });
      } catch { /* 跳过非 JSON 行 */ }
    }
  }
}
