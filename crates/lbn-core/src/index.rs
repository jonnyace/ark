//! Portable full-text index (`index/search.lbi`), format version 1.
//!
//! Built once when a pack is made and covered by the pack's hashes, then read by the same code natively and in WASM.
//! Layout: 4 magic bytes `LBI1`, then a postcard-encoded `IndexFile`. Ranking is BM25 (k1 = 1.2, b = 0.75) over chunks,
//! with a bonus for chunks that contain more of the query's distinct terms and for title matches.
use crate::citation::{Citation, Loc};
use crate::error::{Error, Result};
use crate::text::{snippet, tokenize};
use crate::INDEX_FORMAT_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

const MAGIC: &[u8; 4] = b"LBI1";
const K1: f32 = 1.2;
const B: f32 = 0.75;

/// A searchable piece of a resource with its exact location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    pub resource: String,
    pub loc: Loc,
    /// Heading shown with results: resource title plus page/section/time label.
    pub title: String,
    pub text: String,
}

/// Postcard cannot encode internally tagged enums, so the file stores locations in this plain form.
#[derive(Debug, Serialize, Deserialize)]
enum StoredLoc {
    Whole,
    Page(u32),
    Time(f64, Option<f64>),
    Section(String),
    At(f64, f64, f64),
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredChunk {
    resource: String,
    loc: StoredLoc,
    title: String,
    text: String,
}

impl From<Chunk> for StoredChunk {
    fn from(c: Chunk) -> Self {
        let loc = match c.loc {
            Loc::Whole => StoredLoc::Whole,
            Loc::Page { page } => StoredLoc::Page(page),
            Loc::Time { start, end } => StoredLoc::Time(start, end),
            Loc::Section { id } => StoredLoc::Section(id),
            Loc::At { lat, lon, zoom } => StoredLoc::At(lat, lon, zoom),
        };
        StoredChunk { resource: c.resource, loc, title: c.title, text: c.text }
    }
}

impl From<StoredChunk> for Chunk {
    fn from(c: StoredChunk) -> Self {
        let loc = match c.loc {
            StoredLoc::Whole => Loc::Whole,
            StoredLoc::Page(page) => Loc::Page { page },
            StoredLoc::Time(start, end) => Loc::Time { start, end },
            StoredLoc::Section(id) => Loc::Section { id },
            StoredLoc::At(lat, lon, zoom) => Loc::At { lat, lon, zoom },
        };
        Chunk { resource: c.resource, loc, title: c.title, text: c.text }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexFile {
    version: u32,
    pack: String,
    chunks: Vec<StoredChunk>,
    /// Token count per chunk.
    lens: Vec<u32>,
    /// Sorted terms with postings (chunk number, term frequency).
    terms: Vec<(String, Vec<(u32, u16)>)>,
    /// Title tokens per chunk, for the title bonus.
    title_terms: Vec<Vec<String>>,
}

pub struct Index {
    file: IndexFile,
    chunks: Vec<Chunk>,
    lookup: HashMap<String, usize>,
    avg_len: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Hit {
    pub citation: Citation,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    /// Chunk number inside its pack's index (for fetching the full text).
    pub chunk: u32,
}

#[derive(Default)]
pub struct Builder {
    pack: String,
    chunks: Vec<Chunk>,
}

impl Builder {
    pub fn new(pack: &str) -> Self {
        Builder { pack: pack.into(), chunks: vec![] }
    }
    pub fn add(&mut self, chunk: Chunk) {
        if !chunk.text.trim().is_empty() || !chunk.title.trim().is_empty() {
            self.chunks.push(chunk);
        }
    }
    pub fn len(&self) -> usize {
        self.chunks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
    /// Deterministic bytes: the same chunks always give the same file (reproducible packs).
    pub fn build(self) -> Vec<u8> {
        let mut postings: BTreeMap<String, Vec<(u32, u16)>> = BTreeMap::new();
        let mut lens = Vec::with_capacity(self.chunks.len());
        let mut title_terms = Vec::with_capacity(self.chunks.len());
        for (i, c) in self.chunks.iter().enumerate() {
            let toks = tokenize(&format!("{} {}", c.title, c.text));
            lens.push(toks.len() as u32);
            let mut tf: BTreeMap<String, u32> = BTreeMap::new();
            for t in toks {
                *tf.entry(t).or_default() += 1;
            }
            for (t, n) in tf {
                postings.entry(t).or_default().push((i as u32, n.min(u16::MAX as u32) as u16));
            }
            let mut tt = tokenize(&c.title);
            tt.sort();
            tt.dedup();
            title_terms.push(tt);
        }
        let file = IndexFile {
            version: INDEX_FORMAT_VERSION,
            pack: self.pack,
            chunks: self.chunks.into_iter().map(StoredChunk::from).collect(),
            lens,
            terms: postings.into_iter().collect(),
            title_terms,
        };
        let mut out = MAGIC.to_vec();
        out.extend(postcard::to_allocvec(&file).expect("index serialises"));
        out
    }
}

impl Index {
    pub fn from_bytes(bytes: &[u8]) -> Result<Index> {
        if bytes.len() < 4 || &bytes[..4] != MAGIC {
            return Err(Error::Index("not a search index (bad magic)".into()));
        }
        let mut file: IndexFile =
            postcard::from_bytes(&bytes[4..]).map_err(|e| Error::Index(format!("damaged search index: {e}")))?;
        if file.version != INDEX_FORMAT_VERSION {
            return Err(Error::Incompatible(format!("search index version {}", file.version)));
        }
        if file.lens.len() != file.chunks.len() || file.title_terms.len() != file.chunks.len() {
            return Err(Error::Index("inconsistent search index".into()));
        }
        let n = file.chunks.len() as u32;
        if file.terms.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(Error::Index("search index terms are not sorted".into()));
        }
        if file.terms.iter().any(|(_, p)| p.iter().any(|(c, _)| *c >= n)) {
            return Err(Error::Index("search index refers to missing chunks".into()));
        }
        let lookup = file.terms.iter().enumerate().map(|(i, (t, _))| (t.clone(), i)).collect();
        let avg_len = if file.lens.is_empty() {
            1.0
        } else {
            (file.lens.iter().map(|&l| l as f64).sum::<f64>() / file.lens.len() as f64).max(1.0) as f32
        };
        let chunks = std::mem::take(&mut file.chunks).into_iter().map(Chunk::from).collect();
        Ok(Index { file, chunks, lookup, avg_len })
    }

    pub fn pack(&self) -> &str {
        &self.file.pack
    }
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
    pub fn chunk(&self, i: u32) -> Option<&Chunk> {
        self.chunks.get(i as usize)
    }
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Raw scored chunks for a query (chunk number, score), best first.
    pub fn score(&self, query: &str, limit: usize) -> Vec<(u32, f32)> {
        let mut q = tokenize(query);
        q.sort();
        q.dedup();
        if q.is_empty() {
            return vec![];
        }
        let n = self.chunks.len() as f32;
        let mut scores: HashMap<u32, (f32, u32)> = HashMap::new();
        for term in &q {
            let Some(&ti) = self.lookup.get(term) else { continue };
            let postings = &self.file.terms[ti].1;
            let df = postings.len() as f32;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for &(c, tf) in postings {
                let len = self.file.lens[c as usize] as f32;
                let tf = tf as f32;
                let s = idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / self.avg_len));
                let e = scores.entry(c).or_insert((0.0, 0));
                e.0 += s;
                e.1 += 1;
            }
        }
        let qn = q.len() as f32;
        let mut out: Vec<(u32, f32)> = scores
            .into_iter()
            .map(|(c, (s, matched))| {
                let coverage = matched as f32 / qn;
                let title_hits = q.iter().filter(|t| self.file.title_terms[c as usize].binary_search(t).is_ok()).count();
                let title_bonus = 1.0 + 0.25 * title_hits as f32 / qn;
                (c, s * (0.5 + 0.5 * coverage) * title_bonus)
            })
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        out.truncate(limit);
        out
    }

    pub fn hit(&self, chunk: u32, score: f32, query: &str) -> Hit {
        let c = &self.chunks[chunk as usize];
        Hit {
            citation: Citation { pack: self.file.pack.clone(), resource: c.resource.clone(), loc: c.loc.clone() },
            title: c.title.clone(),
            snippet: snippet(&c.text, query, 220),
            score,
            chunk,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        self.score(query, limit).into_iter().map(|(c, s)| self.hit(c, s, query)).collect()
    }
}

/// Search several packs' indexes and merge the results by score.
/// `per_resource` limits how many hits one resource may contribute (keeps one long document from filling the page).
pub fn search_many(indexes: &[&Index], query: &str, limit: usize, per_resource: usize) -> Vec<Hit> {
    let mut all: Vec<Hit> = indexes.iter().flat_map(|ix| ix.search(query, limit * 3)).collect();
    all.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.citation.to_uri().cmp(&b.citation.to_uri())));
    let mut per: HashMap<(String, String), usize> = HashMap::new();
    let mut out = Vec::new();
    for h in all {
        let k = (h.citation.pack.clone(), h.citation.resource.clone());
        let n = per.entry(k).or_default();
        if *n < per_resource.max(1) {
            *n += 1;
            out.push(h);
            if out.len() == limit {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> Index {
        let mut b = Builder::new("org.example.test");
        b.add(Chunk { resource: "water".into(), loc: Loc::Page { page: 2 }, title: "Safe water".into(), text: "Boil water for one minute to kill germs. Store in clean containers.".into() });
        b.add(Chunk { resource: "fire".into(), loc: Loc::Section { id: "start".into() }, title: "Fire".into(), text: "Gather dry tinder and kindling before you light a fire.".into() });
        b.add(Chunk { resource: "vid".into(), loc: Loc::Time { start: 30.0, end: Some(60.0) }, title: "Video".into(), text: "Now we boil the water on the stove.".into() });
        Index::from_bytes(&b.build()).unwrap()
    }

    #[test]
    fn finds_and_ranks() {
        let ix = build();
        let hits = ix.search("how do I boil water?", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].citation.resource, "water");
        assert_eq!(hits[0].citation.to_uri(), "lbn:org.example.test/water#page=2");
        assert!(hits[0].snippet.to_lowercase().contains("boil"));
        assert!(ix.search("zebra", 10).is_empty());
        assert!(ix.search("", 10).is_empty());
    }

    #[test]
    fn deterministic_bytes() {
        let mk = || {
            let mut b = Builder::new("p.x.y");
            b.add(Chunk { resource: "a".into(), loc: Loc::Whole, title: "A".into(), text: "alpha beta gamma".into() });
            b.build()
        };
        assert_eq!(mk(), mk());
    }

    #[test]
    fn rejects_damaged_index() {
        assert!(Index::from_bytes(b"nope").is_err());
        let mut b = Builder::new("p.x.y");
        b.add(Chunk { resource: "a".into(), loc: Loc::Whole, title: "A".into(), text: "alpha".into() });
        let mut bytes = b.build();
        bytes.truncate(bytes.len() - 3);
        assert!(Index::from_bytes(&bytes).is_err());
    }

    #[test]
    fn merges_packs() {
        let a = build();
        let mut b = Builder::new("org.other.pack");
        b.add(Chunk { resource: "w2".into(), loc: Loc::Whole, title: "Water storage".into(), text: "Boil water. Boil water again. Water water.".into() });
        let b = Index::from_bytes(&b.build()).unwrap();
        let hits = search_many(&[&a, &b], "boil water", 10, 1);
        let packs: Vec<_> = hits.iter().map(|h| h.citation.pack.as_str()).collect();
        assert!(packs.contains(&"org.other.pack") && packs.contains(&"org.example.test"));
    }
}
