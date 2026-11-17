// Extract the text of every page of a PDF with pdf.js (the same renderer the app uses), for pack building.
// Usage: node apps/web/tools/pdf-pages.mjs in.pdf out.pages.json
// Output: a JSON array with one string per page (page 1 first). Pages without a text layer give "".
import { readFileSync, writeFileSync } from "node:fs";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";

const [, , input, output] = process.argv;
if (!input || !output) {
  console.error("usage: pdf-pages.mjs in.pdf out.pages.json");
  process.exit(2);
}
const doc = await getDocument({ data: new Uint8Array(readFileSync(input)), isEvalSupported: false, useSystemFonts: false }).promise;
const pages = [];
for (let i = 1; i <= doc.numPages; i++) {
  const page = await doc.getPage(i);
  const tc = await page.getTextContent();
  let text = "";
  for (const it of tc.items) {
    if ("str" in it) text += it.str + (it.hasEOL ? "\n" : " ");
  }
  pages.push(text.replace(/[ \t]+/g, " ").replace(/\n{3,}/g, "\n\n").trim());
}
writeFileSync(output, JSON.stringify(pages, null, 1) + "\n");
console.log(`${input}: ${doc.numPages} pages, ${pages.reduce((n, p) => n + p.length, 0)} characters of text`);
