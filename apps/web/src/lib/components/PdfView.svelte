<script lang="ts">
  // PDF pages rendered by pdf.js (bundled; its fonts and character maps are local files of the app).
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import { eng } from "../state.svelte";
  import type { ResourceInfo } from "../engine/gen/ResourceInfo";
  let { res, page = 1, onplace }: { res: ResourceInfo; page?: number; onplace: (p: number) => void } = $props();
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  let canvas: HTMLCanvasElement;
  let doc: pdfjs.PDFDocumentProxy | null = null;
  let n = $state(1);
  let count = $state(0);
  let text = $state("");
  let err = $state("");
  let busy = $state(false);
  $effect(() => { n = page; });
  $effect(() => {
    const r = res;
    (async () => {
      try {
        const t = eng().t;
        const assets = new URL("./pdfjs/", document.baseURI).href;
        const src: any = { cMapUrl: assets + "cmaps/", cMapPacked: true, standardFontDataUrl: assets + "standard_fonts/", isEvalSupported: false, enableXfa: false };
        if (t.platform === "desktop") src.url = await t.fileUrl(r.pack, r.path);
        else src.data = new Uint8Array(await t.readFile(r.pack, r.path));
        doc = await pdfjs.getDocument(src).promise;
        count = doc.numPages;
        render(n);
      } catch (e: any) {
        err = `This PDF could not be opened: ${e.message}`;
      }
    })();
  });
  $effect(() => { if (doc) render(n); });
  async function render(p: number) {
    if (!doc || p < 1 || p > count) return;
    busy = true;
    const pg = await doc.getPage(p);
    const width = Math.min(canvas.parentElement!.clientWidth, 1000);
    const vp0 = pg.getViewport({ scale: 1 });
    const scale = (width / vp0.width) * (window.devicePixelRatio || 1);
    const vp = pg.getViewport({ scale });
    canvas.width = vp.width;
    canvas.height = vp.height;
    canvas.style.width = width + "px";
    await pg.render({ canvas, viewport: vp } as any).promise;
    const tc = await pg.getTextContent();
    text = tc.items.map((i: any) => i.str + (i.hasEOL ? "\n" : " ")).join("");
    busy = false;
    onplace(p);
  }
</script>

{#if err}<p class="bad" role="alert">{err}</p>{/if}
<div class="row" style="margin:0.5rem 0">
  <button class="secondary" onclick={() => n--} disabled={n <= 1}>‹ Previous page</button>
  <label for="pdf-page" class="sr-only">Page</label>
  <span>Page <input id="pdf-page" type="number" min="1" max={count} bind:value={n} style="width:5rem" /> of {count}</span>
  <button class="secondary" onclick={() => n++} disabled={n >= count}>Next page ›</button>
</div>
<div aria-busy={busy}>
  <canvas bind:this={canvas} data-testid="pdf-canvas" data-page={n} aria-label={`Page ${n} of ${res.title}`} style="border:1px solid var(--line);background:#fff;max-width:100%"></canvas>
</div>
<details>
  <summary>Text of this page</summary>
  <pre data-testid="pdf-text" style="white-space:pre-wrap;font-family:inherit">{text}</pre>
</details>
