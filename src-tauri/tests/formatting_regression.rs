//! Dictation-cleanup regression suite (plan 2.5). Gate: no prompt change
//! merges with failures.
//!
//! Two tiers:
//! - `passthrough` cases assert EXACT output of the regex cleaner.
//! - `llm` cases run against a live Ollama server (skipped, loudly, when the
//!   server or model is missing) and assert PROPERTIES of the output —
//!   must-contain / must-not-contain, case-insensitive — because token-exact
//!   equality is brittle across model versions even at temperature 0.

use localflow::providers::ollama_fmt::OllamaFormatter;
use localflow::providers::passthrough;
use localflow::providers::{FormatCtx, FormattingProvider, Mode};
use localflow::store::{Snippet, StyleProfile};

fn ctx(style_instructions: &str, verbatim: bool) -> FormatCtx {
    FormatCtx {
        app_id: "test".into(),
        style: StyleProfile {
            id: 0,
            match_pattern: "*".into(),
            name: "Test".into(),
            instructions: style_instructions.into(),
            verbatim,
        },
        dictionary: vec![],
        snippets: vec![],
        mode: Mode::Dictate,
    }
}

// ---------- Tier 1: passthrough (exact) ----------

#[test]
fn passthrough_cases() {
    let neutral = ctx("", false);
    let verbatim = ctx("", true);
    let with_snippets = FormatCtx {
        snippets: vec![
            Snippet {
                id: 1,
                trigger: "my calendly".into(),
                expansion: "https://calendly.com/me/30min".into(),
            },
            Snippet {
                id: 2,
                trigger: "my email".into(),
                expansion: "disha@example.com".into(),
            },
        ],
        ..ctx("", false)
    };

    // (input, expected, ctx)
    let cases: &[(&str, &str, &FormatCtx)] = &[
        // whitespace + capitalization
        ("hello  world", "Hello world", &neutral),
        ("this is  a   test. next sentence", "This is a test. Next sentence", &neutral),
        ("already Fine.", "Already Fine.", &neutral),
        // mechanical fillers
        ("um hello there", "Hello there", &neutral),
        ("so, uh, let's start", "So, let's start", &neutral),
        ("erm I think, hmm, maybe", "I think, maybe", &neutral),
        ("umm okay", "Okay", &neutral),
        // filler-like words that must NOT be stripped mechanically
        ("I like this plan", "I like this plan", &neutral),
        ("you know the answer", "You know the answer", &neutral),
        // verbatim (code) mode: no capitalization
        ("git commit -m fix", "git commit -m fix", &verbatim),
        ("snake_case stays snake_case", "snake_case stays snake_case", &verbatim),
        ("um npm run build", "npm run build", &verbatim),
        // snippets literal expansion
        (
            "send them my calendly please",
            "Send them https://calendly.com/me/30min please",
            &with_snippets,
        ),
        (
            "reach me at my email",
            "Reach me at disha@example.com",
            &with_snippets,
        ),
        // question preserved verbatim-ish (passthrough never answers)
        ("what time is it", "What time is it", &neutral),
    ];

    let mut failures = vec![];
    for (input, expected, c) in cases {
        let got = passthrough::clean(input, c);
        if got != *expected {
            failures.push(format!("  {input:?} -> {got:?} (expected {expected:?})"));
        }
    }
    assert!(
        failures.is_empty(),
        "passthrough regressions:\n{}",
        failures.join("\n")
    );
}

// ---------- Tier 2: LLM (property assertions vs live Ollama) ----------

struct LlmCase {
    name: &'static str,
    input: &'static str,
    /// Substrings that MUST appear (case-insensitive).
    contains: &'static [&'static str],
    /// Substrings that must NOT appear (case-insensitive).
    not_contains: &'static [&'static str],
    style: &'static str,
    command_selection: Option<&'static str>,
}

