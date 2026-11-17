<script lang="ts">
  // Library HTML is untrusted. It is shown in an iframe with `sandbox="allow-same-origin"` (no scripts, forms, popups
  // or top navigation can run) plus a Content-Security-Policy that blocks every network load; scripts, frames,
  // objects, inline event handlers and external styles are removed before display. Images inside the pack are shown
  // from local files. Without scripts, the content cannot reach the app's privileged desktop commands.
  import { eng } from "../state.svelte";
  import type { ResourceInfo } from "../engine/gen/ResourceInfo";
  let { res, section }: { res: ResourceInfo; section: string | null } = $props();
  let frame: HTMLIFrameElement;
  let srcdoc = $state("");
  let err = $state("");
  const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/") + 1) : "");
  function norm(path: string): string | null {
    const out: string[] = [];
    for (const part of path.split("/")) {
      if (part === "" || part === ".") continue;
      if (part === "..") { if (!out.length) return null; out.pop(); } else out.push(part);
    }
    return out.join("/");
  }
  $effect(() => {
    const r = res;
    (async () => {
      try {
        const t = eng().t;
        const html = new TextDecoder().decode(await t.readFile(r.pack, r.path));
        const doc = new DOMParser().parseFromString(html, "text/html");
        doc.querySelectorAll("script,iframe,frame,frameset,object,embed,applet,link,meta[http-equiv],base,form,noscript").forEach((e) => e.remove());
        doc.querySelectorAll("*").forEach((el) => {
          for (const a of [...el.attributes]) {
            const n = a.name.toLowerCase();
            if (n.startsWith("on") || ((n === "href" || n === "src" || n === "xlink:href" || n === "action") && /^\s*(javascript|vbscript|data:text)/i.test(a.value))) el.removeAttribute(a.name);
          }
        });
        for (const img of doc.querySelectorAll("img[src]")) {
          const src = img.getAttribute("src")!;
          const rel = /^[a-z]+:/i.test(src) ? null : norm(dirOf(r.path) + src.split("#")[0].split("?")[0]);
          if (rel) img.setAttribute("src", await t.fileUrl(r.pack, rel).catch(() => ""));
          else img.removeAttribute("src");
        }
        doc.querySelectorAll("a[href]").forEach((a) => {
          const h = a.getAttribute("href")!;
          if (!h.startsWith("#")) { a.setAttribute("title", `Link not available offline: ${h}`); a.removeAttribute("href"); }
        });
        const csp = doc.createElement("meta");
        csp.setAttribute("http-equiv", "Content-Security-Policy");
        csp.setAttribute("content", "default-src 'none'; img-src blob: data: lbn: http://lbn.localhost; style-src 'unsafe-inline'; font-src data:");
        const style = doc.createElement("style");
        style.textContent = `body{font-family:system-ui,sans-serif;line-height:1.55;max-width:46rem;margin:0 auto;padding:0.5rem 1rem;font-size:1.05rem;color:#1b1f24;background:#fff}
          img{max-width:100%;height:auto} .lbn-target{background:#fff3c4;outline:3px solid #e07a00;outline-offset:4px;border-radius:4px}`;
        doc.head.prepend(style);
        doc.head.prepend(csp);
        srcdoc = "<!doctype html>" + doc.documentElement.outerHTML;
      } catch (e: any) {
        err = `This document could not be opened: ${e.message}`;
      }
    })();
  });
  function onload() {
    const d = frame.contentDocument;
    if (!d) return;
    frame.style.height = Math.max(400, d.documentElement.scrollHeight + 24) + "px";
    if (section) {
      const el = d.getElementById(section);
      if (el) {
        el.classList.add("lbn-target");
        let n = el.nextElementSibling;
        while (n && !/^H[1-3]$/.test(n.tagName)) { n.classList.add("lbn-target"); n = n.nextElementSibling; }
        const top = frame.getBoundingClientRect().top + window.scrollY + el.getBoundingClientRect().top - 16;
        window.scrollTo({ top });
      }
    }
  }
</script>

{#if err}<p class="bad" role="alert">{err}</p>{/if}
{#if section}
  <p class="small">Showing section: <strong>{res.sections.find((s) => s.id === section)?.title ?? section}</strong></p>
{/if}
<iframe bind:this={frame} {srcdoc} {onload} sandbox="allow-same-origin" title={`Document: ${res.title}`} style="width:100%;border:1px solid var(--line);border-radius:8px;background:#fff;min-height:400px" data-testid="html-view"></iframe>
