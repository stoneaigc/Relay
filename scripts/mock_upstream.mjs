// 本地 mock 上游(OpenAI 兼容):演示/联调用。
// 启动: node scripts/mock_upstream.mjs
// 端点: GET /v1/models、POST /v1/chat/completions、POST /v1/embeddings
// 与 config「本地服务」上游(http://127.0.0.1:18901/v1)配套。
import http from "node:http";

const PORT = 18901;

const CHAT_MODELS = ["mock-chat", "mock-fast"];

const json = (res, code, obj) => {
  const body = JSON.stringify(obj);
  res.writeHead(code, { "content-type": "application/json" });
  res.end(body);
};

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    const raw = Buffer.concat(chunks).toString("utf8");
    console.log(`[mock] ${req.method} ${url.pathname}`);

    if (req.method === "GET" && url.pathname === "/v1/models") {
      return json(res, 200, {
        object: "list",
        data: CHAT_MODELS.map((id) => ({ id, object: "model", owned_by: "mock" })),
      });
    }

    if (req.method === "POST" && url.pathname === "/v1/chat/completions") {
      let model = "mock-chat";
      let lastUser = "hello";
      try {
        const v = JSON.parse(raw || "{}");
        if (v.model) model = v.model;
        const msgs = Array.isArray(v.messages) ? v.messages : [];
        for (let i = msgs.length - 1; i >= 0; i--) {
          if (msgs[i].role === "user") { lastUser = String(msgs[i].content ?? ""); break; }
        }
        if (v.stream) {
          // 简单 SSE:两段增量 + done
          res.writeHead(200, { "content-type": "text/event-stream" });
          const frame = (delta, finish) =>
            `data: ${JSON.stringify({
              id: "chatcmpl-mock-stream",
              object: "chat.completion.chunk",
              model,
              choices: [{ index: 0, delta, finish_reason: finish ?? null }],
            })}\n\n`;
          res.write(frame({ role: "assistant" }));
          res.write(frame({ content: `[mock] 你说:「${lastUser}」。这是流式回复。` }));
          res.write(frame({}, "stop"));
          res.write("data: [DONE]\n\n");
          return res.end();
        }
        return json(res, 200, {
          id: "chatcmpl-mock-" + Date.now(),
          object: "chat.completion",
          created: Math.floor(Date.now() / 1000),
          model,
          choices: [{
            index: 0,
            message: { role: "assistant", content: `[mock] 你说:「${lastUser}」。这是本地 mock 上游的回复。` },
            finish_reason: "stop",
          }],
          usage: { prompt_tokens: 12, completion_tokens: 20, total_tokens: 32 },
        });
      } catch {
        return json(res, 400, { error: { message: "invalid json" } });
      }
    }

    if (req.method === "POST" && url.pathname === "/v1/embeddings") {
      let model = "mock-embedding";
      let inputs = ["hello"];
      try {
        const v = JSON.parse(raw || "{}");
        if (v.model) model = v.model;
        inputs = Array.isArray(v.input) ? v.input : [String(v.input ?? "hello")];
      } catch { /* 保持默认 */ }
      const vec = Array.from({ length: 64 }, (_, i) => Number(((i * 7 + inputs.length) % 13) / 13 - 0.5));
      return json(res, 200, {
        object: "list",
        model,
        data: inputs.map((_, i) => ({ object: "embedding", index: i, embedding: vec })),
        usage: { prompt_tokens: 8, total_tokens: 8 },
      });
    }

    json(res, 404, { error: { message: `mock upstream: no route for ${req.method} ${url.pathname}` } });
  });
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`[mock] OpenAI 兼容 mock 上游已启动: http://127.0.0.1:${PORT}/v1`);
  console.log(`[mock] 端点: GET /v1/models | POST /v1/chat/completions | POST /v1/embeddings`);
});
