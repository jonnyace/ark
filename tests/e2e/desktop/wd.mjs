// Minimal W3C WebDriver client (no dependencies) for driving the Tauri desktop app through tauri-driver.
export class Session {
  constructor(base, id) { this.base = base; this.id = id; }
  static async start(base, application, env = {}) {
    const r = await fetch(`${base}/session`, { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ capabilities: { alwaysMatch: { "tauri:options": { application, env } } } }) });
    const j = await r.json();
    if (!j.value?.sessionId) throw new Error("session failed: " + JSON.stringify(j).slice(0, 500));
    return new Session(base, j.value.sessionId);
  }
  async cmd(method, path, body) {
    const r = await fetch(`${this.base}/session/${this.id}${path}`, { method, headers: { "content-type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
    const j = await r.json();
    if (j.value && j.value.error) throw new Error(`${path}: ${j.value.error}: ${j.value.message}`);
    return j.value;
  }
  exec(script, ...args) { return this.cmd("POST", "/execute/sync", { script, args }); }
  execAsync(script, ...args) { return this.cmd("POST", "/execute/async", { script, args }); }
  async go(hash) { await this.exec("location.hash = arguments[0]", hash); await sleep(300); }
  async screenshot(file) { const b64 = await this.cmd("GET", "/screenshot"); (await import("node:fs")).writeFileSync(file, Buffer.from(b64, "base64")); }
  async waitFor(script, timeoutMs = 30000, label = script) {
    const t = Date.now();
    let last;
    while (Date.now() - t < timeoutMs) {
      try { last = await this.exec(script); if (last) return last; } catch (e) { last = e.message; }
      await sleep(250);
    }
    throw new Error(`timed out waiting for: ${label} (last: ${JSON.stringify(last)?.slice(0, 300)})`);
  }
  async click(selector) { return this.exec("const e=document.querySelector(arguments[0]); if(!e) throw new Error('no '+arguments[0]); e.click(); return true", selector); }
  async clickText(tag, text) { return this.exec("const e=[...document.querySelectorAll(arguments[0])].find(x=>x.textContent.trim()===arguments[1]); if(!e) throw new Error('no '+arguments[1]); e.click(); return true", tag, text); }
  async fill(selector, value) { return this.exec("const e=document.querySelector(arguments[0]); const set=Object.getOwnPropertyDescriptor(e.__proto__,'value').set; set.call(e,arguments[1]); e.dispatchEvent(new Event('input',{bubbles:true})); return true", selector, value); }
  async text(selector) { return this.exec("return document.querySelector(arguments[0])?.textContent ?? null", selector); }
  async end() { try { await fetch(`${this.base}/session/${this.id}`, { method: "DELETE" }); } catch {} }
}
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
