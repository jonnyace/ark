//! Text handling shared by the index builder and query side: tokenising, stemming, chunking, snippets.
use rust_stemmers::{Algorithm, Stemmer};

const STOP: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "do", "does", "for", "from", "has", "have", "how",
    "i", "if", "in", "into", "is", "it", "its", "me", "my", "of", "on", "or", "should", "so", "than", "that", "the",
    "their", "them", "then", "there", "these", "they", "this", "to", "was", "we", "were", "what", "when", "where",
    "which", "who", "why", "will", "with", "you", "your",
];

/// Lower-cased alphanumeric words, stop words removed, stemmed (English Snowball).
pub fn tokenize(text: &str) -> Vec<String> {
    let stemmer = Stemmer::create(Algorithm::English);
    words(text)
        .filter(|w| !STOP.contains(&w.as_str()))
        .map(|w| stemmer.stem(&w).into_owned())
        .collect()
}

/// Lower-cased words of 1..=40 alphanumeric characters (apostrophes inside words are dropped).
pub fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.replace('\'', "").to_lowercase())
        .filter(|w| !w.is_empty() && w.chars().count() <= 40)
}

/// Collapse runs of whitespace to single spaces and trim.
pub fn squash_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split text into chunks of about `max` characters, breaking at sentence or word boundaries.
pub fn chunk_text(text: &str, max: usize) -> Vec<String> {
    let t = squash_ws(text);
    if t.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut rest = t.as_str();
    while rest.chars().count() > max {
        let limit = rest.char_indices().nth(max).map(|(i, _)| i).unwrap_or(rest.len());
        let window = &rest[..limit];
        let cut = window
            .rfind(". ")
            .filter(|&i| i > limit / 2)
            .map(|i| i + 1)
            .or_else(|| window.rfind(' ').filter(|&i| i > limit / 3))
            .unwrap_or(limit);
        out.push(rest[..cut].trim().to_string());
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}

/// A short excerpt of `text` around the first matching query word, about `len` characters.
pub fn snippet(text: &str, query: &str, len: usize) -> String {
    let stemmer = Stemmer::create(Algorithm::English);
    let q: Vec<String> = tokenize(query);
    let mut pos = 0usize;
    let mut offset = 0usize;
    for w in text.split_inclusive(|c: char| !(c.is_alphanumeric() || c == '\'')) {
        let word: String = w.trim_end_matches(|c: char| !(c.is_alphanumeric() || c == '\'')).replace('\'', "").to_lowercase();
        if !word.is_empty() && q.contains(&stemmer.stem(&word).into_owned()) {
            pos = offset;
            break;
        }
        offset += w.len();
    }
    let chars: Vec<char> = text.chars().collect();
    let char_pos = text[..pos].chars().count();
    let start = char_pos.saturating_sub(len / 3);
    let end = (start + len).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    s = squash_ws(&s);
    if start > 0 {
        s = format!("…{s}");
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokenizes_and_stems() {
        assert_eq!(tokenize("Boiling the water, boiled!"), vec!["boil", "water", "boil"]);
        assert_eq!(tokenize("Don't panic"), vec!["dont", "panic"]);
        assert!(tokenize("the and of").is_empty());
    }
    #[test]
    fn chunks_respect_limit() {
        let text = "One sentence here. ".repeat(200);
        let c = chunk_text(&text, 300);
        assert!(c.len() > 5);
        assert!(c.iter().all(|x| x.chars().count() <= 300));
        assert_eq!(chunk_text("  ", 10).len(), 0);
    }
    #[test]
    fn snippet_centres_on_match() {
        let t = format!("{} boiling water kills germs {}", "x ".repeat(100), "y ".repeat(100));
        let s = snippet(&t, "boil", 60);
        assert!(s.contains("boiling"), "{s}");
        assert!(s.starts_with('…') && s.ends_with('…'));
    }
}
