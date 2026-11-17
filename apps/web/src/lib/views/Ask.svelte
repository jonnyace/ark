<script lang="ts">
  import { onDestroy } from "svelte";
  import { app, eng, say } from "../state.svelte";
  import { readHref } from "../router.svelte";
  import type { AskResult } from "../engine/gen/AskResult";
  let q = $state("");
  let ask = $state<string | null>(null);
  let status = $state("");
  let stream = $state("");
  let result = $state<AskResult | null>(null);
  let err = $state("");
  const off = eng().on((e) => {
    if (e.event === "ask_status" && e.data.ask === ask) status = e.data.status;
    if (e.event === "ask_token" && e.data.ask === ask) stream += e.data.text;
    if (e.event === "ask_done" && e.data.ask === ask) { result = e.data; ask = null; say(e.data.outcome === "answered" ? "Answer ready" : e.data.outcome === "no_evidence" ? "The library does not cover this" : "Stopped"); }
    if (e.event === "ask_error" && e.data.ask === ask) { err = `${e.data.message}`; ask = null; say("The answer failed"); }
  });
  onDestroy(off);
  async function submit(e: Event) {
    e.preventDefault();
    if (!q.trim() || ask) return;
    stream = ""; result = null; err = ""; status = "starting";
    try {
      ask = (await eng().call<{ ask: string }>({ cmd: "ask", args: { question: q, model: null } })).ask;
    } catch (x: any) {
      err = x.message;
    }
  }
  async function cancel() {
    if (ask) await eng().call({ cmd: "cancel_ask", args: { ask } });
  }
  const statusText: Record<string, string> = { starting: "Starting…", searching: "Searching your library…", loading_model: "Loading the model into memory (this can take a while the first time)…", generating: "Writing an answer from the sources…" };
  // Split the answer into text and citation keys so each [S1] becomes a link to its exact source.
  function parts(text: string) {
    return text.split(/(\[S\d+\])/).map((p) => ({ p, key: /^\[S\d+\]$/.test(p) ? p.slice(1, -1) : null }));
  }
  const ready = $derived(app.ai?.runtime && app.ai.enabled && app.ai.models.length > 0);
</script>

<h1 tabindex="-1">Ask your library</h1>
{#if !app.ai}
  <p class="muted">Checking local AI…</p>
{:else if !ready}
  <div class="notice">
    {#if !app.ai.runtime}<p>Local AI is not available here: {app.ai.unavailable_reason}</p>
    {:else if !app.ai.models.length}<p>Local AI is optional and no model is installed. <a href="#/ai">Set up local AI</a>.</p>
    {:else}<p>Local AI is turned off. <a href="#/ai">Turn it on</a>.</p>{/if}
    <p>You can still <a href="#/search">search the library</a>: it finds the same sources.</p>
  </div>
{:else}
  <p class="small muted">Answers are written on this {app.hello?.platform === "browser" ? "device, in this browser" : "computer"}, only from your installed packs, with links to the exact page, section or moment. They can still be wrong: check the sources, and get trained help in an emergency.</p>
  <form onsubmit={submit} class="card">
    <label for="ask-q">Your question</label>
    <textarea id="ask-q" rows="2" bind:value={q} maxlength="2000"></textarea>
    <div class="row" style="margin-top:0.5rem">
      <button type="submit" disabled={!!ask || !q.trim()}>Ask</button>
      {#if ask}<button type="button" class="secondary" onclick={cancel}>Stop</button>{/if}
    </div>
  </form>
{/if}

<div aria-live="polite" aria-busy={!!ask}>
  {#if ask}<p class="muted">{statusText[status] ?? status}</p>{/if}
  {#if err}<div class="notice bad" role="alert"><p>{err}</p></div>{/if}
  {#if ask && stream}<div class="card"><p style="white-space:pre-wrap">{stream}</p></div>{/if}
</div>

{#if result}
  <section class="card" data-testid="answer" data-outcome={result.outcome}>
    {#if result.outcome === "no_evidence"}
      <h2>Not found in your library</h2>
      <p>Nothing installed answers this clearly, so no answer was generated. Closest matches:</p>
    {:else}
      <h2>Answer</h2>
      {#if result.answer}
        <p style="white-space:pre-wrap" data-testid="answer-text">{#each parts(result.answer.text) as x}{#if x.key}{@const s = result.sources.find((s) => s.key === x.key)}<a class="cite" href={s ? readHref(s.citation) : undefined} title={s?.title} data-testid="cite">[{x.key}]</a>{:else}{x.p}{/if}{/each}</p>
        {#if result.outcome === "cancelled"}<p class="warn small">Stopped before the end.</p>{/if}
        {#if result.answer.not_found}<p class="small">The model found that the sources do not answer this.</p>{/if}
        {#if result.answer.uncited}<p class="warn small">The model did not mark which source each sentence came from. Check the sources below before relying on it.</p>{/if}
        {#if result.answer.unknown_keys.length}<p class="warn small">Removed references to sources it was not given: {result.answer.unknown_keys.join(", ")}.</p>{/if}
      {/if}
      <h2>Sources it was given</h2>
    {/if}
    <ol class="plain">
      {#each result.sources as s}
        <li><strong>[{s.key}]</strong> <a href={readHref(s.citation)} data-testid="source-link">{s.title}</a>
          <div class="small muted">{s.excerpt}</div></li>
      {/each}
    </ol>
    {#if result.stats}<p class="small muted">{result.model} · {result.stats.prompt_tokens} prompt tokens in {(result.stats.prompt_ms / 1000).toFixed(1)} s · {result.stats.generated_tokens} tokens written in {(result.stats.gen_ms / 1000).toFixed(1)} s</p>{/if}
  </section>
{/if}
