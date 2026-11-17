<script lang="ts">
  import { eng, app } from "../state.svelte";
  import { readHref } from "../router.svelte";
  import type { ResourceInfo } from "../engine/gen/ResourceInfo";
  let items = $state<ResourceInfo[]>([]);
  let filter = $state("all");
  let text = $state("");
  const kinds: Record<string, string> = { html: "Documents", pdf: "PDFs", article: "Encyclopedia", video: "Videos", map: "Maps" };
  $effect(() => {
    app.packs;
    eng().call<ResourceInfo[]>({ cmd: "list_resources", args: { pack: null } }).then((r) => (items = r.filter((x) => x.type !== "model")));
  });
  const shown = $derived(items.filter((r) => (filter === "all" || r.type === filter) && (!text || r.title.toLowerCase().includes(text.toLowerCase()))));
</script>

<h1 tabindex="-1">Library</h1>
<div class="row card">
  <label for="lib-kind" class="sr-only">Kind</label>
  <select id="lib-kind" bind:value={filter}>
    <option value="all">Everything</option>
    {#each Object.entries(kinds) as [k, v]}<option value={k}>{v}</option>{/each}
  </select>
  <label for="lib-filter" class="sr-only">Filter by title</label>
  <input id="lib-filter" type="text" bind:value={text} placeholder="Filter by title" />
</div>
{#if !items.length}
  <p>Nothing installed yet. <a href="#/packs">Get packs</a>.</p>
{:else}
  <p class="muted small" aria-live="polite">{shown.length} item{shown.length === 1 ? "" : "s"}</p>
  <ul class="plain">
    {#each shown as r (r.pack + r.id)}
      <li>
        <a href={readHref(`lbn:${r.pack}/${r.id}`)}><strong>{r.title}</strong></a>
        <span class="badge">{kinds[r.type] ?? r.type}</span>
        {#if r.pages}<span class="badge">{r.pages} pages</span>{/if}
        {#if r.duration_s}<span class="badge">{Math.round(r.duration_s)} s</span>{/if}
        {#if r.summary}<div class="small muted">{r.summary}</div>{/if}
      </li>
    {/each}
  </ul>
{/if}
