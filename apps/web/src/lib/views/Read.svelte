<script lang="ts">
  import { eng, app, say } from "../state.svelte";
  import { route } from "../router.svelte";
  import type { ResolvedCitation } from "../engine/gen/ResolvedCitation";
  import HtmlView from "../components/HtmlView.svelte";
  import PdfView from "../components/PdfView.svelte";
  import VideoView from "../components/VideoView.svelte";
  import MapView from "../components/MapView.svelte";
  let rc = $state<ResolvedCitation | null>(null);
  let here = $state(""); // current position as a citation URI (page, time, section, map position)
  let note = $state("");
  const uri = $derived(route.params.get("c") ?? "");
  $effect(() => {
    const u = uri;
    rc = null;
    eng().call<ResolvedCitation>({ cmd: "resolve_citation", args: { uri: u } }).then((r) => {
      rc = r;
      here = u;
      if (r.valid && r.resource) eng().call({ cmd: "add_history", args: { target: u, title: r.resource.title } }).catch(() => {});
    });
  });
  const r = $derived(rc?.resource);
  const loc: any = $derived(rc?.citation?.loc ?? { kind: "whole" });
  const bookmarked = $derived(app.personal?.bookmarks.find((b) => b.target === here));
  async function bookmark() {
    if (bookmarked) await eng().call({ cmd: "delete_bookmark", args: { id: bookmarked.id } });
    else await eng().call({ cmd: "save_bookmark", args: { target: here, title: r!.title + (here.includes("#") ? ` (${here.split("#")[1].replace("=", " ")})` : "") } });
    say(bookmarked ? "Bookmark removed" : "Bookmarked");
  }
  async function saveNote() {
    if (!note.trim()) return;
    await eng().call({ cmd: "save_note", args: { id: null, target: here, text: note } });
    note = "";
    say("Note saved");
  }
  const notes = $derived(app.personal?.notes.filter((n) => r && n.target.startsWith(`lbn:${r.pack}/${r.id}`)) ?? []);
</script>

{#if !rc}
  <p class="muted" aria-busy="true">Opening…</p>
{:else if !rc.valid || !r}
  <h1 tabindex="-1">This source cannot be opened</h1>
  <div class="notice bad" role="alert">
    <p>{rc.error}</p>
    <p class="small">Citation: <code>{uri}</code></p>
  </div>
  <p>If the pack was removed or is not installed on this device, install it from <a href="#/packs">Packs</a>.</p>
{:else}
  <h1 tabindex="-1">{r.title}</h1>
  <p class="small muted">
    {r.source?.publisher ?? ""}{#if r.source?.edition} · {r.source.edition}{/if} · Licence: {r.license.spdx}
    {#if r.license.attribution} — {r.license.attribution}{/if}
  </p>
  <div class="row" style="margin-bottom:0.5rem">
    <button class="secondary" onclick={bookmark} aria-pressed={!!bookmarked}>{bookmarked ? "★ Bookmarked" : "☆ Bookmark this place"}</button>
    <span class="small muted"><code>{here}</code></span>
  </div>
  {#if r.type === "html" || r.type === "article"}
    <HtmlView res={r} section={loc.kind === "section" ? loc.id : null} />
  {:else if r.type === "pdf"}
    <PdfView res={r} page={loc.kind === "page" ? loc.page : 1} onplace={(p) => (here = `lbn:${r.pack}/${r.id}#page=${p}`)} />
  {:else if r.type === "video"}
    <VideoView res={r} start={loc.kind === "time" ? loc.start : 0} end={loc.kind === "time" ? loc.end : null} onplace={(t) => (here = `lbn:${r.pack}/${r.id}#t=${t.toFixed(1)}`)} />
  {:else if r.type === "map"}
    <MapView res={r} at={loc.kind === "at" ? loc : null} onplace={(p) => (here = `lbn:${r.pack}/${r.id}#at=${p.lat.toFixed(5)},${p.lon.toFixed(5)},${p.zoom.toFixed(1)}`)} />
  {:else}
    <p>This item is a {r.type}. <a href="#/ai">Manage AI models</a>.</p>
  {/if}
  <section class="card" aria-label="Notes">
    <h2>Notes</h2>
    {#each notes as n (n.id)}<p class="small"><a href={`#/read?c=${encodeURIComponent(n.target)}`}>{n.target.split("#")[1] ?? "whole item"}</a>: {n.text}</p>{/each}
    <label for="note">Add a note about this place</label>
    <textarea id="note" rows="2" bind:value={note}></textarea>
    <button onclick={saveNote} disabled={!note.trim()}>Save note</button>
  </section>
{/if}
