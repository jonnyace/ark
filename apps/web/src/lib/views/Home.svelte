<script lang="ts">
  import { app, fmtBytes, eng, refreshStorage, say } from "../state.svelte";
  import { go, readHref } from "../router.svelte";
  let q = $state("");
  const damaged = $derived(app.packs.filter((p) => p.health !== "ok"));
  const contentPacks = $derived(app.packs.filter((p) => p.installed.kind !== "model"));
  const active = $derived(app.jobs.filter((j) => !["done", "cancelled"].includes(j.state)));
  async function keep() {
    const ok = await eng().call<boolean>({ cmd: "request_persistence" });
    say(ok ? "The browser agreed to keep this library unless you clear it yourself." : "The browser did not agree to keep this library permanently.");
    refreshStorage();
  }
</script>

<h1 tabindex="-1">Your offline library</h1>

<form class="row card" role="search" onsubmit={(e) => { e.preventDefault(); if (q.trim()) go("search", { q }); }}>
  <label class="sr-only" for="home-q">Search the library</label>
  <input id="home-q" type="search" bind:value={q} placeholder="Search documents, videos, maps and articles" />
  <button type="submit">Search</button>
</form>

{#if contentPacks.length === 0}
  <section class="card">
    <h2>Get started</h2>
    <p>Nothing is installed on this {app.hello?.platform === "browser" ? "browser" : "computer"} yet. Choose packs of documents, maps, videos or
      encyclopedias, or import a pack file you already have. Once installed, everything works without internet.</p>
    <a class="button" href="#/packs">Choose or import packs</a>
  </section>
{/if}

{#if damaged.length}
  <div class="notice bad" role="alert">
    <strong>Some packs are damaged or incomplete:</strong> {damaged.map((p) => p.installed.title).join(", ")}.
    {#if app.hello?.capabilities.evictable_storage}The browser may have removed stored files.{/if}
    <a href="#/packs">Repair or reinstall</a>
  </div>
{/if}

{#if active.length}
  <section class="card" aria-label="Downloads">
    <h2>In progress</h2>
    {#each active as j (j.job)}
      <p>{j.title}: <strong>{j.state}</strong> {#if j.bytes_total}— {fmtBytes(j.bytes_done)} of {fmtBytes(j.bytes_total)}{/if}
        {#if j.error}<span class="bad">— {j.error.message}</span>{/if}</p>
    {/each}
    <a href="#/packs">Manage downloads</a>
  </section>
{/if}

<div class="grid">
  <section class="card">
    <h2>Library</h2>
    <p>{contentPacks.length} pack{contentPacks.length === 1 ? "" : "s"} with {contentPacks.reduce((n, p) => n + p.resource_count, 0)} items.</p>
    <p><a href="#/library">Browse everything</a> · <a href="#/packs">Manage packs</a></p>
  </section>
  <section class="card">
    <h2>Storage</h2>
    {#if app.storage}
      <p>Packs use {fmtBytes(app.storage.used_bytes)}{#if app.storage.free_bytes != null}; {fmtBytes(app.storage.free_bytes)} free{/if}.</p>
      {#if app.storage.evictable}
        {#if app.storage.persisted}
          <p class="ok small">The browser has agreed to keep this library unless you clear site data. Keep a copy of your pack files and export your notes anyway.</p>
        {:else}
          <p class="warn small">Browser storage can be cleared by the browser when space runs low. This is not guaranteed permanent storage.</p>
          <button class="secondary" onclick={keep}>Ask the browser to keep my library</button>
        {/if}
      {/if}
    {/if}
  </section>
  <section class="card">
    <h2>Local AI</h2>
    {#if !app.ai}
      <p class="muted">Checking…</p>
    {:else if !app.ai.runtime}
      <p>Not available on this device. {app.ai.unavailable_reason}</p>
    {:else if !app.ai.models.length}
      <p>Optional. No model installed. Reading, search, maps and video work without it.</p>
      <a href="#/ai">Set up local AI</a>
    {:else if !app.ai.enabled}
      <p>A model is installed but AI is turned off.</p><a href="#/ai">Turn on</a>
    {:else}
      <p>Ready. Answers come only from your installed library, with sources.</p><a class="button" href="#/ask">Ask a question</a>
    {/if}
  </section>
</div>

{#if app.personal?.history.length}
  <section class="card">
    <h2>Recently opened</h2>
    <ul class="plain">
      {#each [...app.personal.history].reverse().filter((h, i, a) => a.findIndex((x) => x.target === h.target) === i).slice(0, 5) as h}
        <li><a href={readHref(h.target)}>{h.title}</a></li>
      {/each}
    </ul>
  </section>
{/if}
