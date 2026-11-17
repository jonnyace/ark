// Checks that personal data written by an older app (schema 1) shows up, migrated, in the installed app.
import { Session } from "./wd.mjs";
const [app, out] = process.argv.slice(2);
const s = await Session.start("http://127.0.0.1:4444", app, { LBN_DATA: process.env.LBN_DATA });
let ok = false;
try {
  await s.waitFor("return document.querySelector('h1')?.textContent === 'Your offline library'", 30000);
  await s.go("#/saved");
  const href = await s.waitFor("const a=[...document.querySelectorAll('main a')].find(a=>a.textContent.includes('Kit upkeep (from the old app)')); return a && a.getAttribute('href')", 20000, "migrated bookmark");
  ok = decodeURIComponent(href).includes("lbn:org.lastbook.sample.docs/supplies-checklist#page=3");
  console.log(`${ok ? "PASS" : "FAIL"} schema-1 bookmark migrated and shown (${decodeURIComponent(href)})`);
  await s.screenshot(`${out}/desktop-saved.png`);
} finally {
  await s.end();
  process.exit(ok ? 0 : 1);
}
