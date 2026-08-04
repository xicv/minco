import { createRequire } from "node:module";
import { expect, test } from "@playwright/test";

const require = createRequire(import.meta.url);
const pusherBrowserBuild = require.resolve("pusher-js/dist/web/pusher.min.js");

test("unmodified pusher-js reaches connected against the Rust server", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const state = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        });
        const timer = window.setTimeout(() => {
          resolve(pusher.connection.state);
          pusher.disconnect();
        }, 5_000);
        pusher.connection.bind("connected", () => {
          window.clearTimeout(timer);
          resolve(pusher.connection.state);
          pusher.disconnect();
        });
      }),
  );

  expect(state).toBe("connected");
});

test("a public channel reaches the subscribed state", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const subscribed = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        });
        const channel = pusher.subscribe("orders");
        const timer = window.setTimeout(() => {
          resolve(false);
          pusher.disconnect();
        }, 5_000);
        channel.bind("pusher:subscription_succeeded", () => {
          window.clearTimeout(timer);
          resolve(true);
          pusher.disconnect();
        });
      }),
  );

  expect(subscribed).toBe(true);
});

test("a subscribed public channel receives a typed server event", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const payload = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        });
        const channel = pusher.subscribe("orders");
        const timer = window.setTimeout(() => {
          resolve(null);
          pusher.disconnect();
        }, 5_000);
        channel.bind("order.updated", (event) => {
          window.clearTimeout(timer);
          resolve(event);
          pusher.disconnect();
        });
      }),
  );

  expect(payload).toEqual({ order_id: "ord-123", version: 7 });
});

test("a correctly authorised private channel reaches the subscribed state", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const result = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
          channelAuthorization: {
            endpoint: "/realtime/auth",
            transport: "ajax",
          },
        });
        const channel = pusher.subscribe("private-orders");
        const timer = window.setTimeout(() => {
          resolve({ subscribed: false, error: "timeout" });
          pusher.disconnect();
        }, 5_000);
        channel.bind("pusher:subscription_succeeded", () => {
          window.clearTimeout(timer);
          resolve({ subscribed: true, error: null });
          pusher.disconnect();
        });
        channel.bind("pusher:subscription_error", (error) => {
          window.clearTimeout(timer);
          resolve({ subscribed: false, error });
          pusher.disconnect();
        });
      }),
  );

  expect(result).toEqual({ subscribed: true, error: null });
});

test("an invalid private-channel signature is rejected by the Rust server", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const result = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
          channelAuthorization: {
            customHandler: (_params, callback) => {
              callback(null, { auth: "proof-key:invalid" });
            },
          },
        });
        const channel = pusher.subscribe("private-orders");
        const timer = window.setTimeout(() => {
          resolve({ rejected: false, error: "timeout" });
          pusher.disconnect();
        }, 5_000);
        channel.bind("pusher:subscription_succeeded", () => {
          window.clearTimeout(timer);
          resolve({ rejected: false, error: "subscribed" });
          pusher.disconnect();
        });
        channel.bind("pusher:subscription_error", (error) => {
          window.clearTimeout(timer);
          resolve({ rejected: true, error });
          pusher.disconnect();
        });
      }),
  );

  expect(result).toEqual({
    rejected: true,
    error: { code: "invalid_channel_authorization", status: 403 },
  });
});

test("pusher-js answers a protocol ping with pong", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const pongObserved = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        });
        const timer = window.setTimeout(() => {
          resolve(false);
          pusher.disconnect();
        }, 5_000);
        pusher.bind("proof.pong_observed", () => {
          window.clearTimeout(timer);
          resolve(true);
          pusher.disconnect();
        });
      }),
  );

  expect(pongObserved).toBe(true);
});

test("a published event excludes only the originating socket", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const result = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const options = {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        };
        const origin = new window.Pusher("proof-key", options);
        const peer = new window.Pusher("proof-key", options);
        const originChannel = origin.subscribe("orders");
        const peerChannel = peer.subscribe("orders");
        let originReceived = false;
        let peerPayload = null;
        originChannel.bind("proof.excluded", () => {
          originReceived = true;
        });
        peerChannel.bind("proof.excluded", (payload) => {
          peerPayload = payload;
        });
        const timer = window.setTimeout(() => {
          resolve({ originReceived, peerPayload, publishStatus: null });
          origin.disconnect();
          peer.disconnect();
        }, 5_000);
        Promise.all([
          new Promise((ready) =>
            originChannel.bind("pusher:subscription_succeeded", ready),
          ),
          new Promise((ready) =>
            peerChannel.bind("pusher:subscription_succeeded", ready),
          ),
        ]).then(async () => {
          const response = await fetch("/proof/publish", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              channel: "orders",
              event: "proof.excluded",
              data: { order_id: "ord-123", version: 8 },
              exclude_socket_id: origin.connection.socket_id,
            }),
          });
          window.setTimeout(() => {
            window.clearTimeout(timer);
            resolve({
              originReceived,
              peerPayload,
              publishStatus: response.status,
            });
            origin.disconnect();
            peer.disconnect();
          }, 300);
        });
      }),
  );

  expect(result).toEqual({
    originReceived: false,
    peerPayload: { order_id: "ord-123", version: 8 },
    publishStatus: 202,
  });
});

test("pusher-js reconnects and resubscribes after a gateway-style close", async ({ page }) => {
  await page.goto("http://127.0.0.1:3210/");
  await page.addScriptTag({ path: pusherBrowserBuild });

  const result = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const pusher = new window.Pusher("proof-key", {
          cluster: "mt1",
          wsHost: "127.0.0.1",
          wsPort: 3210,
          forceTLS: false,
          enabledTransports: ["ws"],
          enableStats: false,
        });
        const socketIds = [];
        let subscriptions = 0;
        let disconnectStatus = null;
        const channel = pusher.subscribe("orders");
        const timer = window.setTimeout(() => {
          resolve({ socketIds, subscriptions, disconnectStatus });
          pusher.disconnect();
        }, 8_000);
        pusher.connection.bind("connected", () => {
          socketIds.push(pusher.connection.socket_id);
        });
        channel.bind("pusher:subscription_succeeded", async () => {
          subscriptions += 1;
          if (subscriptions === 1) {
            const response = await fetch(
              `/proof/disconnect/${pusher.connection.socket_id}`,
              { method: "POST" },
            );
            disconnectStatus = response.status;
            return;
          }
          window.clearTimeout(timer);
          resolve({ socketIds, subscriptions, disconnectStatus });
          pusher.disconnect();
        });
      }),
  );

  expect(result).toEqual({
    socketIds: expect.arrayContaining([expect.any(String), expect.any(String)]),
    subscriptions: 2,
    disconnectStatus: 202,
  });
  expect(new Set(result.socketIds).size).toBe(2);
});
