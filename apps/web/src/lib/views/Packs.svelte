<script lang="ts">
  import { app, eng, fmtBytes, refreshPacks, refreshStorage, say } from "../state.svelte";
  import type { CatalogReply } from "../engine/gen/CatalogReply";
  import type { SourceInfo } from "../engine/gen/SourceInfo";
  import type { PackSource } from "../engine/gen/PackSource";
  import type { VerifyReport } from "../engine/gen/VerifyReport";

  const browser = app.hello?.platform === "browser";
  const params = new URLSearchParams(location.search);
  let catalogUrl = $state(params.get("catalog") ?? (browser ? new URL("packs/catalog.json", document.baseURI).href : ""));
  let catalog = $state<CatalogReply | null>(null);
  let catErr = $state("");
  let address = $state("");
  let pending = $state<{ source: PackSource; info: SourceInfo } | null>(null);
  let confirmUnsigned = $state(false);
  let confirmDowngrade = $state(false);
  let msg = $state("");
  let reports = $state<Record<string, VerifyReport>>({});
  let busy = $state<Record<string, boolean>>({});

  async function loadCatalog() {
    catErr = ""; catalog = null;
    try {
      catalog = await eng().call<CatalogReply>({ cmd: "read_catalog", args: { url: catalogUrl } });
    } catch (e: any) {
      catErr = `${e.message}. ${app.online ? "" : "You are offline: catalogs need a connection, but you can import pack files."}`;
    }
  }
  async function inspect(source: PackSource) {
    msg = ""; pending = null; confirmUnsigned = false; confirmDowngrade = false;
    try {
      pending = { source, info: await eng().call<SourceInfo>({ cmd: "inspect_source", args: { source } }) };
    } catch (e: any) {
      msg = `This pack cannot be used: ${e.message}`;
      say(msg);
    }
  }
  async function install() {
    if (!pending) return;
    try {
      await eng().call({ cmd: "install", args: { source: pending.source, allow_unsigned: confirmUnsigned, allow_downgrade: confirmDowngrade } });
      say(`Installing ${pending.info.title}`);
      pending = null;
      refreshPacks();
    } catch (e: any) {
      msg = e.message;
    }
  }
  async function fromAddress(e: Event) {
    e.preventDefault();
    const a = address.trim();
    if (!a) return;
    inspect(/^https?:\/\//.test(a) ? { type: "url", url: a } : { type: "local", path: a });
  }
  async function pick(kind: "file" | "folder") {
    const s = await eng().t.pickPackSource(kind);
    if (s) inspect(s);
  }
  async function job(cmd: "pause_job" | "resume_job" | "cancel_job", id: string) {
    try { await eng().call({ cmd, args: { job: id } } as any); } catch (e: any) { say(e.message); }
    refreshPacks();
  }
  async function verify(id: string) {
    busy[id] = true;
    try {
      reports[id] = await eng().call<VerifyReport>({ cmd: "verify_pack", args: { pack: id } });
      say(reports[id].ok ? "Every file checked: no damage found" : "Damage found");
    } finally { busy[id] = false; }
  }
  async function repair(id: string) {
    try {
      const r = await eng().call<any>({ cmd: "repair_pack", args: { pack: id } });
      say(r.job ? "Repair started" : "Nothing to repair");
    } catch (e: any) { say(e.message); msg = e.message; }
    refreshPacks();
  }
  async function remove(id: string, title: string, version: string | null = null) {
    if (!confirm(version ? `Remove the older version ${version} of “${title}”?` : `Remove “${title}” from this device? Its files will be deleted. Your notes and bookmarks are kept.`)) return;
    const r = await eng().call<{ freed_bytes: number }>({ cmd: "remove_pack", args: { pack: id, version } });
    say(`Removed; ${fmtBytes(r.freed_bytes)} freed`);
    refreshStorage();
  }
  async function rollback(id: string) {
    try {
      const b = await eng().call<any>({ cmd: "rollback_pack", args: { pack: id } });
      say(`Now using version ${b.version}`);
    } catch (e: any) { say(e.message); msg = e.message; }
  }
  async function keep() {
    const ok = await eng().call<boolean>({ cmd: "request_persistence" });
    say(ok ? "The browser agreed to keep this library unless you clear site data." : "The browser did not agree; it may clear the library when space runs low.");
    refreshStorage();
  }
  const jobs = $derived(app.jobs.filter((j) => j.state !== "done"));
</script>

<h1 tabindex="-1">Packs</h1>
<p>Packs are read-only bundles of documents, maps, videos, encyclopedias or AI models. Each file is checked against its
  published size and SHA-256 before a pack is used, and a working pack is never changed in place.</p>

{#if app.storage}
  <section class="card" aria-label="Storage">
    <h2>Storage on this {browser ? "browser" : "computer"}</h2>
    <p>Packs: {fmtBytes(app.storage.used_bytes)}{#if app.storage.free_bytes != null} · free: {fmtBytes(app.storage.free_bytes)}{/if}{#if app.storage.quota_bytes} · browser quota: {fmtBytes(app.storage.quota_bytes)}{/if}</p>
    {#if app.storage.location}<p class="small muted">Location: {app.storage.location}</p>{/if}
    {#if app.storage.evictable}
      <p class="small {app.storage.persisted ? 'ok' : 'warn'}">{app.storage.persisted
        ? "Persistent storage granted: the browser should not clear it on its own. Clearing site data still deletes it."
        : "Not persistent: the browser may clear this library when the device is low on space. Keep your .lbpack files and export your notes."}</p>
      {#if !app.storage.persisted}<button class="secondary" onclick={keep}>Ask the browser to keep my library</button>{/if}
    {/if}
  </section>
{/if}

{#if jobs.length}
  <section class="card" aria-label="Downloads and imports">
    <h2>Downloads and imports</h2>
    <ul class="plain">
      {#each jobs as j (j.job)}
        <li data-testid="job" data-state={j.state}>
          <strong>{j.title}</strong> {j.version} — <span>{j.state}</span>
          {#if j.bytes_total}<progress max={j.bytes_total} value={j.bytes_done} aria-label={`${j.title} progress`}></progress>
            <span class="small muted">{fmtBytes(j.bytes_done)} of {fmtBytes(j.bytes_total)} · file {j.files_done} of {j.files_total}{#if j.current_file} · {j.current_file}{/if}</span>{/if}
          {#if j.error}<p class="bad small">{j.error.message} ({j.error.code})</p>{/if}
          <div class="row">
            {#if ["downloading", "verifying", "queued"].includes(j.state)}<button class="secondary" onclick={() => job("pause_job", j.job)}>Pause</button>{/if}
            {#if ["paused", "failed"].includes(j.state)}<button onclick={() => job("resume_job", j.job)}>{j.state === "failed" ? "Retry" : "Resume"}</button>{/if}
            {#if j.state !== "activating"}<button class="secondary" onclick={() => job("cancel_job", j.job)}>Cancel</button>{/if}
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<section class="card" aria-label="Installed packs">
  <h2>Installed</h2>
  {#if !app.packs.length}<p>No packs installed yet.</p>{/if}
  <ul class="plain">
    {#each app.packs as p (p.installed.id)}
      <li data-testid="pack" data-pack={p.installed.id}>
        <strong>{p.installed.title}</strong> <span class="badge">{p.installed.kind}</span> <span class="badge">v{p.installed.version}</span>
        <span class="badge">{fmtBytes(p.installed.bytes)}</span>
        {#if p.health === "ok"}<span class="badge ok">ok</span>{:else}<span class="badge bad">damaged</span>{/if}
        <div class="small muted">{p.installed.signed_by ? `Signed by ${p.installed.signed_by}` : "Unsigned: you chose to trust its source"} · {p.license.spdx} · {p.publisher}</div>
        {#if p.health !== "ok"}<p class="bad small">Files are missing or damaged{browser ? " (the browser may have cleared storage)" : ""}. Repair it, or install it again.</p>{/if}
        {#if reports[p.installed.id]}
          {@const r = reports[p.installed.id]}
          <p class="small {r.ok ? 'ok' : 'bad'}">{r.ok ? `Checked ${r.checked} files: all match.` : `Damaged: ${[...r.damaged, ...r.missing].join(", ")}`}</p>
        {/if}
        <div class="row">
          <button class="secondary" onclick={() => verify(p.installed.id)} disabled={busy[p.installed.id]}>{busy[p.installed.id] ? "Checking…" : "Check files"}</button>
          <button class="secondary" onclick={() => repair(p.installed.id)}>Repair</button>
          {#each p.other_versions as v}<button class="secondary" onclick={() => rollback(p.installed.id)}>Go back to {v}</button>
            <button class="secondary" onclick={() => remove(p.installed.id, p.installed.title, v)}>Delete {v}</button>{/each}
          <button class="danger" onclick={() => remove(p.installed.id, p.installed.title)}>Remove</button>
        </div>
      </li>
    {/each}
  </ul>
</section>

<section class="card" aria-label="Get packs">
  <h2>Get packs from a catalog</h2>
  <form class="row" onsubmit={(e) => { e.preventDefault(); loadCatalog(); }}>
    <label for="cat-url" class="sr-only">Catalog address</label>
    <input id="cat-url" type="url" bind:value={catalogUrl} placeholder="https://…/catalog.json" />
    <button type="submit" disabled={!catalogUrl}>Show packs</button>
  </form>
  {#if catErr}<p class="bad small" role="alert">{catErr}</p>{/if}
  {#if catalog}
    <p class="small muted">{catalog.title} · published {catalog.published} · {catalog.signed_by ? `signed by ${catalog.signed_by}` : "unsigned catalog"}
      {#if catalog.expired}<span class="warn"> · this list is out of date (expired {catalog.expires}); installed packs are not affected</span>{/if}</p>
    <ul class="plain">
      {#each catalog.packs as cp}
        <li data-testid="catalog-pack" data-pack={cp.entry.id}>
          <strong>{cp.entry.title}</strong> <span class="badge">{cp.entry.kind}</span> <span class="badge">{fmtBytes(cp.entry.bytes)}</span> <span class="badge">{cp.entry.license}</span>
          <div class="small">{cp.entry.description}</div>
          {#if cp.installed_version === cp.entry.version}<span class="ok small">Installed</span>
          {:else}<button onclick={() => inspect({ type: "url", url: cp.manifest_url })}>{cp.installed_version ? `Update from ${cp.installed_version}` : "Choose"}</button>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section class="card" aria-label="Import">
  <h2>Import without internet</h2>
  <p>Copy a <code>.lbpack</code> file from a USB drive or another device and open it here.</p>
  <div class="row">
    <button onclick={() => pick("file")}>Open a pack file…</button>
    {#if !browser}<button class="secondary" onclick={() => pick("folder")}>Import an old Lastbook library folder…</button>{/if}
  </div>
  <form class="row" style="margin-top:0.75rem" onsubmit={fromAddress}>
    <label for="addr" class="sr-only">Pack address{browser ? "" : " or path"}</label>
    <input id="addr" type="text" bind:value={address} placeholder={browser ? "https://…/pack.json" : "https://…/pack.json, /path/to/file.lbpack, or a pack folder"} />
    <button type="submit" class="secondary">Check</button>
  </form>
  {#if msg}<p class="bad small" role="alert">{msg}</p>{/if}
</section>

{#if pending}
  {@const i = pending.info}
  <section class="card" aria-label="Confirm install" data-testid="confirm">
    <h2>Install “{i.title}”?</h2>
    <p>{i.description}</p>
    <p class="small">Version {i.version}{#if i.installed_version} (installed: {i.installed_version}){/if} · {i.resources} items · {fmtBytes(i.bytes)} · licence {i.license.spdx}{#if i.license.attribution} — {i.license.attribution}{/if}</p>
    {#if i.model}<p class="small">AI model: {i.model.name}, {i.model.quantization}, needs about {fmtBytes(i.model.memory_bytes)} of memory.</p>{/if}
    {#if i.signed_by}<p class="ok small">Signed by a trusted publisher: {i.signed_by}.</p>
    {:else}
      <p class="warn">This pack is not signed by a trusted publisher. Its files will still be checked against its own manifest, but nobody vouches for where it came from.</p>
      <label><input type="checkbox" bind:checked={confirmUnsigned} /> I trust where this pack came from</label>
    {/if}
    {#if i.unsupported.length}
      <details open><summary>{i.unsupported.length} item(s) will not be imported (they stay in the original folder)</summary>
        <ul class="small">{#each i.unsupported as u}<li>{u.title}: {u.reason}</li>{/each}</ul></details>
    {/if}
    {#if i.installed_version && i.installed_version !== i.version}
      <label><input type="checkbox" bind:checked={confirmDowngrade} /> Allow this to replace version {i.installed_version} even if it is older (roll back)</label>
    {/if}
    <div class="row">
      <button onclick={install} disabled={!i.signed_by && !confirmUnsigned}>Install</button>
      <button class="secondary" onclick={() => (pending = null)}>Cancel</button>
    </div>
  </section>
{/if}