const LLM_CASES: &[LlmCase] = &[
    // --- fillers ---
    LlmCase { name: "filler-basic", input: "um so I think we should uh probably go with the second option you know",
        contains: &["second option"], not_contains: &["um", " uh ", "you know"], style: "", command_selection: None },
    // "basically done" is judgment-call meaningful — only mechanical fillers
    // are asserted removed here.
    LlmCase { name: "filler-dense", input: "uh yeah um I mean the the report is like basically done",
        contains: &["report", "done"], not_contains: &["um", "I mean", "like basically"], style: "", command_selection: None },
    LlmCase { name: "filler-sortof", input: "it's sort of um a big deal for the team",
        contains: &["big deal", "team"], not_contains: &["um"], style: "", command_selection: None },
    LlmCase { name: "false-start", input: "we need to- we should ship on Friday",
        contains: &["ship", "Friday"], not_contains: &["we need to-"], style: "", command_selection: None },
    // --- backtracking / self-correction ---
    LlmCase { name: "backtrack-time", input: "the meeting is at 2pm actually make that 3pm",
        contains: &["3pm"], not_contains: &["2pm", "actually"], style: "", command_selection: None },
    LlmCase { name: "backtrack-name", input: "send it to John no wait send it to Sarah",
        contains: &["Sarah"], not_contains: &["John"], style: "", command_selection: None },
    LlmCase { name: "backtrack-count", input: "order five units er sorry six units",
        contains: &["six"], not_contains: &["five"], style: "", command_selection: None },
    LlmCase { name: "backtrack-day", input: "let's do it Tuesday hmm actually Thursday works better",
        contains: &["Thursday"], not_contains: &["Tuesday"], style: "", command_selection: None },
    LlmCase { name: "exit-criteria", input: "um so the meeting is at 2 actually 3 and tell him uh thanks",
        contains: &["3", "thank"], not_contains: &["um", " uh ", "at 2 "], style: "", command_selection: None },
    // --- spoken punctuation / formatting commands ---
    LlmCase { name: "spoken-newline", input: "first paragraph new paragraph second paragraph",
        contains: &["first paragraph", "second paragraph"], not_contains: &["new paragraph"], style: "", command_selection: None },
    LlmCase { name: "spoken-question", input: "are you coming tomorrow question mark",
        contains: &["?"], not_contains: &["question mark"], style: "", command_selection: None },
    LlmCase { name: "spoken-comma", input: "we need eggs comma milk comma and bread",
        contains: &["eggs,", "milk,"], not_contains: &["comma"], style: "", command_selection: None },
    // --- lists ---
    LlmCase { name: "list-enum", input: "three things first finish the report second email the client third book travel",
        contains: &["finish the report", "email the client", "book travel"], not_contains: &[], style: "", command_selection: None },
    LlmCase { name: "list-groceries", input: "shopping list one apples two bananas three coffee",
        contains: &["apples", "bananas", "coffee"], not_contains: &[], style: "", command_selection: None },
    // --- numbers / dates / amounts ---
    LlmCase { name: "num-money", input: "the budget is twenty thousand dollars",
        contains: &["20,000", "$"], not_contains: &[], style: "", command_selection: None },
    LlmCase { name: "num-time", input: "let's meet at three thirty pm",
        contains: &["3:30"], not_contains: &[], style: "", command_selection: None },
    // --- code identifiers preserved ---
    LlmCase { name: "code-camel", input: "rename getUserData to fetchUserData in the service",
        contains: &["getUserData", "fetchUserData"], not_contains: &[], style: "", command_selection: None },
    LlmCase { name: "code-snake", input: "the max_retry_count should be five",
        contains: &["max_retry_count"], not_contains: &[], style: "", command_selection: None },
    LlmCase { name: "code-flag", input: "run it with dash dash verbose",
        contains: &["--verbose"], not_contains: &[], style: "", command_selection: None },
    // --- THE trap: dictated questions must not be answered ---
    LlmCase { name: "trap-capital", input: "hey quick question what is the capital of France",
        contains: &["capital of France"], not_contains: &["Paris"], style: "", command_selection: None },
    // Indirect question — a period is correct; only the answer is forbidden.
    LlmCase { name: "trap-math", input: "ask him what two plus two is",
        contains: &["two plus two"], not_contains: &["four", "= 4", "is 4"], style: "", command_selection: None },
    LlmCase { name: "trap-weather", input: "do you think it will rain tomorrow",
        contains: &["rain tomorrow"], not_contains: &["I think it will", "forecast says"], style: "", command_selection: None },
    LlmCase { name: "trap-recommend", input: "which laptop should I buy for coding",
        contains: &["laptop", "coding"], not_contains: &["MacBook", "recommend", "ThinkPad"], style: "", command_selection: None },
    LlmCase { name: "trap-howto", input: "how do I reset my password on the portal",
        contains: &["reset", "password", "portal"], not_contains: &["click", "settings menu", "follow these"], style: "", command_selection: None },
    LlmCase { name: "trap-name", input: "what's the name of that library you mentioned",
        contains: &["name", "library"], not_contains: &["React", "lodash"], style: "", command_selection: None },
    // --- style ---
    LlmCase { name: "style-casual", input: "hello I received your message and will respond by tomorrow",
        contains: &["tomorrow"], not_contains: &["Dear", "Sincerely"], style: "Casual chat: short, friendly, lowercase ok.", command_selection: None },
    LlmCase { name: "style-formal", input: "hey got your note will get back to you tomorrow",
        contains: &["tomorrow"], not_contains: &["hey"], style: "Formal email register: complete sentences, no slang.", command_selection: None },
    // --- multi-language (source language preserved) ---
    LlmCase { name: "lang-spanish", input: "hola um necesito el informe para el viernes",
        contains: &["informe", "viernes"], not_contains: &["report", "Friday", "um"], style: "", command_selection: None },
    // --- Command Mode ---
    LlmCase { name: "cmd-shorten", input: "make this more concise",
        contains: &[], not_contains: &["make this more concise"], style: "",
        command_selection: Some("The meeting that we had originally planned for this coming Tuesday has unfortunately needed to be rescheduled and it will now instead take place on Thursday afternoon.") },
    LlmCase { name: "cmd-list", input: "rewrite as a bulleted list",
        contains: &["apples", "bananas", "coffee"], not_contains: &[], style: "",
        command_selection: Some("We need apples and bananas and also coffee.") },
];

