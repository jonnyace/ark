// Desktop smoke: open the app under Xvfb and check the engine answers.
import { Session, sleep } from "./wd.mjs";
const app = process.argv[2], out = process.argv[3] ?? "/tmp";
const s = await Session.start("http://127.0.0.1:4444", app, { LBN_DATA: process.env.LBN_DATA ?? "/tmp/lbn-data" });
try {
  await s.waitFor("return document.querySelector('h1')?.textContent", 30000, "home heading");
  console.log("h1:", await s.text("h1"));
  console.log("platform:", await s.exec("return document.querySelector('main').textContent.slice(0,200)"));
  await s.screenshot(`${out}/desktop-home.png`);
} finally { await s.end(); }
