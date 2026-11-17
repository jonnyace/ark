//! Building packs from source folders (and from legacy Lastbook libraries): copy files, extract text, build the
//! search index, write `pack.json` with sizes and hashes. Output is deterministic for the same input.
//!
//! Source folder: `pack.src.json` = a manifest without `files`/`index`/`format`/`format_version`, plus build-only
//! fields per resource: `text_pages` (JSON file with one string per PDF page) and `text` (plain-text file).
//! `include` lists extra files or folders to ship (licences, map glyphs...).
use crate::util::{hash_file, join_rel};
use crate::{NativeError, Result};
use lbn_core::citation::Loc;
use lbn_core::index::{Builder, Chunk};
use lbn_core::manifest::{self, FileEntry, IndexRef, Manifest, ParsedManifest, Resource, ResourceType, Section, FORMAT_NAME};
use lbn_core::text::{chunk_text, squash_ws};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CHUNK_CHARS: usize = 900;
pub const INDEX_PATH: &str = "index/search.lbi";

#[derive(Debug, Clone, Deserialize)]
pub struct SrcResource {
    #[serde(flatten)]
    pub resource: Resource,
    #[serde(default)]
    pub text_pages: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SrcSpec {
    pub id: String,
    pub version: String,
    pub kind: manifest::PackKind,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub language: Option<String>,
    pub created: String,
    pub publisher: manifest::Publisher,
    pub license: manifest::License,
    pub compat: manifest::Compat,
    pub resources: Vec<SrcResource>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub model: Option<manifest::ModelInfo>,
}

/// Text supplied directly (legacy import) instead of through files named in the spec.
pub enum TextOverride {
    Pages(Vec<String>),
    Plain(String),
}

pub struct BuildInput<'a> {
    pub spec: SrcSpec,
    pub root: &'a Path,
    pub texts: std::collections::HashMap<String, TextOverride>,
    /// Hard-link instead of copying where possible (legacy import of a large library on the same disk).
    pub link: bool,
}

pub fn build_from_dir(src: &Path, out: &Path) -> Result<ParsedManifest> {
    let spec: SrcSpec = serde_json::from_slice(&std::fs::read(src.join("pack.src.json"))?)
        .map_err(|e| NativeError::coded("manifest", format!("pack.src.json: {e}")))?;
    build(BuildInput { spec, root: src, texts: Default::default(), link: false }, out, |_| {})
}

pub fn build(input: BuildInput<'_>, out: &Path, mut log: impl FnMut(&str)) -> Result<ParsedManifest> {
    let BuildInput { mut spec, root, mut texts, link } = input;
    if out.exists() && std::fs::read_dir(out)?.next().is_some() {
        return Err(NativeError::coded("exists", format!("{} is not empty", out.display())));
    }
    std::fs::create_dir_all(out)?;
    let mut paths: BTreeSet<String> = BTreeSet::new();
    let mut ix = Builder::new(&spec.id);
    for sr in spec.resources.iter_mut() {
        let r = &mut sr.resource;
        for p in std::iter::once(&r.path).chain(r.transcript.iter()).chain(r.poster.iter()).chain(r.assets.iter()) {
            paths.insert(p.clone());
        }
        if let Some(l) = r.license.as_ref().and_then(|l| l.file.clone()) {
            paths.insert(l);
        }
        if let Some(m) = &r.map {
            paths.insert(m.style.clone());
        }
        let ov = texts.remove(&r.id);
        index_resource(&mut ix, r, sr.text_pages.as_deref(), sr.text.as_deref(), ov, root)?;
    }
    if let Some(l) = &spec.license.file {
        paths.insert(l.clone());
    }
    for inc in &spec.include {
        let p = join_rel(root, inc)?;
        if p.is_dir() {
            collect(root, &p, &mut paths)?;
        } else {
            paths.insert(inc.clone());
        }
    }
    let mut files = Vec::new();
    for p in &paths {
        let from = join_rel(root, p)?;
        let to = join_rel(out, p)?;
        std::fs::create_dir_all(to.parent().unwrap())?;
        let md = std::fs::symlink_metadata(&from).map_err(|_| NativeError::coded("missing", format!("{p} is missing")))?;
        if !md.file_type().is_file() {
            return Err(NativeError::coded("unsafe_path", format!("{p} is not a regular file")));
        }
        if !(link && std::fs::hard_link(&from, &to).is_ok()) {
            std::fs::copy(&from, &to)?;
        }
        let (size, sha256) = hash_file(&to, |_| {})?;
        files.push(FileEntry { path: p.clone(), size, sha256 });
    }
    let index = if spec.kind != manifest::PackKind::Model && !ix.is_empty() {
        log(&format!("indexed {} chunks", ix.len()));
        let bytes = ix.build();
        let to = join_rel(out, INDEX_PATH)?;
        std::fs::create_dir_all(to.parent().unwrap())?;
        std::fs::write(&to, &bytes)?;
        files.push(FileEntry { path: INDEX_PATH.into(), size: bytes.len() as u64, sha256: lbn_core::hash::sha256_hex(&bytes) });
        Some(IndexRef { search: INDEX_PATH.into() })
    } else {
        None
    };
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let m = Manifest {
        format: FORMAT_NAME.into(),
        format_version: lbn_core::PACK_FORMAT_VERSION,
        id: spec.id,
        version: spec.version,
        kind: spec.kind,
        title: spec.title,
        description: spec.description,
        language: spec.language,
        created: spec.created,
        publisher: spec.publisher,
        license: spec.license,
        compat: spec.compat,
        resources: spec.resources.into_iter().map(|r| r.resource).collect(),
        files,
        index,
        model: spec.model,
    };
    let mut bytes = serde_json::to_vec_pretty(&m).unwrap();
    bytes.push(b'\n');
    let parsed = manifest::parse(&bytes)?;
    std::fs::write(out.join(manifest::MANIFEST_NAME), &bytes)?;
    Ok(parsed)
}