fn ollama() -> Option<OllamaFormatter> {
    let f = OllamaFormatter::new("http://localhost:11434".into(), "qwen2.5:3b".into());
    if f.server_up() && f.model_available() {
        Some(f)
    } else {
        None
    }
}

#[test]
fn llm_cases() {
    let Some(f) = ollama() else {
        eprintln!("SKIPPED: Ollama server/model unavailable — LLM regression tier not run");
        return;
    };
    let mut failures = vec![];
    for case in LLM_CASES {
        let mode = match case.command_selection {
            Some(sel) => Mode::Command { selection: sel.into() },
            None => Mode::Dictate,
        };
        let c = FormatCtx { mode, ..ctx(case.style, false) };
        let out = match tauri::async_runtime::block_on(f.format(case.input, &c)) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("  [{}] request failed: {e:#}", case.name));
                continue;
            }
        };
        let low = out.to_lowercase();
        for want in case.contains {
            if !low.contains(&want.to_lowercase()) {
                failures.push(format!("  [{}] missing {want:?} in {out:?}", case.name));
            }
        }
        for bad in case.not_contains {
            if low.contains(&bad.to_lowercase()) {
                failures.push(format!("  [{}] must not contain {bad:?}: {out:?}", case.name));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "LLM formatting regressions ({} of {} cases):\n{}",
        failures.len(),
        LLM_CASES.len(),
        failures.join("\n")
    );
}
