//! SQLite persistence (plan 1.5): history, dictionary, snippets,
//! style_profiles, settings. All local, migrations table from day one.
//! API keys NEVER live here — they go to the OS keychain (see commands.rs).

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub struct Store(Mutex<Connection>);

const MIGRATIONS: &[(&str, &str)] = &[(
    "v1",
    "
    CREATE TABLE history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ts INTEGER NOT NULL,             -- unix seconds
        app_id TEXT NOT NULL DEFAULT '', -- frontmost app (Phase 2 context.rs)
        raw TEXT NOT NULL,
        formatted TEXT NOT NULL,
        stt_provider TEXT NOT NULL,
        fmt_provider TEXT NOT NULL DEFAULT 'passthrough',
        latency_ms INTEGER NOT NULL
    );
    CREATE INDEX idx_history_ts ON history(ts DESC);
    CREATE TABLE dictionary (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        term TEXT NOT NULL UNIQUE,
        spoken_variants TEXT NOT NULL DEFAULT '', -- comma-separated
        case_sensitive INTEGER NOT NULL DEFAULT 1
    );
    CREATE TABLE snippets (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        trigger TEXT NOT NULL UNIQUE,
        expansion TEXT NOT NULL
    );
    CREATE TABLE style_profiles (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        match_pattern TEXT NOT NULL,
        name TEXT NOT NULL,
        instructions TEXT NOT NULL
    );
    CREATE TABLE settings (
        k TEXT PRIMARY KEY,
        v TEXT NOT NULL
    );
    ",
),
(
    "v2-styles-defaults",
    "
    ALTER TABLE style_profiles ADD COLUMN verbatim INTEGER NOT NULL DEFAULT 0;
    INSERT INTO style_profiles (match_pattern, name, instructions, verbatim) VALUES
    ('com.apple.mail,com.microsoft.Outlook,com.readdle.smartemail-Mac',
     'Email',
     'Formal register: complete sentences, correct punctuation and capitalization. No slang. Only include greetings or sign-offs if dictated.',
     0),
    ('com.tinyspeck.slackmacgap,com.hnc.Discord,com.apple.MobileSMS,ru.keepcoder.Telegram,net.whatsapp.WhatsApp,com.facebook.archon',
     'Chat',
     'Casual and concise, like a quick message to a colleague. Lowercase starts are fine. No formal sign-offs.',
     0),
    ('com.apple.Notes,notion.id,md.obsidian,com.google.Docs,com.microsoft.Word',
     'Docs',
     'Structured prose with clear sentences and paragraph breaks. Format enumerations as lists.',
     0),
    ('com.microsoft.VSCode,com.apple.dt.Xcode,com.googlecode.iterm2,com.apple.Terminal,dev.zed.Zed,com.todesktop.230313mzl4w4u92,com.exafunction.windsurf,com.jetbrains.intellij,com.sublimetext.4',
     'Code (verbatim)',
     'Verbatim mode: no LLM rewriting; identifiers, flags and commands pass through untouched.',
     1),
    ('*',
     'Default',
     'Neutral, clear written English. Remove fillers, fix punctuation, keep the speaker''s meaning and tone.',
     0);
    ",
)];

