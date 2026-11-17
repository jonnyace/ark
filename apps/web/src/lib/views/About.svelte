<script lang="ts">
  import { app } from "../state.svelte";
  let build = $state<{ version: string } | null>(null);
  fetch("./build-info.json").then((r) => r.json()).then((b) => (build = b)).catch(() => {});
</script>

<h1 tabindex="-1">About</h1>
<section class="card">
  <p><strong>Ark</strong> {app.hello?.engine_version} · engine API {app.hello?.api_version} · {app.hello?.platform}{#if build} · app build {build.version}{/if}</p>
  <p>An open-source offline library. After you install packs, it needs no internet, account, server or subscription. It
    sends nothing anywhere. Downloads happen only when you choose them.</p>
  <p>Source code: MIT licence. Builds on <a href="https://github.com/Immortal-Protocols/lastbook">Lastbook</a> (MIT), llama.cpp (MIT),
    pdf.js (Apache-2.0), MapLibre GL JS (BSD-3-Clause), PMTiles (BSD-3-Clause), wllama (MIT), Svelte (MIT) and Tauri (MIT/Apache-2.0).
    Library content and models keep their own licences, shown with each pack.</p>
</section>
<section class="card">
  <h2>Content licences</h2>
  <ul class="plain">
    {#each app.packs as p}<li><strong>{p.installed.title}</strong>: {p.license.spdx}{#if p.license.attribution} — {p.license.attribution}{/if}</li>{/each}
  </ul>
</section>
<section class="card">
  <h2>Limits</h2>
  <ul>
    <li>Generated answers can be wrong. They only use your installed packs; always check the cited source.</li>
    {#if app.hello?.capabilities.evictable_storage}<li>In a browser, storage can be cleared by the browser or by clearing site data. Keep your pack files and export your notes.</li>{/if}
  </ul>
</section>
