<script lang="ts">
  import { onMount, tick } from "svelte";
  import { app, boot } from "./lib/state.svelte";
  import { route } from "./lib/router.svelte";
  import Home from "./lib/views/Home.svelte";
  import Library from "./lib/views/Library.svelte";
  import Search from "./lib/views/Search.svelte";
  import Read from "./lib/views/Read.svelte";
  import Ask from "./lib/views/Ask.svelte";
  import Packs from "./lib/views/Packs.svelte";
  import Ai from "./lib/views/Ai.svelte";
  import Saved from "./lib/views/Saved.svelte";
  import About from "./lib/views/About.svelte";

  const nav = [
    ["home", "Home"], ["library", "Library"], ["search", "Search"], ["ask", "Ask"],
    ["packs", "Packs"], ["ai", "AI"], ["saved", "Saved"], ["about", "About"],
  ];
  let main: HTMLElement;
  onMount(boot);
  $effect(() => {
    route.view;
    tick().then(() => main?.querySelector<HTMLElement>("h1")?.focus());
  });
</script>

<a class="skip" href="#main" onclick={(e) => { e.preventDefault(); main.focus(); }}>Skip to content</a>
<header class="top">
  <a class="brand" href="#/">Lastbook <span>Next</span></a>
  <span class="net" class:off={!app.online} role="status">
    {app.online ? "Online" : "Offline"} · works offline
  </span>
  <nav aria-label="Main">
    <ul>
      {#each nav as [id, label]}
        <li><a href={`#/${id}`} aria-current={route.view === id ? "page" : undefined}>{label}</a></li>
      {/each}
    </ul>
  </nav>
</header>

<main id="main" tabindex="-1" bind:this={main}>
  {#if app.error}
    <section class="card error" role="alert">
      <h1 tabindex="-1">The library engine could not start</h1>
      <p>{app.error}</p>
      <p>If this is a browser, it may not support the storage this app needs (Origin Private File System). Try a current
        Chrome, Edge, Firefox or Safari, or the desktop app.</p>
    </section>
  {:else if !app.ready}
    <p class="muted" aria-busy="true">Opening your library…</p>
  {:else if route.view === "home"}<Home />
  {:else if route.view === "library"}<Library />
  {:else if route.view === "search"}<Search />
  {:else if route.view === "read"}<Read />
  {:else if route.view === "ask"}<Ask />
  {:else if route.view === "packs"}<Packs />
  {:else if route.view === "ai"}<Ai />
  {:else if route.view === "saved"}<Saved />
  {:else if route.view === "about"}<About />
  {:else}<h1 tabindex="-1">Not found</h1><p><a href="#/">Go home</a></p>
  {/if}
</main>

<div class="sr-only" aria-live="polite" aria-atomic="true">{app.announce}</div>
