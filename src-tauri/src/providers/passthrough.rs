//! Passthrough formatter (plan 2.1): regex-level cleanup only. The fastest
//! path, the graceful degradation when no LLM is available, and the whole
//! story in verbatim (code/terminal) contexts.

use super::{FormatCtx, FormattingProvider, Mode};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::LazyLock;

pub struct Passthrough;

static MULTISPACE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[ \t]{2,}").expect("static regex"));

/// Common pure-filler tokens safe to strip mechanically. Deliberately short —
/// aggressive filler removal without an LLM mangles real sentences.
/// (No look-around: the regex crate doesn't support it — capture the leading
/// boundary and restore it in the replacement.)
static FILLERS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::RegexBuilder::new(r"(^|[\s])(?:um+|uh+|erm+|hmm+)[,.]?(?:\s+|$)")
        .case_insensitive(true)
        .build()
        .expect("static regex")
});

pub fn clean(raw: &str, ctx: &FormatCtx) -> String {
    let mut text = raw.trim().to_string();
    // Repeat until fixpoint: adjacent fillers ("uh yeah um") overlap matches.
    loop {
        let next = FILLERS.replace_all(&text, "$1").into_owned();
        if next == text {
            break;
        }
        text = next;
    }
    text = MULTISPACE.replace_all(&text, " ").into_owned();

    // Literal snippet expansion (plan 2.9 passthrough path).
    for s in &ctx.snippets {
        if let Ok(re) = regex::RegexBuilder::new(&format!(r"\b{}\b", regex::escape(&s.trigger)))
            .case_insensitive(true)
            .build()
        {
            text = re.replace_all(&text, s.expansion.as_str()).into_owned();
        }
    }

    // Capitalize sentence starts (skip in verbatim contexts — code casing wins).
    if !ctx.style.verbatim {
        text = capitalize_sentences(&text);
    }
    text.trim().to_string()
}

fn capitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    // '.' only ends a sentence when followed by whitespace — otherwise it's a
    // URL, email, filename or decimal ("calendly.com" must not become ".Com").
    let mut pending_period = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if pending_period {
                at_start = true;
                pending_period = false;
            }
            out.push(ch);
            continue;
        }
        if pending_period {
            pending_period = false; // '.' mid-token: not a sentence end
        }
        if at_start && ch.is_alphabetic() {
            out.extend(ch.to_uppercase());
            at_start = false;
        } else {
            match ch {
                '.' => pending_period = true,
                '!' | '?' => at_start = true,
                _ => at_start = false,
            }
            out.push(ch);
        }
    }
    out
}

#[async_trait]
impl FormattingProvider for Passthrough {
    fn id(&self) -> &'static str {
        "passthrough"
    }

    fn is_local(&self) -> bool {
        true
    }

    async fn format(&self, raw: &str, ctx: &FormatCtx) -> Result<String> {
        // Command mode without an LLM can't rewrite — return the selection
        // untouched so nothing is destroyed.
        if let Mode::Command { selection } = &ctx.mode {
            anyhow::bail!(
                "Command Mode needs an LLM formatter (Ollama or OpenAI); selection left unchanged: {}",
                &selection[..selection.len().min(40)]
            );
        }
        Ok(clean(raw, ctx))
    }
}
