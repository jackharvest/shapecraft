// Headless page driver: node cdp.mjs URL OUTDIR W H script.json
// script: [{"shot":"name"}, {"scroll":y}, {"wait":ms}, {"drag":[[x0,y0],[x1,y1]],"steps":n,"hold":true}, {"up":[x,y]}, {"eval":"js"}, {"fullshot":"name"}]
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs';
const [url, out, W, H, scriptPath] = process.argv.slice(2);
const script = JSON.parse(readFileSync(scriptPath, 'utf8'));
mkdirSync(out, { recursive: true });
const HS = process.env.HOME + '/Library/Caches/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-mac-arm64/chrome-headless-shell';
const port = 9300 + Math.floor(Math.random() * 500);
const proc = spawn(HS, [`--remote-debugging-port=${port}`, '--hide-scrollbars', '--autoplay-policy=no-user-gesture-required', `--window-size=${W},${H}`, 'about:blank'], { stdio: 'ignore' });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ws, id = 0; const pending = new Map();
for (let i = 0; i < 50; i++) {
  try { const r = await fetch(`http://127.0.0.1:${port}/json/list`); const tabs = await r.json(); const t = tabs.find(t => t.type === 'page'); if (t) { ws = new WebSocket(t.webSocketDebuggerUrl); break; } } catch {}
  await sleep(200);
}
await new Promise(r => ws.onopen = r);
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const send = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', { width: +W, height: +H, deviceScaleFactor: 1, mobile: false });
if (process.env.THEME) await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: process.env.THEME }] });
await send('Page.navigate', { url });
await sleep(2500);
const mouse = (type, x, y, buttons = 1) => send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons, clickCount: 1 });
for (const s of script) {
  if (s.wait) await sleep(s.wait);
  if (s.scroll !== undefined) { await send('Runtime.evaluate', { expression: `document.documentElement.style.scrollBehavior='auto';scrollTo(0,${s.scroll})` }); await sleep(900); }
  if (s.eval) { const r = await send('Runtime.evaluate', { expression: s.eval, returnByValue: true }); console.log(s.tag || 'eval', JSON.stringify(r.result?.result?.value)); }
  if (s.drag) {
    const [[x0, y0], [x1, y1]] = s.drag; const n = s.steps || 12;
    await mouse('mouseMoved', x0, y0, 0); await mouse('mousePressed', x0, y0);
    for (let i = 1; i <= n; i++) { await mouse('mouseMoved', x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n); await sleep(16); }
    if (!s.hold) await mouse('mouseReleased', x1, y1, 0);
  }
  if (s.up) await mouse('mouseReleased', s.up[0], s.up[1], 0);
  if (s.click) { await mouse('mouseMoved', s.click[0], s.click[1], 0); await mouse('mousePressed', ...s.click); await mouse('mouseReleased', s.click[0], s.click[1], 0); }
  if (s.shot || s.fullshot) {
    const p = { format: 'png' };
    if (s.fullshot) { const m = await send('Page.getLayoutMetrics'); p.captureBeyondViewport = true; p.clip = { x: 0, y: 0, width: +W, height: m.result.cssContentSize.height, scale: s.scale || 0.5 }; }
    const r = await send('Page.captureScreenshot', p);
    writeFileSync(`${out}/${s.shot || s.fullshot}.png`, Buffer.from(r.result.data, 'base64'));
  }
}
ws.close(); proc.kill();
process.exit(0);
