<script lang="ts">
  // Video from local storage with its WebVTT captions. The transcript list comes from the browser's own text-track
  // parser; each line is a button that seeks to its moment, so timestamp citations open at the exact point.
  import { eng } from "../state.svelte";
  import type { ResourceInfo } from "../engine/gen/ResourceInfo";
  let { res, start = 0, end = null, onplace }: { res: ResourceInfo; start?: number; end?: number | null; onplace: (t: number) => void } = $props();
  let video = $state<HTMLVideoElement>()!;
  let src = $state("");
  let vtt = $state("");
  let poster = $state("");
  let cues = $state<{ start: number; end: number; text: string }[]>([]);
  let now = $state(0);
  let err = $state("");
  const fmt = (t: number) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;
  $effect(() => {
    const r = res;
    (async () => {
      const t = eng().t;
      src = await t.fileUrl(r.pack, r.path);
      if (r.transcript) vtt = await t.fileUrl(r.pack, r.transcript);
      if (r.poster) poster = await t.fileUrl(r.pack, r.poster);
    })().catch((e) => (err = `This video could not be opened: ${e.message}`));
  });
  $effect(() => {
    if (video && video.readyState >= 1) seek(start);
  });
  function seek(t: number) {
    if (!video) return;
    video.currentTime = Math.max(0, t);
  }
  function loaded() {
    seek(start);
  }
  function trackLoaded(e: Event) {
    const tt = (e.target as HTMLTrackElement).track;
    cues = Array.from(tt.cues ?? []).map((c: any) => ({ start: c.startTime, end: c.endTime, text: c.text.replace(/<[^>]+>/g, "") }));
  }
</script>

{#if err}<p class="bad" role="alert">{err}</p>{/if}
{#if src}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={video} {src} poster={poster || undefined} controls preload="metadata" crossorigin="anonymous" data-testid="video"
    onloadedmetadata={loaded} ontimeupdate={() => (now = video.currentTime)} onpause={() => onplace(video.currentTime)} onseeked={() => onplace(video.currentTime)}
    style="width:100%;max-height:70vh;background:#000;border-radius:8px">
    {#if vtt}<track kind="captions" srclang="en" label="English" src={vtt} default onload={trackLoaded} />{/if}
    Your browser cannot play this video format (WebM VP9/Opus).
  </video>
  <p class="small muted" data-testid="video-time" aria-live="off">At {fmt(now)} of {fmt(res.duration_s ?? 0)}{#if start}. Opened at the cited moment {fmt(start)}{end != null ? `–${fmt(end)}` : ""}.{/if}</p>
{/if}
{#if cues.length}
  <section aria-label="Transcript">
    <h2>Transcript</h2>
    <ol class="plain">
      {#each cues as c}
        <li style={now >= c.start && now < c.end ? "background:var(--card);font-weight:600" : ""}>
          <button class="secondary" onclick={() => { seek(c.start); video.play().catch(() => {}); }} aria-label={`Play from ${fmt(c.start)}`}>{fmt(c.start)}</button>
          {c.text}
        </li>
      {/each}
    </ol>
  </section>
{/if}
