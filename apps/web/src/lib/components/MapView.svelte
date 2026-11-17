<script lang="ts">
  // Offline vector map: MapLibre GL draws tiles from the pack's PMTiles file through a byte-range reader, with the
  // pack's own style, glyphs and sprites. Every URL in the style is rewritten to a pack file; nothing is fetched online.
  import { onDestroy } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import "maplibre-gl/dist/maplibre-gl.css";
  import { Protocol, PMTiles } from "pmtiles";
  import mapWorkerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url";
  import { eng } from "../state.svelte";
  import type { ResourceInfo } from "../engine/gen/ResourceInfo";
  let { res, at = null, onplace }: { res: ResourceInfo; at?: { lat: number; lon: number; zoom: number } | null; onplace: (p: { lat: number; lon: number; zoom: number }) => void } = $props();
  let el: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let err = $state("");
  let pos = $state("");

  const g = globalThis as any;
  if (!g.__lbnMapProtocols) {
    maplibregl.setWorkerUrl(mapWorkerUrl); // bundled with the app, never loaded from elsewhere
    const protocol = new Protocol();
    maplibregl.addProtocol("pmtiles", protocol.tile);
    maplibregl.addProtocol("lbnpack", async (params: any) => {
      const u = params.url.slice("lbnpack://".length);
      const i = u.indexOf("/");
      const pack = decodeURIComponent(u.slice(0, i));
      const path = u.slice(i + 1).split("/").map(decodeURIComponent).join("/");
      try {
        const buf = await eng().t.readFile(pack, path);
        if (params.type === "json") return { data: JSON.parse(new TextDecoder().decode(buf)) };
        return { data: buf };
      } catch {
        if (path.endsWith(".pbf")) return { data: new ArrayBuffer(0) }; // glyph range not shipped: draw without it
        throw new Error(`map file not in pack: ${path}`);
      }
    });
    g.__lbnMapProtocols = protocol;
  }

  $effect(() => {
    const r = res;
    let cancelled = false;
    (async () => {
      try {
        const t = eng().t;
        const mi = r.map!;
        const style = JSON.parse(new TextDecoder().decode(await t.readFile(r.pack, mi.style)));
        const key = `${r.pack}/${r.path}`;
        const source = {
          getKey: () => key,
          getBytes: async (offset: number, length: number) => ({ data: await t.readRange(r.pack, r.path, offset, length) }),
        };
        g.__lbnMapProtocols.add(new PMTiles(source as any));
        const packUrl = (p: string) => `lbnpack://${encodeURIComponent(r.pack)}/${p}`;
        for (const s of Object.values<any>(style.sources)) {
          if (typeof s.url === "string" && s.url.startsWith("pmtiles://")) s.url = `pmtiles://${key}`;
        }
        if (style.glyphs) style.glyphs = packUrl(style.glyphs);
        if (typeof style.sprite === "string") style.sprite = packUrl(style.sprite);
        if (cancelled) return;
        const [w, s, e, n] = mi.bounds;
        map = new maplibregl.Map({
          container: el, style, attributionControl: { compact: false },
          center: at ? [at.lon, at.lat] : [(w + e) / 2, (s + n) / 2], zoom: at ? at.zoom : 13,
          maxBounds: [[w - 0.2, s - 0.2], [e + 0.2, n + 0.2]], minZoom: mi.minzoom, maxZoom: Math.min(mi.maxzoom + 3, 22),
        });
        if (!at) map.fitBounds([[w, s], [e, n]], { padding: 20, duration: 0 });
        map.addControl(new maplibregl.NavigationControl(), "top-right");
        map.addControl(new maplibregl.ScaleControl({}), "bottom-left");
        const report = () => {
          const c = map!.getCenter();
          const p = { lat: c.lat, lon: c.lng, zoom: map!.getZoom() };
          pos = `${p.lat.toFixed(5)}, ${p.lon.toFixed(5)} · zoom ${p.zoom.toFixed(1)}`;
          onplace(p);
        };
        map.on("moveend", report);
        map.on("load", () => { el.dataset.loaded = "true"; report(); });
        map.on("error", (e: any) => console.warn("map:", e?.error?.message ?? e));
      } catch (e: any) {
        err = /webgl/i.test(String(e?.message)) ? "This device cannot draw maps: WebGL is not available in this browser or window." : `The map could not be opened: ${e?.message ?? e}`;
      }
    })();
    return () => { cancelled = true; };
  });
  onDestroy(() => map?.remove());
</script>

{#if err}<p class="bad" role="alert">{err}</p>{/if}
<div bind:this={el} role="region" aria-label={`Map: ${res.title}. Select the map, then use arrow keys to pan and plus or minus to zoom.`} data-testid="map"
  style="width:100%;height:65vh;min-height:320px;border:1px solid var(--line);border-radius:8px"></div>
<p class="small muted" aria-live="polite">Centre: {pos}</p>