fn collect(root: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(root, &p, out)?;
        } else {
            out.insert(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn index_resource(
    ix: &mut Builder,
    r: &mut Resource,
    pages_file: Option<&str>,
    text_file: Option<&str>,
    ov: Option<TextOverride>,
    root: &Path,
) -> Result<()> {
    let rid = r.id.clone();
    let add = |ix: &mut Builder, loc: Loc, title: String, text: &str| {
        for c in chunk_text(text, CHUNK_CHARS) {
            ix.add(Chunk { resource: rid.clone(), loc: loc.clone(), title: title.clone(), text: c });
        }
    };
    match r.rtype {
        ResourceType::Pdf => {
            let pages: Vec<String> = match (ov, pages_file) {
                (Some(TextOverride::Pages(p)), _) => p,
                (Some(TextOverride::Plain(t)), _) => vec![t],
                (None, Some(f)) => serde_json::from_slice(&std::fs::read(join_rel(root, f)?)?)
                    .map_err(|e| NativeError::coded("manifest", format!("{f}: {e}")))?,
                (None, None) => vec![],
            };
            if r.pages.is_none() && !pages.is_empty() {
                r.pages = Some(pages.len() as u32);
            }
            let n = r.pages.unwrap_or(0) as usize;
            for (i, t) in pages.iter().enumerate().take(n.max(1)) {
                add(ix, Loc::Page { page: i as u32 + 1 }, format!("{} — page {}", r.title, i + 1), t);
            }
        }
        ResourceType::Html | ResourceType::Article => {
            let text = match (ov, text_file) {
                (Some(TextOverride::Plain(t)), _) => Some(t),
                (Some(TextOverride::Pages(p)), _) => Some(p.join("\n")),
                (None, Some(f)) => Some(String::from_utf8_lossy(&std::fs::read(join_rel(root, f)?)?).into_owned()),
                (None, None) => None,
            };
            match text {
                Some(t) => add(ix, Loc::Whole, r.title.clone(), &t),
                None => {
                    let html = String::from_utf8_lossy(&std::fs::read(join_rel(root, &r.path)?)?).into_owned();
                    let doc = html_text(&html);
                    if r.sections.is_empty() {
                        r.sections = doc.sections.iter().map(|(id, title, _)| Section { id: id.clone(), title: title.clone() }).collect();
                    }
                    if !doc.intro.trim().is_empty() {
                        add(ix, Loc::Whole, r.title.clone(), &doc.intro);
                    }
                    for (id, title, body) in &doc.sections {
                        add(ix, Loc::Section { id: id.clone() }, format!("{} — {}", r.title, title), &format!("{title}. {body}"));
                    }
                }
            }
        }
        ResourceType::Video => {
            let mut done = false;
            if let Some(t) = &r.transcript {
                if t.ends_with(".vtt") {
                    let vtt = String::from_utf8_lossy(&std::fs::read(join_rel(root, t)?)?).into_owned();
                    let cues = lbn_core::vtt::parse(&vtt)?;
                    let dur = r.duration_s.unwrap_or(f64::MAX);
                    for w in lbn_core::vtt::windows(&cues, 30.0) {
                        let start = w.start.min(dur);
                        let end = w.end.min(dur);
                        let label = format!("{} — {}", r.title, lbn_core::citation::time_label(start));
                        ix.add(Chunk { resource: r.id.clone(), loc: Loc::Time { start, end: Some(end) }, title: label, text: w.text });
                    }
                    done = true;
                }
            }
            if !done {
                if let Some(TextOverride::Plain(t)) = ov {
                    add(ix, Loc::Whole, r.title.clone(), &t);
                } else {
                    ix.add(Chunk { resource: r.id.clone(), loc: Loc::Whole, title: r.title.clone(), text: r.summary.clone().unwrap_or_default() });
                }
            }
        }
        ResourceType::Map => {
            ix.add(Chunk { resource: r.id.clone(), loc: Loc::Whole, title: r.title.clone(), text: r.summary.clone().unwrap_or_default() });
        }
        ResourceType::Model => {}
    }
    Ok(())
}

pub struct HtmlDoc {
    pub title: String,
    pub intro: String,
    /// (id, heading, body text) for each h2/h3 that has an id.
    pub sections: Vec<(String, String, String)>,
}

/// Small, tolerant HTML-to-text extractor for search: drops head/script/style/template, keeps h2/h3 ids as sections.
/// It never executes or renders anything; display uses the sandboxed viewer.
pub fn html_text(html: &str) -> HtmlDoc {
    let mut doc = HtmlDoc { title: String::new(), intro: String::new(), sections: vec![] };
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let mut skip_until: Option<&str> = None;
    let mut cur_heading: Option<(String, String)> = None; // (id, collected heading text)
    let mut in_heading = false;
    let mut in_title = false;
    let mut buf = String::new();
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let end = match lower[i..].find('>') {
                Some(e) => i + e,
                None => break,
            };
            let tag = &lower[i + 1..end];
            let raw = &html[i + 1..end];
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let closing = tag.starts_with('/');
            if let Some(stop) = skip_until {
                if closing && name == stop {
                    skip_until = None;
                }
                i = end + 1;
                continue;
            }
            match (name.as_str(), closing) {
                ("script" | "style" | "template" | "noscript" | "svg", false) => skip_until = Some(match name.as_str() { "script" => "script", "style" => "style", "template" => "template", "noscript" => "noscript", _ => "svg" }),
                ("title", false) => in_title = true,
                ("title", true) => in_title = false,
                ("h2" | "h3", false) => {
                    if let Some(id) = attr(raw, "id") {
                        flush(&mut doc, &mut cur_heading, &mut buf);
                        cur_heading = Some((id, String::new()));
                        in_heading = true;
                    }
                }
                ("h2" | "h3", true) => in_heading = false,
                ("p" | "div" | "li" | "br" | "tr" | "h1" | "h4" | "td" | "section", _) => buf.push(' '),
                _ => {}
            }
            i = end + 1;
            continue;
        }
        let next = lower[i..].find('<').map(|n| i + n).unwrap_or(bytes.len());
        let text = decode_entities(&html[i..next]);
        if skip_until.is_none() {
            if in_title {
                doc.title.push_str(&text);
            } else if in_heading {
                if let Some((_, h)) = cur_heading.as_mut() {
                    h.push_str(&text);
                }
            } else {
                buf.push_str(&text);
            }
        }
        i = next;
    }
    flush(&mut doc, &mut cur_heading, &mut buf);
    doc.title = squash_ws(&doc.title);
    doc
}

