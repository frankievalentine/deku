import { serve } from "@hono/node-server";
import { Hono } from "hono";

const app = new Hono();

app.get("/", (c) =>
  c.json({
    app: "hono",
    message: "Deku Hono starter"
  })
);

app.get("/health", (c) => c.json({ status: "ok" }));

const port = Number(process.env.PORT || 3000);

serve({
  fetch: app.fetch,
  port
});
