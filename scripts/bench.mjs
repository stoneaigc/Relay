#!/usr/bin/env node
// Relay 数据面基准脚本(零依赖,Node 18+)。
//
// 场景:
//   1. healthz  GET /healthz                  —— 纯 HTTP 栈(无鉴权)
//   2. models   GET /v1/models                —— 内存鉴权热路径(Bearer Key 哈希直查)
//   3. chat     POST /v1/chat/completions     —— 全链路:鉴权→路由→缓存(L1 必 miss)→上游→计费→异步落库
//
// chat 场景需要真实可用路由:脚本内嵌一个 mock OpenAI 上游(固定响应),
// 使用前在管理后台建好 模型(kind=openai, base_url 指向 mock)与模型组(对外模型名 bench)。
// 每请求 messages 携带随机 nonce,语义缓存 L1 永不命中,测的是最重的未命中路径。
//
// 用法:
//   node scripts/bench.mjs --key rk_live_xxx [--base http://localhost:8080]
//        [--duration 10] [--concurrency 16] [--model bench] [--mock-port 9999] [--skip-chat]
//
// 注意:压测用户并发上限需 ≥ --concurrency(默认 16,即全局默认限额),否则 429。

import { createServer } from "node:http";

const args = {};
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  if (a.startsWith("--")) args[a.slice(2)] = process.argv[i + 1] ?? true, i++;
}
const BASE = (args.base ?? "http://localhost:8080").replace(/\/$/, "");
const KEY = args.key ?? "";
const DURATION = Number(args.duration ?? 10);
const CONC = Number(args.concurrency ?? 16);
const MODEL = args.model ?? "bench";
const MOCK_PORT = Number(args["mock-port"] ?? 9999);
const SKIP_CHAT = !!args["skip-chat"];

const MOCK_RESP = JSON.stringify({
  id: "chatcmpl-bench",
  object: "chat.completion",
  created: 0,
  model: MODEL,
  choices: [{ index: 0, message: { role: "assistant", content: "ok" }, finish_reason: "stop" }],
  usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
});

function startMockUpstream() {
  return new Promise((resolve) => {
    const srv = createServer((req, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(MOCK_RESP);
    });
    srv.listen(MOCK_PORT, "127.0.0.1", () => resolve(srv));
  });
}

async function benchOnce(name, makeReq, durationMs) {
  const lat = [];
  let errs = 0;
  let lastErr = "";
  const t0 = Date.now();
  await Promise.all(
    Array.from({ length: CONC }, async () => {
      while (Date.now() - t0 < durationMs) {
        const s = performance.now();
        try {
          const res = await makeReq();
          if (!res.ok) {
            errs++;
            lastErr = `${res.status} ${(await res.text()).slice(0, 120)}`;
          } else {
            await res.arrayBuffer(); // 消费完才算完整响应
          }
        } catch (e) { errs++; lastErr = String(e).slice(0, 120); }
        lat.push(performance.now() - s);
      }
    })
  );
  const wall = (Date.now() - t0) / 1000;
  lat.sort((a, b) => a - b);
  const q = (p) => lat.length ? lat[Math.min(lat.length - 1, Math.floor((p / 100) * lat.length))].toFixed(1) : "-";
  return {
    name,
    rps: (lat.length / wall).toFixed(0),
    avg: lat.length ? (lat.reduce((s, v) => s + v, 0) / lat.length).toFixed(1) : "-",
    p50: q(50), p95: q(95), p99: q(99),
    errs, lastErr, total: lat.length,
  };
}

function chatBody() {
  return JSON.stringify({
    model: MODEL,
    messages: [{ role: "user", content: `hi ${Math.random().toString(36).slice(2)}` }],
    max_tokens: 16,
  });
}

const auth = { authorization: `Bearer ${KEY}` };
const scenarios = [
  { name: "healthz  GET /healthz (纯 HTTP 栈)", skip: false,
    req: () => fetch(`${BASE}/healthz`) },
  { name: "models   GET /v1/models (内存鉴权)", skip: !KEY,
    req: () => fetch(`${BASE}/v1/models`, { headers: auth }) },
  { name: `chat     POST /v1/chat/completions (全链路, model=${MODEL})`, skip: SKIP_CHAT || !KEY,
    req: () => fetch(`${BASE}/v1/chat/completions`, { method: "POST", headers: { ...auth, "content-type": "application/json" }, body: chatBody() }) },
];

console.log(`relay bench → ${BASE}  duration=${DURATION}s  concurrency=${CONC}`);
let mock;
if (!SKIP_CHAT) {
  mock = await startMockUpstream();
  console.log(`mock upstream listening on 127.0.0.1:${MOCK_PORT}`);
}
if (!KEY) console.log("(!) 未提供 --key,跳过鉴权场景\n");

const rows = [];
for (const sc of scenarios) {
  if (sc.skip) { rows.push({ name: sc.name, rps: "skip", avg: "-", p50: "-", p95: "-", p99: "-", errs: "-", total: 0 }); continue; }
  process.stdout.write(`▶ ${sc.name} ... `);
  const r = await benchOnce(sc.name, sc.req, DURATION * 1000);
  rows.push(r);
  console.log(`${r.rps} req/s  p99=${r.p99}ms`);
  await new Promise((r2) => setTimeout(r2, 800)); // 场景间冷却
}

mock?.close();

console.log("\n场景                                                  RPS    avg    P50    P95    P99   errs");
console.log("-".repeat(104));
for (const r of rows) {
  console.log(
    `${r.name.padEnd(52)} ${String(r.rps).padStart(5)} ${String(r.avg).padStart(6)} ${String(r.p50).padStart(6)} ${String(r.p95).padStart(6)} ${String(r.p99).padStart(6)} ${String(r.errs).padStart(6)}`
  );
}
const bad = rows.find((r) => Number(r.errs) > 0);
if (bad) console.log(`\n(!) ${bad.name} 有错误,样本: ${bad.lastErr}`);
console.log("\n备注: chat 为语义缓存 L1 未命中路径(每请求 nonce);release build 数字更具代表性。");
