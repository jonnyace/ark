// Plays videos of given codecs from an imported library and reports whether real frames are decoded
// (videoWidth > 0 and time advancing). A missing decoder shows as a running player over a black picture.
import { Session, sleep } from "./wd.mjs";
import { readFileSync } from "node:fs";
const [app, pack, list] = process.argv.slice(2);
const s = await Session.start("http://127.0.0.1:4444", app, { LBN_DATA: process.env.LBN_DATA });
let fails = 0;
try {
  await s.waitFor("return !!document.querySelector('h1')", 30000);
  for (const line of readFileSync(list, "utf8").trim().split("\n")) {
    const [codec, id] = line.split(" ");
    await s.go("#/read?c=" + encodeURIComponent(`lbn:${pack}/${id}#t=60`));
    const r = await s.execAsync(`const done=arguments[arguments.length-1]; (async()=>{
      for (let i=0;i<60 && !document.querySelector('[data-testid=video]');i++) await new Promise(r=>setTimeout(r,250));
      const v=document.querySelector('[data-testid=video]'); if(!v) return done('no video element');
      v.muted=true; for(let i=0;i<60 && v.readyState<2;i++) await new Promise(r=>setTimeout(r,250));
      const t0=v.currentTime; await v.play().catch(()=>{}); await new Promise(r=>setTimeout(r,3000)); v.pause();
      done({w:v.videoWidth,h:v.videoHeight,t0,t1:v.currentTime,err:v.error&&v.error.code});})()`);
    const ok = r && r.w > 0 && r.t1 > r.t0;
    if (!ok) fails++;
    console.log(`${ok ? "PASS" : "FAIL"} ${codec} video plays with real frames ${JSON.stringify(r)}`);
  }
} finally { await s.end(); process.exit(fails ? 1 : 0); }
