<script lang="ts">
  import { eng } from "../state.svelte";
  import { route, go, readHref } from "../router.svelte";
  import type { SearchReply } from "../engine/gen/SearchReply";
  let q = $state(route.params.get("q") ?? "");
  let res = $state<SearchReply | null>(null);
  let busy = $state(false);
  let err = $state("");
  $effect(() => {
    const query = route.params.get("q") ?? "";
    q = query;
    if (!query.trim()) { res = null; return; }
    busy = true;
    err = "";
    eng().call<SearchReply>({ cmd: "search", args: { query, limit: 30, packs: null } })
      .then((r) => (res = r)).catch((e) => (err = e.message)).finally(() => (busy = false));
  });
</script>

<h1 tabindex="-1">Search</h1>
<form class="row card" role="search" onsubmit={(e) => { e.preventDefault(); go("search", { q }); }}>
  <label class="sr-only" for="search-q">Search the library</label>
  <input id="search-q" type="search" bind:value={q} placeholder="What do you need?" />
  <button type="submit">Search</button>
</form>
<div aria-live="polite" aria-busy={busy}>
  {#if err}<p class="bad">{err}</p>{/if}
  {#if res}
    <p class="muted small">{res.hits.length} result{res.hits.length === 1 ? "" : "s"}{#if res.skipped.length} · could not search: {res.skipped.join(", ")} (damaged){/if}</p>
    {#if !res.hits.length}<p>No matches in the installed library. Try other words, or <a href="#/packs">add packs</a>.</p>{/if}
    <ol class="plain">
      {#each res.hits as h}
        <li>
          <a href={readHref(h.uri)}><strong>{h.title}</strong></a>
          <div class="small">{h.snippet}</div>
          <div class="small muted"><code>{h.uri}</code></div>
        </li>
      {/each}
    </ol>
  {/if}
</div>
