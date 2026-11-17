<script lang="ts">
  import { app, eng, say, refreshPersonal } from "../state.svelte";
  import { readHref } from "../router.svelte";
  let msg = $state("");
  async function exp() {
    const json = await eng().call<string>({ cmd: "export_personal" });
    const ok = await eng().t.saveTextFile(`lastbook-personal-${new Date().toISOString().slice(0, 10)}.json`, json);
    say(ok ? "Exported" : "Export cancelled");
  }
  async function imp() {
    const text = await eng().t.pickTextFile();
    if (!text) return;
    try {
      await eng().call({ cmd: "import_personal", args: { data: text } });
      msg = "Imported and merged. A backup of your previous data was kept.";
    } catch (e: any) {
      msg = `Not imported: ${e.message}`;
    }
    say(msg);
    refreshPersonal();
  }
  const p = $derived(app.personal);
</script>

<h1 tabindex="-1">Saved</h1>
<section class="card">
  <h2>Your data</h2>
  <p>Bookmarks, notes, history, saved answers and settings are kept apart from the packs, on this device only.
    Export them to keep a copy or move them to another device.</p>
  <div class="row"><button onclick={exp}>Export to a file…</button><button class="secondary" onclick={imp}>Import from a file…</button></div>
  {#if msg}<p class="small" role="status">{msg}</p>{/if}
</section>
{#if p}
  <section class="card"><h2>Bookmarks</h2>
    {#if !p.bookmarks.length}<p class="muted">None yet. Use “Bookmark this place” while reading.</p>{/if}
    <ul class="plain">{#each p.bookmarks as b (b.id)}<li><a href={readHref(b.target)}>{b.title}</a>
      <button class="secondary" onclick={() => eng().call({ cmd: "delete_bookmark", args: { id: b.id } })} aria-label={`Delete bookmark ${b.title}`}>Delete</button></li>{/each}</ul>
  </section>
  <section class="card"><h2>Notes</h2>
    {#if !p.notes.length}<p class="muted">None yet.</p>{/if}
    <ul class="plain">{#each p.notes as n (n.id)}<li><a href={readHref(n.target)}><code>{n.target}</code></a><p>{n.text}</p>
      <button class="secondary" onclick={() => eng().call({ cmd: "delete_note", args: { id: n.id } })}>Delete note</button></li>{/each}</ul>
  </section>
  <section class="card"><h2>Saved answers</h2>
    {#if !p.answers.length}<p class="muted">None yet.</p>{/if}
    <ul class="plain">{#each p.answers as a (a.id)}<li><strong>{a.question}</strong><p class="small">{a.answer}</p>
      <p class="small">{#each a.citations as c}<a href={readHref(c)}>{c}</a> {/each}</p></li>{/each}</ul>
  </section>
  <section class="card"><h2>History</h2>
    <ul class="plain">{#each [...p.history].reverse().slice(0, 50) as h}<li><a href={readHref(h.target)}>{h.title}</a> <span class="small muted">{h.at}</span></li>{/each}</ul>
  </section>
{/if}