impl Store {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(db_path).context("opening sqlite db")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                name TEXT PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )?;
        for (name, sql) in MIGRATIONS {
            let done: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM schema_migrations WHERE name = ?1",
                    [name],
                    |r| r.get::<_, i64>(0),
                )
                .map(|n| n > 0)?;
            if !done {
                conn.execute_batch(sql)
                    .with_context(|| format!("applying migration {name}"))?;
                conn.execute(
                    "INSERT INTO schema_migrations (name, applied_at) VALUES (?1, strftime('%s','now'))",
                    [name],
                )?;
                tracing::info!("applied db migration {name}");
            }
        }
        Ok(Self(Mutex::new(conn)))
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().expect("db mutex poisoned")
    }

    // ---- settings ----

    pub fn setting(&self, k: &str) -> Option<String> {
        self.conn()
            .query_row("SELECT v FROM settings WHERE k = ?1", [k], |r| r.get(0))
            .ok()
    }

    pub fn setting_or(&self, k: &str, default: &str) -> String {
        self.setting(k).unwrap_or_else(|| default.to_string())
    }

    pub fn set_setting(&self, k: &str, v: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO settings (k, v) VALUES (?1, ?2)
             ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![k, v],
        )?;
        Ok(())
    }

    // ---- history ----

    pub fn history_insert_full(
        &self,
        raw: &str,
        formatted: &str,
        stt_provider: &str,
        fmt_provider: &str,
        latency_ms: u64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO history (ts, raw, formatted, stt_provider, fmt_provider, latency_ms)
             VALUES (strftime('%s','now'), ?1, ?2, ?3, ?4, ?5)",
            params![raw, formatted, stt_provider, fmt_provider, latency_ms as i64],
        )?;
        Ok(())
    }

    pub fn history_list(&self, query: Option<&str>, limit: u32) -> Result<Vec<HistoryRow>> {
        let conn = self.conn();
        let like = format!("%{}%", query.unwrap_or(""));
        let mut stmt = conn.prepare(
            "SELECT id, ts, app_id, raw, formatted, stt_provider, latency_ms FROM history
             WHERE raw LIKE ?1 OR formatted LIKE ?1
             ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![like, limit], |r| {
                Ok(HistoryRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    app_id: r.get(2)?,
                    raw: r.get(3)?,
                    formatted: r.get(4)?,
                    stt_provider: r.get(5)?,
                    latency_ms: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn history_delete(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn history_clear(&self) -> Result<()> {
        self.conn().execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Deletes rows older than `days`. Called at startup (plan 1.6 nightly prune).
    pub fn history_prune(&self, days: u32) -> Result<usize> {
        let n = self.conn().execute(
            "DELETE FROM history WHERE ts < strftime('%s','now') - ?1 * 86400",
            [days as i64],
        )?;
        Ok(n)
    }

    // ---- dictionary ----

    pub fn dict_list(&self) -> Result<Vec<DictEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, term, spoken_variants, case_sensitive FROM dictionary ORDER BY term",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DictEntry {
                    id: r.get(0)?,
                    term: r.get(1)?,
                    spoken_variants: r.get(2)?,
                    case_sensitive: r.get::<_, i64>(3)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn dict_add(&self, term: &str, spoken_variants: &str, case_sensitive: bool) -> Result<()> {
        self.conn().execute(
            "INSERT INTO dictionary (term, spoken_variants, case_sensitive) VALUES (?1, ?2, ?3)
             ON CONFLICT(term) DO UPDATE SET spoken_variants = excluded.spoken_variants,
                                             case_sensitive = excluded.case_sensitive",
            params![term, spoken_variants, case_sensitive as i64],
        )?;
        Ok(())
    }

    pub fn dict_delete(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM dictionary WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- snippets ----

    pub fn snippets_list(&self) -> Result<Vec<Snippet>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT id, trigger, expansion FROM snippets ORDER BY trigger")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Snippet {
                    id: r.get(0)?,
                    trigger: r.get(1)?,
                    expansion: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn snippet_add(&self, trigger: &str, expansion: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO snippets (trigger, expansion) VALUES (?1, ?2)
             ON CONFLICT(trigger) DO UPDATE SET expansion = excluded.expansion",
            params![trigger, expansion],
        )?;
        Ok(())
    }

    pub fn snippet_delete(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- style profiles ----

    pub fn styles_list(&self) -> Result<Vec<StyleProfile>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, match_pattern, name, instructions, verbatim FROM style_profiles ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(StyleProfile {
                    id: r.get(0)?,
                    match_pattern: r.get(1)?,
                    name: r.get(2)?,
                    instructions: r.get(3)?,
                    verbatim: r.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn style_update(&self, id: i64, match_pattern: &str, instructions: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE style_profiles SET match_pattern = ?2, instructions = ?3 WHERE id = ?1",
            params![id, match_pattern, instructions],
        )?;
        Ok(())
    }

    /// Resolves the style for a bundle id: first profile whose comma-separated
    /// patterns contain a match (substring either way), else the '*' fallback.
    pub fn style_for_app(&self, bundle_id: &str) -> StyleProfile {
        let styles = self.styles_list().unwrap_or_default();
        let mut fallback = None;
        for s in &styles {
            for pat in s.match_pattern.split(',') {
                let pat = pat.trim();
                if pat == "*" {
                    fallback = Some(s.clone());
                } else if !pat.is_empty()
                    && !bundle_id.is_empty()
                    && (bundle_id.contains(pat) || pat.contains(bundle_id))
                {
                    return s.clone();
                }
            }
        }
        fallback.unwrap_or(StyleProfile {
            id: 0,
            match_pattern: "*".into(),
            name: "Default".into(),
            instructions: "Neutral, clear written English.".into(),
            verbatim: false,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub id: i64,
    pub ts: i64,
    pub app_id: String,
    pub raw: String,
    pub formatted: String,
    pub stt_provider: String,
    pub latency_ms: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfile {
    pub id: i64,
    pub match_pattern: String,
    pub name: String,
    pub instructions: String,
    pub verbatim: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictEntry {
    pub id: i64,
    pub term: String,
    pub spoken_variants: String,
    pub case_sensitive: bool,
}

/// Post-STT dictionary enforcement (plan 1.7): case-correcting find/replace of
/// each term (and its spoken variants) back to the canonical spelling.
pub fn apply_dictionary(text: &str, entries: &[DictEntry]) -> String {
    let mut out = text.to_string();
    for e in entries {
        let mut needles: Vec<String> = vec![e.term.clone()];
        needles.extend(
            e.spoken_variants
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
        for needle in needles {
            let Ok(re) = regex::RegexBuilder::new(&format!(r"\b{}\b", regex::escape(&needle)))
                .case_insensitive(true)
                .build()
            else {
                continue;
            };
            out = re.replace_all(&out, e.term.as_str()).into_owned();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, variants: &str) -> DictEntry {
        DictEntry {
            id: 0,
            term: term.into(),
            spoken_variants: variants.into(),
            case_sensitive: true,
        }
    }

    #[test]
    fn dictionary_corrects_case_and_variants() {
        let entries = vec![entry("Kubernetes", "cooper netties"), entry("LocalFlow", "")];
        assert_eq!(
            apply_dictionary("deploy to kubernetes with localflow", &entries),
            "deploy to Kubernetes with LocalFlow"
        );
        assert_eq!(
            apply_dictionary("cooper netties cluster", &entries),
            "Kubernetes cluster"
        );
    }

    #[test]
    fn store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("localflow-test-{}", std::process::id()));
        let store = Store::open(&dir.join("t.db")).unwrap();
        store.set_setting("stt_provider", "parakeet").unwrap();
        assert_eq!(store.setting("stt_provider").unwrap(), "parakeet");
        store
            .history_insert_full("raw", "fmt", "whisper-cpp", "passthrough", 900)
            .unwrap();
        assert_eq!(store.history_list(None, 10).unwrap().len(), 1);
        assert_eq!(store.history_list(Some("fmt"), 10).unwrap().len(), 1);
        store.dict_add("Kubernetes", "", true).unwrap();
        assert_eq!(store.dict_list().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
