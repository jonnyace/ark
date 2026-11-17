// Desktop end-to-end journey (WebDriver through tauri-driver, WebKitGTK under Xvfb).
//   phase "install": packs over HTTP from the local test server + the AI model from a .lbpack path.
//   phase "offline": run with --network none: restart, search, read HTML/PDF/video/map, ask the local AI, open citations.
import { Session, sleep } from "./wd.mjs";
import { writeFileSync } from "node:fs";
const [app, phase, out] = process.argv.slice(2);
const DATA = process.env.LBN_DATA;
const results = [];
const check = (name, ok, detail = "") => { results.push({ name, ok: !!ok, detail }); console.log(`${ok ? "PASS" : "FAIL"} ${name} ${detail}`); };
const s = await Session.start("http://127.0.0.1:4444", app, { LBN_DATA: DATA });
const shot = (n) => s.screenshot(`${out}/desktop-${phase}-${n}.png`).catch(() => {});
try {
  await s.waitFor("return document.querySelector('h1')?.textContent === 'Your offline library'", 30000, "home");
  if (phase === "install") {
    await s.go("#/packs");
    await s.waitFor("return !!document.querySelector('#cat-url')");
    await s.fill("#cat-url", "http://127.0.0.1:18787/packs/catalog.json");
    await s.clickText("button", "Show packs");
    await s.waitFor("return document.querySelectorAll('[data-testid=catalog-pack]').length >= 6", 30000, "catalog");
    check("catalog signed", await s.exec("return /signed by Ark publisher/.test(document.querySelector('main').textContent)"));
    for (const id of ["docs", "video", "map", "encyclopedia"]) {
      await s.exec(`document.querySelector('[data-testid=catalog-pack][data-pack="org.lastbook.sample.${id}"] button').click()`);
      await s.waitFor("return !!document.querySelector('[data-testid=confirm]')", 20000, "confirm " + id);
      await s.clickText("button", "Install");
      await s.waitFor(`return !!document.querySelector('[data-testid=pack][data-pack="org.lastbook.sample.${id}"]')`, 60000, "installed " + id);
      check(`install ${id} over HTTP`, true);
    }
    await s.fill("#addr", "/src/dist/packs/org.lastbook.model.qwen2.5-1.5b-instruct.lbpack");
    await s.clickText("button", "Check");
    await s.waitFor("return !!document.querySelector('[data-testid=confirm]')", 30000, "confirm model");
    await shot("confirm-model");
    await s.clickText("button", "Install");
    await s.waitFor(`return !!document.querySelector('[data-testid=pack][data-pack="org.lastbook.model.qwen2.5-1.5b-instruct"]')`, 300000, "model installed");
    check("install AI model from .lbpack file", true);
    await s.go("#/ai");
    await s.waitFor("return /Runtime available/.test(document.querySelector('main').textContent)", 60000, "runtime");
    await s.exec("const c=[...document.querySelectorAll('label')].find(l=>l.textContent.includes('Use local AI')).querySelector('input'); if(!c.checked) c.click(); return true");
    await sleep(1000);
    check("AI enabled with fit check", await s.exec("return /Fits: needs about/.test(document.querySelector('main').textContent)"));
    await shot("ai");
  } else {
    check("network really absent", await s.execAsync("const done=arguments[arguments.length-1]; fetch('https://example.com',{mode:'no-cors'}).then(()=>done(false)).catch(()=>done(true))"));
    check("packs present after restart", (await s.exec("return document.querySelector('main').textContent")).includes("4 packs"));
    await s.go("#/search?q=" + encodeURIComponent("bleach drops gallon"));
    await s.waitFor("return document.querySelectorAll('main ol li').length > 0", 20000, "search hits");
    check("search offline", (await s.text("main ol li a")).includes("Disinfecting with household bleach"));
    await shot("search");
    const read = async (uri) => { await s.go("#/read?c=" + encodeURIComponent(uri)); await sleep(500); };
    await read("lbn:org.lastbook.sample.docs/water-guide#sec=bleach");
    check("HTML section highlighted", await s.waitFor("const f=document.querySelector('[data-testid=html-view]'); return f && f.contentDocument && f.contentDocument.querySelector('.lbn-target')?.id === 'bleach'", 20000, "html"));
    check("HTML viewer has no scripts", await s.exec("const f=document.querySelector('[data-testid=html-view]'); return f.getAttribute('sandbox')==='allow-same-origin' && f.contentDocument.querySelectorAll('script').length===0"));
    await shot("html");
    await read("lbn:org.lastbook.sample.docs/supplies-checklist#page=2");
    await s.waitFor("return document.querySelector('[data-testid=pdf-canvas]')?.dataset.page === '2' && document.querySelector('[data-testid=pdf-text]').textContent.length > 50", 30000, "pdf page 2");
    check("PDF page 2 rendered with text", (await s.text("[data-testid=pdf-text]")).includes("wrench"));
    await shot("pdf");
    await read("lbn:org.lastbook.sample.video/storing-water#t=27");
    const v = await s.waitFor("const v=document.querySelector('[data-testid=video]'); return v && v.readyState>=1 && Math.abs(v.currentTime-27)<0.6 ? [v.currentTime, v.readyState, v.duration] : null", 30000, "video seek");
    check("video seeks to cited time", true, JSON.stringify(v));
    const played = await s.execAsync("const done=arguments[arguments.length-1]; const v=document.querySelector('[data-testid=video]'); v.muted=true; const t0=v.currentTime; v.play().then(()=>setTimeout(()=>{v.pause(); done([t0, v.currentTime, v.error && v.error.code])},2000)).catch(e=>done(String(e)))");
    check("video plays", Array.isArray(played) && played[1] > played[0], JSON.stringify(played));
    check("transcript listed", await s.waitFor("return document.querySelectorAll('section[aria-label=Transcript] li').length === 6", 10000, "transcript"));
    await shot("video");
    await read("lbn:org.lastbook.sample.map/ithaca#at=42.4396,-76.4970,15");
    const mapOk = await s.waitFor("const m=document.querySelector('[data-testid=map]'); const e=document.querySelector('main .bad'); return (m && m.dataset.loaded==='true') ? 'loaded' : (e ? 'error: '+e.textContent : null)", 40000, "map");
    check("map draws offline", mapOk === "loaded", mapOk);
    await sleep(2000);
    await shot("map");
    await s.go("#/ask");
    await s.waitFor("return !!document.querySelector('#ask-q')", 20000, "ask form");
    await s.fill("#ask-q", "How long should I boil water to make it safe at high elevation?");
    await s.clickText("button", "Ask");
    await s.waitFor("return !!document.querySelector('[data-testid=answer]')", 300000, "answer");
    const ans = await s.text("[data-testid=answer-text]");
    check("AI answered offline with the fact", /three minutes|3 minutes/i.test(ans), JSON.stringify(ans));
    const cites = await s.exec("return [...document.querySelectorAll('[data-testid=cite]')].map(a=>a.getAttribute('href'))");
    check("answer has citations", cites.length > 0, JSON.stringify(cites));
    await shot("answer");
    for (const href of [...new Set(cites)]) {
      await s.go(href.slice(1));
      const h = await s.waitFor("const h=document.querySelector('h1')?.textContent; return h && h!=='Opening…' && h!=='Ask your library' ? h : null", 20000, "citation page");
      check(`citation opens (${decodeURIComponent(href).slice(8)})`, h !== "This source cannot be opened", h);
    }
    await shot("citation");
    await s.go("#/");
    await s.go("#/ask");
    await s.waitFor("return !!document.querySelector('#ask-q') && !document.querySelector('[data-testid=answer]')", 20000, "fresh ask form");
    await s.fill("#ask-q", "What is the capital of Australia?");
    await s.clickText("button", "Ask");
    await s.waitFor("return document.querySelector('[data-testid=answer]')?.dataset.outcome === 'no_evidence'", 60000, "refusal");
    check("out-of-library question refused without generation", true);
  }
} catch (e) {
  check("journey", false, e.message);
  await shot("failure");
} finally {
  await s.end();
  writeFileSync(`${out}/desktop-${phase}-results.json`, JSON.stringify(results, null, 1));
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed}/${results.length} checks passed`);
  process.exit(failed ? 1 : 0);
}
