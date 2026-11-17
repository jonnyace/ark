// Write a MapLibre style for a Protomaps-basemap PMTiles file, with every URL pointing INSIDE the map pack:
// tiles "pmtiles://map.pmtiles", glyphs "fonts/{fontstack}/{range}.pbf", sprite "sprites/light".
// The app rewrites these pack-relative URLs to local files at display time; nothing is fetched from the network.
// Usage: node apps/web/tools/map-style.mjs out/style.json [flavor=light] [lang=en]
import { writeFileSync } from "node:fs";
import { layers, namedFlavor } from "@protomaps/basemaps";

const [, , out, flavor = "light", lang = "en"] = process.argv;
const style = {
  version: 8,
  name: `Ark offline map (${flavor})`,
  glyphs: "fonts/{fontstack}/{range}.pbf",
  sprite: `sprites/${flavor}`,
  sources: {
    protomaps: {
      type: "vector",
      url: "pmtiles://map.pmtiles",
      attribution: "© OpenStreetMap contributors (ODbL); Protomaps basemap",
    },
  },
  layers: layers("protomaps", namedFlavor(flavor), { lang }),
};
writeFileSync(out, JSON.stringify(style, null, 1) + "\n");
const fonts = new Set();
JSON.stringify(style.layers, (k, v) => {
  if (k === "text-font") v.flat(Infinity).filter((x) => typeof x === "string" && x.includes(" ")).forEach((f) => fonts.add(f));
  return v;
});
console.log(`wrote ${out}: ${style.layers.length} layers; fonts used: ${[...fonts].join(", ")}`);
