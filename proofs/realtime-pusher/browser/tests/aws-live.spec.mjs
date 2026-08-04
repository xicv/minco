import { test, expect } from '@playwright/test';
import path from 'node:path';

const wsHost = process.env.MINCO_REALTIME_PROOF_WS_HOST;

test.skip(!wsHost, 'MINCO_REALTIME_PROOF_WS_HOST is required for the live AWS proof');

test('unmodified pusher-js connects and receives a typed event through AWS', async ({ page }) => {
  const pusherBrowserPath = path.resolve(
    'node_modules/pusher-js/dist/web/pusher.min.js',
  );
  await page.goto('about:blank');
  await page.addScriptTag({ path: pusherBrowserPath });

  const result = await page.evaluate(async (host) => {
    const pusher = new window.Pusher('proof-key', {
      wsHost: host,
      wsPort: 443,
      wssPort: 443,
      forceTLS: true,
      enabledTransports: ['ws'],
      disableStats: true,
    });
    const channel = pusher.subscribe('public-orders');

    return await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`timed out in state ${pusher.connection.state}`));
      }, 20_000);
      channel.bind('order.updated', (data) => {
        clearTimeout(timeout);
        const outcome = {
          state: pusher.connection.state,
          socketId: pusher.connection.socket_id,
          data,
        };
        pusher.disconnect();
        resolve(outcome);
      });
    });
  }, wsHost);

  expect(result.state).toBe('connected');
  expect(result.socketId).toMatch(/^\d+\.\d+$/);
  expect(result.data).toEqual({ order_id: 'proof-order', status: 'ready' });
});
