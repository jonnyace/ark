// Hash routing: #/<view>?<query>. Works from file-like origins, Tauri and any web path without server rewrites.
export const route = $state({ view: "home", params: new URLSearchParams() });

function parse() {
  const h = location.hash.replace(/^#\/?/, "");
  const [view, q] = h.split("?");
  route.view = view || "home";
  route.params = new URLSearchParams(q ?? "");
}
parse();
addEventListener("hashchange", parse);

export function go(view: string, params: Record<string, string> = {}) {
  const q = new URLSearchParams(params).toString();
  location.hash = `#/${view}${q ? "?" + q : ""}`;
}

export function readHref(uri: string) {
  return `#/read?c=${encodeURIComponent(uri)}`;
}
