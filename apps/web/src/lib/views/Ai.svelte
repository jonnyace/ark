<script lang="ts">
  import { app, eng, refreshAi, say, fmtBytes } from "../state.svelte";
  let busy = $state("");
  let err = $state("");
  async function setEnabled(on: boolean) {
    const s = { ...app.personal!.settings, ai_enabled: on };
    await eng().call({ cmd: "save_settings", args: { settings: s } });
    say(on ? "Local AI turned on" : "Local AI turned off and unloaded");
    refreshAi();
  }
  async function choose(pack: string) {
    await eng().call({ cmd: "save_settings", args: { settings: { ...app.personal!.settings, model: pack } } });
    say("Model chosen");
  }
  async function load(pack: string) {
    busy = pack; err = "";
    try { await eng().call({ cmd: "load_model", args: { pack } }); say("Model loaded"); }
    catch (e: any) { err = e.message; say("The model could not be loaded"); }
    finally { busy = ""; refreshAi(); }
  }
  async function unload() {
    await eng().call({ cmd: "unload_model" });
    say("Model unloaded; its memory is free again");
    refreshAi();
  }
  $effect(() => { refreshAi(); });
</script>

<h1 tabindex="-1">Local AI</h1>
<p>Optional. A language model on this {app.hello?.platform === "browser" ? "device" : "computer"} answers questions from your installed packs and links each claim to its source.
  Nothing is sent anywhere. Without it, reading, search, maps and video work exactly the same.</p>

{#if !app.ai}<p class="muted" aria-busy="true">Checking…</p>
{:else}
  <section class="card">
    <h2>This device</h2>
    {#if app.ai.runtime}
      <p class="ok">Runtime available: {app.ai.runtime_name}.</p>
    {:else}
      <p class="warn" data-testid="ai-unavailable">Not available: {app.ai.unavailable_reason}</p>
    {/if}
    {#if app.personal}
      <label><input type="checkbox" checked={app.ai.enabled} onchange={(e) => setEnabled((e.target as HTMLInputElement).checked)} /> Use local AI</label>
    {/if}
  </section>

  <section class="card">
    <h2>Models</h2>
    {#if !app.ai.models.length}
      <p>No model installed. Install an AI pack (for example “Qwen2.5 1.5B Instruct”) from <a href="#/packs">Packs</a>.</p>
    {/if}
    {#if err}<p class="bad" role="alert">{err}</p>{/if}
    <ul class="plain">
      {#each app.ai.models as m (m.pack)}
        <li>
          <strong>{m.title}</strong>
          {#if app.ai.loaded === m.pack}<span class="badge ok">loaded</span>{/if}
          {#if app.personal?.settings.model === m.pack}<span class="badge">default</span>{/if}
          <p class="small {m.fits.ok ? 'ok' : 'warn'}">{m.fits.reason}</p>
          <details>
            <summary>Model details</summary>
            <dl class="small">
              <dt>Weights</dt><dd>{m.info.upstream}</dd>
              <dt>Runtime</dt><dd>{m.info.runtime} (at least {m.info.min_runtime}), format {m.info.format}, quantization {m.info.quantization}</dd>
              <dt>Tokenizer</dt><dd>{m.info.tokenizer}</dd>
              <dt>Prompt template</dt><dd>{m.info.template}</dd>
              <dt>Context</dt><dd>{m.info.context_length} tokens</dd>
              <dt>Generation</dt><dd>temperature {m.info.defaults.temperature}, top-k {m.info.defaults.top_k}, top-p {m.info.defaults.top_p}, min-p {m.info.defaults.min_p}, up to {m.info.defaults.max_tokens} tokens, seed {m.info.defaults.seed}</dd>
              <dt>Memory</dt><dd>about {fmtBytes(m.info.memory_bytes)} while answering; at least {fmtBytes(m.info.min_ram_bytes)} RAM recommended</dd>
            </dl>
          </details>
          <div class="row">
            {#if app.ai.loaded === m.pack}<button class="secondary" onclick={unload}>Unload (free memory)</button>
            {:else}<button class="secondary" onclick={() => load(m.pack)} disabled={!app.ai.runtime || !!busy}>{busy === m.pack ? "Loading…" : "Load now"}</button>{/if}
            {#if app.personal?.settings.model !== m.pack}<button class="secondary" onclick={() => choose(m.pack)}>Use by default</button>{/if}
            <a class="button secondary" href="#/packs">Remove…</a>
          </div>
        </li>
      {/each}
    </ul>
  </section>
  {#if app.ai.enabled && app.ai.models.length}<p><a class="button" href="#/ask">Ask a question</a></p>{/if}
{/if}