fn flush(doc: &mut HtmlDoc, cur: &mut Option<(String, String)>, buf: &mut String) {
    let body = squash_ws(buf);
    match cur.take() {
        Some((id, h)) if lbn_core::manifest::is_valid_resource_id(&id) => doc.sections.push((id, squash_ws(&h), body)),
        _ => {
            if !body.is_empty() {
                if !doc.intro.is_empty() {
                    doc.intro.push(' ');
                }
                doc.intro.push_str(&body);
            }
        }
    }
    buf.clear();
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let l = tag.to_ascii_lowercase();
    let pos = l.find(&format!("{name}="))?;
    let rest = &tag[pos + name.len() + 1..];
    let (q, rest) = match rest.chars().next()? {
        '"' => ('"', &rest[1..]),
        '\'' => ('\'', &rest[1..]),
        _ => (' ', rest),
    };
    Some(rest.split(q).next()?.trim_end_matches('/').to_string())
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&rsquo;", "\u{2019}")
        .replace("&mdash;", "\u{2014}")
        .replace("&ndash;", "\u{2013}")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_sections() {
        let d = html_text("<html><head><title>T &amp; U</title><style>p{}</style></head><body><h1>Top</h1><p>Intro text.</p>\
            <script>alert('x')</script><h2 id=\"boil\">Boiling</h2><p>Boil one minute.</p><h3 id='cool'>Cool</h3><p>Let it cool.</p>\
            <h2>No id</h2><p>Tail.</p></body></html>");
        assert_eq!(d.title, "T & U");
        assert_eq!(d.intro, "Top Intro text.");
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.sections[0], ("boil".into(), "Boiling".into(), "Boil one minute.".into()));
        assert_eq!(d.sections[1].2, "Let it cool. No id Tail.");
    }
}
