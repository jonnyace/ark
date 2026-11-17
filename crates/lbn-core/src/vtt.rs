//! Minimal WebVTT reader: cue times and text, enough for transcripts, search chunks and timestamp citations.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    let (main, ms) = s.split_once('.')?;
    let parts: Vec<&str> = main.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [m, s] => (0u64, m.parse::<u64>().ok()?, s.parse::<u64>().ok()?),
        [h, m, s] => (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?),
        _ => return None,
    };
    if ms.len() != 3 || m > 59 || sec > 59 {
        return None;
    }
    Some((h * 3600 + m * 60 + sec) as f64 + ms.parse::<u64>().ok()? as f64 / 1000.0)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ")
}

pub fn parse(text: &str) -> Result<Vec<Cue>> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    if !text.starts_with("WEBVTT") {
        return Err(Error::Other("not a WebVTT file".into()));
    }
    let mut cues = Vec::new();
    for block in text.split("\n\n").skip(1) {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        if lines[0].starts_with("NOTE") || lines[0].starts_with("STYLE") || lines[0].starts_with("REGION") {
            continue;
        }
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) else {
            return Err(Error::Other(format!("bad cue time {:?}", lines[ti])));
        };
        let body = lines[ti + 1..].iter().map(|l| strip_tags(l)).collect::<Vec<_>>().join(" ");
        if end >= start {
            cues.push(Cue { start, end, text: crate::text::squash_ws(&body) });
        }
    }
    Ok(cues)
}

/// Group cues into windows of about `secs` seconds for search and retrieval.
pub fn windows(cues: &[Cue], secs: f64) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    for c in cues {
        match out.last_mut() {
            Some(w) if c.start - w.start < secs => {
                w.end = w.end.max(c.end);
                if !w.text.ends_with(&c.text) {
                    w.text.push(' ');
                    w.text.push_str(&c.text);
                }
            }
            _ => out.push(c.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cues() {
        let v = "WEBVTT\nKind: captions\n\nNOTE hi\n\n1\n00:00:01.000 --> 00:00:04.500 align:start\nBring water to a <c>boil</c>.\n\n00:01:05.250 --> 00:01:07.000\nWait &amp; cool.\n";
        let c = parse(v).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].start, 1.0);
        assert_eq!(c[0].text, "Bring water to a boil.");
        assert_eq!(c[1].start, 65.25);
        assert_eq!(c[1].text, "Wait & cool.");
        assert!(parse("hello").is_err());
        let w = windows(&c, 120.0);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].end, 67.0);
    }
}
