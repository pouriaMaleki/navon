//! i18n catalog tooling.
//!
//! Source of truth: `i18n/locales/<locale>.json`. EN is authored by humans;
//! other locales are populated by `cargo xtask i18n-sync` (OpenAI) and
//! committed for review. This module:
//!
//! 1. Loads + validates the catalog files.
//! 2. Generates per-platform outputs (web JSON, iOS xcstrings, Android
//!    strings.xml, parity snapshots).
//! 3. Calls the OpenAI API to fill missing translations, with a sticky
//!    source-hash mechanism so manual edits aren't overwritten.
//!
//! Subcommands:
//!   cargo xtask i18n-gen [--check]
//!   cargo xtask i18n-sync --locale <code> [--dry-run] [--budget-usd <N>]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CATALOG_CONFIG: &str = "i18n/catalog.config.json";
const GLOSSARY: &str = "i18n/glossary.json";
const LOCALES_DIR: &str = "i18n/locales";

#[derive(Debug, Deserialize)]
pub struct CatalogConfig {
    /// Schema version. Reserved for forward-compat — readers may reject
    /// catalogs whose version they don't understand.
    #[allow(dead_code)]
    pub version: u32,
    #[serde(rename = "sourceLocale")]
    pub source_locale: String,
    pub locales: Vec<String>,
    pub openai: OpenAiConfig,
    pub outputs: OutputsConfig,
    /// Per-locale options: writing-direction hint for layout helpers and
    /// a free-text variant string fed to the translator so it produces
    /// the most-used variant of a language (e.g. Brazilian Portuguese
    /// for `pt`, Latin American Spanish for `es`). All fields optional;
    /// missing locales fall back to LTR + no extra translator hint.
    #[serde(rename = "localeOptions", default)]
    pub locale_options: BTreeMap<String, LocaleOption>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LocaleOption {
    /// True for right-to-left scripts (Arabic, Persian, Hebrew, Urdu).
    /// Surfaced in the catalog as documentation; each platform also
    /// keeps its own RTL list (see e.g. `RTL_LOCALES` in
    /// companion-web/src/i18n/index.ts) for runtime layout decisions.
    #[allow(dead_code)]
    #[serde(default)]
    pub rtl: bool,
    #[serde(rename = "translatorVariant", default)]
    pub translator_variant: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiConfig {
    pub model: String,
    pub endpoint: String,
    #[serde(rename = "maxKeysPerRequest")]
    pub max_keys_per_request: usize,
}

#[derive(Debug, Deserialize)]
pub struct OutputsConfig {
    pub web: String,
    pub ios: String,
    #[serde(rename = "iosMessages")]
    pub ios_messages: String,
    /// Source-locale Android values dir (e.g. `.../res/values`). For
    /// non-source locales we append `-<locale>` automatically — no need
    /// to enumerate every locale here.
    #[serde(rename = "androidValues")]
    pub android_values: String,
    #[serde(rename = "androidRaw")]
    pub android_raw: String,
    #[serde(rename = "parityFixture")]
    pub parity_fixture: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Glossary {
    #[serde(rename = "doNotTranslate", default)]
    pub do_not_translate: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Catalog {
    pub locale: String,
    /// `BTreeMap` so writes are deterministic (key-sorted).
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Entry {
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub placeholders: BTreeMap<String, String>,
    #[serde(rename = "sourceHash", default, skip_serializing_if = "String::is_empty")]
    pub source_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EntryStatus>,
    #[serde(rename = "translatedAt", default, skip_serializing_if = "Option::is_none")]
    pub translated_at: Option<String>,
    #[serde(rename = "translatedBy", default, skip_serializing_if = "Option::is_none")]
    pub translated_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryStatus {
    Missing,
    AiTranslated,
    Reviewed,
    Locked,
}

pub fn run(args: &[String], workspace_root: &Path) -> Result<(), String> {
    let (subcommand, rest) = match args.split_first() {
        Some(parts) => parts,
        None => return Err(help_text()),
    };
    match subcommand.as_str() {
        "i18n-gen" => run_gen(rest, workspace_root),
        "i18n-sync" => run_sync(rest, workspace_root),
        "i18n-sync-all" => run_sync_all(rest, workspace_root),
        "i18n-extract" => Err(
            "i18n-extract is not implemented yet; hand-author keys in i18n/locales/en.json"
                .to_owned(),
        ),
        other => Err(format!("unknown i18n subcommand `{other}`. {}", help_text())),
    }
}

/// Convenience wrapper: run `i18n-sync` for every non-source locale in
/// catalog.config.json, then refresh platform outputs. Same flag set as
/// `i18n-sync` minus `--locale` (the locales are read from the config).
/// Designed to be invoked from a VSCode task or a one-key keybinding.
fn run_sync_all(args: &[String], root: &Path) -> Result<(), String> {
    let mut dry_run = false;
    let mut budget_usd: Option<f64> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--budget-usd" => {
                budget_usd = Some(
                    iter.next()
                        .ok_or_else(|| "`--budget-usd` requires a value".to_owned())?
                        .parse()
                        .map_err(|e| format!("invalid --budget-usd: {e}"))?,
                );
            }
            other => return Err(format!("unsupported `i18n-sync-all` argument `{other}`")),
        }
    }

    let config = load_config(root)?;
    let targets: Vec<&String> = config
        .locales
        .iter()
        .filter(|l| **l != config.source_locale)
        .collect();
    if targets.is_empty() {
        println!("no non-source locales configured — nothing to sync.");
        return Ok(());
    }
    println!(
        "i18n-sync-all: syncing {} locale(s){}",
        targets.len(),
        if dry_run { " (dry run)" } else { "" }
    );

    for locale in targets {
        println!("\n— locale: {locale} —");
        let mut sub_args: Vec<String> = vec!["--locale".into(), locale.clone()];
        if dry_run {
            sub_args.push("--dry-run".into());
        }
        if let Some(b) = budget_usd {
            sub_args.push("--budget-usd".into());
            sub_args.push(format!("{b}"));
        }
        run_sync(&sub_args, root)?;
    }

    if !dry_run {
        // One final codegen pass so the platform-bundled JSON / xcstrings /
        // strings.xml files reflect every locale's freshly-translated keys.
        run_gen(&[], root)?;
    }
    Ok(())
}

fn help_text() -> String {
    "i18n subcommands:\n  \
     cargo xtask i18n-gen [--check]\n  \
     cargo xtask i18n-sync --locale <code> [--dry-run] [--budget-usd <N>]\n  \
     cargo xtask i18n-sync-all [--dry-run] [--budget-usd <N>]"
        .to_owned()
}

// -----------------------------------------------------------------------------
// gen
// -----------------------------------------------------------------------------

fn run_gen(args: &[String], root: &Path) -> Result<(), String> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => return Err(format!("unsupported `i18n-gen` argument `{other}`")),
        }
    }

    let config = load_config(root)?;
    let mut catalogs: BTreeMap<String, Catalog> = BTreeMap::new();
    for locale in &config.locales {
        let catalog = load_catalog(root, locale)?;
        catalogs.insert(locale.clone(), catalog);
    }

    let en = catalogs
        .get(&config.source_locale)
        .ok_or_else(|| format!("source locale `{}` not in catalogs", config.source_locale))?
        .clone();

    let outputs = collect_outputs(root, &config, &catalogs, &en)?;

    if check {
        let mut diffs = Vec::new();
        for (path, content) in &outputs {
            match fs::read_to_string(path) {
                Ok(existing) if existing == *content => {}
                _ => diffs.push(path.display().to_string()),
            }
        }
        if !diffs.is_empty() {
            return Err(format!(
                "i18n outputs are stale; re-run `cargo xtask i18n-gen`. Stale files:\n  {}",
                diffs.join("\n  ")
            ));
        }
        println!("i18n outputs are up to date.");
        return Ok(());
    }

    for (path, content) in &outputs {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("create_dir_all {}: {e}", parent.display()))?;
        }
        fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    println!("i18n: wrote {} files.", outputs.len());
    Ok(())
}

fn collect_outputs(
    root: &Path,
    config: &CatalogConfig,
    catalogs: &BTreeMap<String, Catalog>,
    en: &Catalog,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut outputs = Vec::new();

    // Web: one flat key→value JSON per locale. Untranslated keys fall back
    // to the EN value so the runtime can resolve everything.
    for locale in &config.locales {
        let path = root.join(&config.outputs.web).join(format!("{locale}.json"));
        let cat = catalogs.get(locale).expect("locale must exist");
        let mut flat: BTreeMap<&String, &String> = BTreeMap::new();
        for (key, en_entry) in &en.entries {
            let value = cat
                .entries
                .get(key)
                .map(|e| &e.value)
                .unwrap_or(&en_entry.value);
            flat.insert(key, value);
        }
        outputs.push((path, format!("{}\n", serde_json::to_string_pretty(&flat).unwrap())));
    }

    // iOS: one Localizable.xcstrings file with all locales merged.
    let ios_path = root.join(&config.outputs.ios);
    outputs.push((ios_path, build_xcstrings(config, catalogs, en)));

    // iOS runtime JSON: same per-locale flat key→value JSON we ship to web,
    // bundled with the app so the i18n runtime can switch locales at run-
    // time (xcstrings is compiled into the bundle by Xcode and not readable
    // as JSON at runtime).
    for locale in &config.locales {
        let path = root
            .join(&config.outputs.ios_messages)
            .join(format!("{locale}.json"));
        let cat = catalogs.get(locale).expect("locale must exist");
        let mut flat: BTreeMap<&String, &String> = BTreeMap::new();
        for (key, en_entry) in &en.entries {
            let value = cat
                .entries
                .get(key)
                .map(|e| &e.value)
                .unwrap_or(&en_entry.value);
            flat.insert(key, value);
        }
        outputs.push((path, format!("{}\n", serde_json::to_string_pretty(&flat).unwrap())));
    }

    // Android: one strings.xml per locale. Source locale → `values/`;
    // every other locale → `values-<code>/` derived from the source dir.
    let android_values_base = root.join(&config.outputs.android_values);
    for locale in &config.locales {
        let dir = if locale == &config.source_locale {
            android_values_base.clone()
        } else {
            // Strip a trailing path segment + replace with values-<locale>.
            // We assume the configured path ends with `values` (the standard
            // Android convention) so non-source locales sit alongside it
            // as a sibling `values-<locale>` dir.
            let parent = android_values_base
                .parent()
                .ok_or_else(|| format!("androidValues path has no parent: {}", android_values_base.display()))?;
            parent.join(format!("values-{locale}"))
        };
        let path = dir.join("strings.xml");
        let cat = catalogs.get(locale).expect("locale must exist");
        outputs.push((path, build_strings_xml(en, cat)));
    }

    // Android runtime JSON: per-locale flat key→value JSON in res/raw,
    // bundled with the app so the i18n runtime can switch locales at run-
    // time. We emit `messages_<locale>.json` because res/raw resource
    // names must match `[a-z0-9_]+`.
    for locale in &config.locales {
        let path = root
            .join(&config.outputs.android_raw)
            .join(format!("messages_{locale}.json"));
        let cat = catalogs.get(locale).expect("locale must exist");
        let mut flat: BTreeMap<&String, &String> = BTreeMap::new();
        for (key, en_entry) in &en.entries {
            let value = cat
                .entries
                .get(key)
                .map(|e| &e.value)
                .unwrap_or(&en_entry.value);
            flat.insert(key, value);
        }
        outputs.push((path, format!("{}\n", serde_json::to_string_pretty(&flat).unwrap())));
    }

    // Parity fixture — structured `cueMessage` outputs every platform must agree on.
    outputs.push((
        root.join(&config.outputs.parity_fixture),
        build_parity_fixture(),
    ));

    Ok(outputs)
}

fn build_xcstrings(
    config: &CatalogConfig,
    catalogs: &BTreeMap<String, Catalog>,
    en: &Catalog,
) -> String {
    // Apple's xcstrings is a single JSON file with shape:
    // {
    //   "sourceLanguage": "en",
    //   "version": "1.0",
    //   "strings": {
    //     "<key>": {
    //       "comment": "...",
    //       "extractionState": "manual",
    //       "localizations": {
    //         "en": { "stringUnit": { "state": "translated", "value": "..." } },
    //         "fi": { "stringUnit": { "state": "translated", "value": "..." } }
    //       }
    //     }, ...
    //   }
    // }
    let mut strings = serde_json::Map::new();
    for (key, en_entry) in &en.entries {
        let mut locs = serde_json::Map::new();
        for locale in &config.locales {
            let cat = catalogs.get(locale).expect("locale must exist");
            let value = cat
                .entries
                .get(key)
                .map(|e| e.value.clone())
                .unwrap_or_else(|| en_entry.value.clone());
            locs.insert(
                locale.clone(),
                serde_json::json!({
                    "stringUnit": { "state": "translated", "value": value }
                }),
            );
        }
        let mut entry = serde_json::Map::new();
        if !en_entry.description.is_empty() {
            entry.insert(
                "comment".to_owned(),
                serde_json::Value::String(en_entry.description.clone()),
            );
        }
        entry.insert(
            "extractionState".to_owned(),
            serde_json::Value::String("manual".to_owned()),
        );
        entry.insert("localizations".to_owned(), serde_json::Value::Object(locs));
        strings.insert(key.clone(), serde_json::Value::Object(entry));
    }

    let root = serde_json::json!({
        "sourceLanguage": config.source_locale,
        "version": "1.0",
        "strings": strings,
    });
    format!("{}\n", serde_json::to_string_pretty(&root).unwrap())
}

fn build_strings_xml(en: &Catalog, target: &Catalog) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
    // App-level strings that are not part of the translatable catalog but are
    // required by AndroidManifest.xml. These are written to every locale so
    // the build always finds them; app_name is intentionally not translated.
    s.push_str("    <string name=\"app_name\">ESP32MapCompanion</string>\n");
    for (key, en_entry) in &en.entries {
        let value = target
            .entries
            .get(key)
            .map(|e| e.value.as_str())
            .unwrap_or(en_entry.value.as_str());
        let android_key = key.replace('.', "_");
        // formatted="false" tells Android's getString() to skip its own printf
        // formatting; we render the ICU template ourselves at runtime via
        // android.icu.text.MessageFormat.
        s.push_str(&format!(
            "    <string name=\"{}\" formatted=\"false\">{}</string>\n",
            android_key,
            xml_escape(value)
        ));
    }
    s.push_str("</resources>\n");
    s
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
}

fn build_parity_fixture() -> String {
    // The cue-engine surface that all three platforms must format identically
    // when the locale is forced to English. Each row is `(event, expected)`
    // where `expected` is the structured (key, values) tuple a platform's
    // `cueMessage(event, distanceUnit="metric")` should return.
    //
    // Distance values match the existing test fixtures: 187 → 190 (rounded
    // to 10), 184 → 180, etc. Distance unit is forced to metric here; the
    // imperial-mode parity is platform-local.
    let maneuvers_50 = [
        "left", "right", "keepLeft", "keepRight", "exitLeft", "exitRight", "uturn", "generic",
    ];
    let maneuvers_10 = maneuvers_50;
    let maneuvers_next = ["left", "right", "uturn", "generic"];

    let mut rows = Vec::new();
    for k in maneuvers_50 {
        rows.push(serde_json::json!({
            // `distanceM: 50` represents the rider crossing the 50 m
            // approach threshold from above (the typical case). The
            // event now carries the actual distance so route-start
            // edge cases ("d = 15 m at first tick") render with the
            // real value instead of a hardcoded 50.
            "event": { "kind": "turn50m", "turnKind": k, "distanceM": 50 },
            "expected": {
                "key": format!("cue.turn50m.{k}"),
                "values": { "distance": 50, "distanceUnit": "meters" }
            }
        }));
    }
    // Combined back-to-back cue: when a follow-up maneuver lies within
    // ~30 m of the upcoming one, both platforms must coalesce into a
    // single ICU `cue.turn50mCombined` phrase rather than queueing two
    // overlapping cues.
    rows.push(serde_json::json!({
        "event": { "kind": "turn50m", "turnKind": "right", "distanceM": 50, "followUpKind": "left" },
        "expected": {
            "key": "cue.turn50mCombined",
            "values": {
                "distance": 50,
                "distanceUnit": "meters",
                "first": "right",
                "second": "left"
            }
        }
    }));
    for k in maneuvers_10 {
        rows.push(serde_json::json!({
            "event": { "kind": "turn10m", "turnKind": k },
            "expected": { "key": format!("cue.turn10m.{k}"), "values": {} }
        }));
    }
    for k in maneuvers_next {
        rows.push(serde_json::json!({
            "event": { "kind": "nextTurnInAbout", "turnKind": k, "distanceM": 187 },
            "expected": {
                "key": format!("cue.nextTurnInAbout.{k}"),
                "values": { "distance": 190, "distanceUnit": "meters" }
            }
        }));
    }
    rows.push(serde_json::json!({
        "event": { "kind": "arrivingInM", "distanceM": 184 },
        "expected": {
            "key": "cue.arrivingInM",
            "values": { "distance": 180, "distanceUnit": "meters" }
        }
    }));
    for kind in ["arrived", "offTrack", "rerouting", "onTrack", "repeatedOffTrackSilence"] {
        let key = if kind == "repeatedOffTrackSilence" {
            "cue.offTrack" // shares text with offTrack
        } else {
            match kind {
                "arrived" => "cue.arrived",
                "offTrack" => "cue.offTrack",
                "rerouting" => "cue.rerouting",
                "onTrack" => "cue.onTrack",
                _ => unreachable!(),
            }
        };
        rows.push(serde_json::json!({
            "event": { "kind": kind },
            "expected": { "key": key, "values": {} }
        }));
    }

    let root = serde_json::json!({
        "version": 1,
        "comment": "Generated by xtask i18n-gen — every platform's cueMessage(event, distanceUnit=\"metric\") must produce `expected`.",
        "rows": rows,
    });
    format!("{}\n", serde_json::to_string_pretty(&root).unwrap())
}

// -----------------------------------------------------------------------------
// sync
// -----------------------------------------------------------------------------

fn run_sync(args: &[String], root: &Path) -> Result<(), String> {
    let mut locale: Option<String> = None;
    let mut dry_run = false;
    let mut budget_usd: Option<f64> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--locale" => {
                locale = Some(
                    iter.next()
                        .ok_or_else(|| "`--locale` requires a value".to_owned())?
                        .clone(),
                );
            }
            other if other.starts_with("--locale=") => {
                locale = Some(other.trim_start_matches("--locale=").to_owned());
            }
            "--dry-run" => dry_run = true,
            "--budget-usd" => {
                budget_usd = Some(
                    iter.next()
                        .ok_or_else(|| "`--budget-usd` requires a value".to_owned())?
                        .parse()
                        .map_err(|e| format!("invalid --budget-usd: {e}"))?,
                );
            }
            other => return Err(format!("unsupported `i18n-sync` argument `{other}`")),
        }
    }
    let locale = locale.ok_or_else(|| "`--locale <code>` is required".to_owned())?;

    let config = load_config(root)?;
    if locale == config.source_locale {
        return Err(format!("cannot sync the source locale `{locale}`"));
    }
    if !config.locales.contains(&locale) {
        return Err(format!(
            "locale `{locale}` is not in catalog.config.json:locales"
        ));
    }

    let glossary = load_glossary(root)?;
    let en = load_catalog(root, &config.source_locale)?;
    let mut tgt = load_catalog_or_empty(root, &locale);

    let mut to_translate = Vec::new();
    for (key, en_entry) in &en.entries {
        let en_hash = source_hash(en_entry);
        let needs = match tgt.entries.get(key) {
            None => true,
            Some(t) => {
                let status = t.status.unwrap_or(EntryStatus::AiTranslated);
                match status {
                    // Locked: never re-translate. The user has frozen this
                    // entry intentionally and wants to maintain it by hand.
                    EntryStatus::Locked => false,
                    // Reviewed: sticky. If the user wrote a sourceHash,
                    // honour drift detection (re-translate when EN
                    // changes); if not (hand-curated entry), trust that
                    // the user's translation applies to the current
                    // source. To force re-translation, clear the entry.
                    EntryStatus::Reviewed => {
                        !t.source_hash.is_empty() && t.source_hash != en_hash
                    }
                    // AiTranslated (or missing status): sticky as long as
                    // the recorded source hash matches. EN drift triggers
                    // a re-translation. Empty hash = legacy entry from a
                    // pre-hash sync; treat as drifted to refresh.
                    _ => t.source_hash != en_hash,
                }
            }
        };
        if needs {
            to_translate.push(key.clone());
        }
    }

    if to_translate.is_empty() {
        println!("locale `{locale}` is up to date — nothing to translate.");
        return Ok(());
    }

    println!(
        "i18n-sync: {} key(s) to translate to `{locale}` via {}{}",
        to_translate.len(),
        config.openai.model,
        if dry_run { " (dry run)" } else { "" }
    );
    if let Some(b) = budget_usd {
        println!("  budget cap: ${b:.2}");
    }

    if dry_run {
        for key in &to_translate {
            println!("  - {key}");
        }
        return Ok(());
    }

    // Auto-load `.env` at the workspace root so users don't have to remember
    // `set -a; source .env; set +a` every session. Existing env vars take
    // precedence so an explicit shell export still wins.
    load_dotenv(root);
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| {
            "OPENAI_API_KEY env var is required for `i18n-sync`. \
             Either `export OPENAI_API_KEY=...` or copy `.env.example` to \
             `.env` at the workspace root and fill in your key."
                .to_owned()
        })?;

    let chunks: Vec<Vec<String>> = to_translate
        .chunks(config.openai.max_keys_per_request)
        .map(|c| c.to_vec())
        .collect();

    let now = iso8601_now();
    let model_tag = format!("openai/{}", config.openai.model);
    let locale_option = config.locale_options.get(&locale);
    for chunk in chunks {
        let translated = translate_chunk(
            &config.openai, &api_key, &locale, locale_option,
            &en, &chunk, &glossary,
        )?;
        for (key, value) in translated {
            let en_entry = en
                .entries
                .get(&key)
                .ok_or_else(|| format!("translator returned unknown key `{key}`"))?;
            validate_placeholders(&en_entry.value, &value, &key)?;
            tgt.entries.insert(
                key,
                Entry {
                    value,
                    description: en_entry.description.clone(),
                    placeholders: en_entry.placeholders.clone(),
                    source_hash: source_hash(en_entry),
                    status: Some(EntryStatus::AiTranslated),
                    translated_at: Some(now.clone()),
                    translated_by: Some(model_tag.clone()),
                },
            );
        }
    }

    write_catalog(root, &locale, &tgt)?;
    println!("wrote i18n/locales/{locale}.json — review the diff and run `cargo xtask i18n-gen`.");
    Ok(())
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    response_format: serde_json::Value,
    temperature: f32,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: String,
}

fn translate_chunk(
    cfg: &OpenAiConfig,
    api_key: &str,
    locale: &str,
    locale_option: Option<&LocaleOption>,
    en: &Catalog,
    keys: &[String],
    glossary: &Glossary,
) -> Result<Vec<(String, String)>, String> {
    let system = "\
You translate ICU MessageFormat strings for a cycling navigation companion app. \
Strict rules: \
(1) preserve every {placeholder} EXACTLY — same names, same ICU types and arms; \
(2) do not translate tokens listed in the glossary; \
(3) voice cues are short and imperative; UI labels are neutral; \
(4) output STRICTLY a JSON object with the schema described below — no prose, no code fences.";

    let mut user = String::new();
    user.push_str(&format!("Target locale: {locale}\n"));
    if let Some(variant) = locale_option.and_then(|o| o.translator_variant.as_deref()) {
        // For locales with multiple major variants (Spanish, Portuguese,
        // Chinese, Arabic, …) the catalog uses the bare ISO 639-1 code,
        // and this hint pins the variant we want — typically the most-
        // spoken one. See `localeOptions` in catalog.config.json.
        user.push_str(&format!("Variant: {variant}\n"));
    }
    if !glossary.do_not_translate.is_empty() {
        user.push_str(&format!(
            "Glossary (do not translate): {}\n",
            glossary.do_not_translate.join(", ")
        ));
    }
    user.push_str("Translate each `source` to the target locale. Return JSON of shape:\n");
    user.push_str(r#"{"translations":[{"key":"...","value":"..."}]}"#);
    user.push_str("\n\nEntries:\n");
    let mut payload = Vec::new();
    for key in keys {
        let en_entry = en
            .entries
            .get(key)
            .ok_or_else(|| format!("missing EN entry for `{key}`"))?;
        payload.push(serde_json::json!({
            "key": key,
            "source": en_entry.value,
            "description": en_entry.description,
        }));
    }
    user.push_str(&serde_json::to_string_pretty(&payload).unwrap());

    let req = OpenAiRequest {
        model: &cfg.model,
        messages: vec![
            OpenAiMessage { role: "system", content: system.to_owned() },
            OpenAiMessage { role: "user", content: user },
        ],
        response_format: serde_json::json!({ "type": "json_object" }),
        temperature: 0.1,
    };

    let resp: OpenAiResponse = ureq::post(&cfg.endpoint)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_json(serde_json::to_value(&req).unwrap())
        .map_err(|e| format!("OpenAI request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("decoding OpenAI response: {e}"))?;
    let content = resp
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| "OpenAI response had no choices".to_owned())?
        .message
        .content;

    #[derive(Deserialize)]
    struct Wrapper {
        translations: Vec<Pair>,
    }
    #[derive(Deserialize)]
    struct Pair {
        key: String,
        value: String,
    }
    let wrapper: Wrapper = serde_json::from_str(&content)
        .map_err(|e| format!("OpenAI returned non-JSON content: {e}\n--raw--\n{content}"))?;
    Ok(wrapper.translations.into_iter().map(|p| (p.key, p.value)).collect())
}

fn validate_placeholders(en_value: &str, translated: &str, key: &str) -> Result<(), String> {
    let en_set = extract_placeholder_names(en_value);
    let tr_set = extract_placeholder_names(translated);
    if en_set != tr_set {
        return Err(format!(
            "translation for `{key}` has mismatched placeholders: EN {:?} vs TR {:?}",
            en_set, tr_set
        ));
    }
    Ok(())
}

/// Extract the *names* of top-level ICU placeholders from a MessageFormat
/// string, ignoring the type/arms inside `{}`. We only care that the same
/// set of names is used in the translation.
fn extract_placeholder_names(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut depth = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if depth == 0 {
                // Read identifier until ',' or '}' or whitespace.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < chars.len()
                    && (chars[j].is_alphanumeric() || chars[j] == '_')
                {
                    j += 1;
                }
                if j > start {
                    let name: String = chars[start..j].iter().collect();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            depth += 1;
            i += 1;
        } else if chars[i] == '}' {
            if depth > 0 {
                depth -= 1;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    names.sort();
    names
}

// -----------------------------------------------------------------------------
// loading / saving
// -----------------------------------------------------------------------------

/// Tolerant `.env` loader. Reads `<root>/.env` line-by-line and sets each
/// `KEY=value` pair via `std::env::set_var`, but only if the variable is
/// not already present in the environment — so an explicit shell export
/// always wins over the file. Silently no-ops when the file is absent;
/// the goal is convenience, not enforcement.
///
/// Supports a deliberately tiny dialect: ignores blank lines and
/// comments (`#`), strips matching surrounding single or double quotes
/// from the value, and tolerates `export KEY=value` for users coming
/// from shell-style files. Anything fancier (multi-line values, variable
/// interpolation) is intentionally not supported.
fn load_dotenv(root: &Path) {
    let path = root.join(".env");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            value = &value[1..value.len() - 1];
        }
        if std::env::var_os(key).is_none() {
            // Safe in this CLI: we run before any threads spawn (ureq's
            // blocking client uses the calling thread), so the
            // historically-unsafe-for-multithreaded `set_var` is benign
            // here. If we ever switch to async/tokio, move this to a
            // proper crate.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

fn load_config(root: &Path) -> Result<CatalogConfig, String> {
    let path = root.join(CATALOG_CONFIG);
    let text =
        fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn load_glossary(root: &Path) -> Result<Glossary, String> {
    let path = root.join(GLOSSARY);
    let text =
        fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn load_catalog(root: &Path, locale: &str) -> Result<Catalog, String> {
    let path = root.join(LOCALES_DIR).join(format!("{locale}.json"));
    let text =
        fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn load_catalog_or_empty(root: &Path, locale: &str) -> Catalog {
    load_catalog(root, locale).unwrap_or(Catalog {
        locale: locale.to_owned(),
        entries: BTreeMap::new(),
    })
}

fn write_catalog(root: &Path, locale: &str, catalog: &Catalog) -> Result<(), String> {
    let path = root.join(LOCALES_DIR).join(format!("{locale}.json"));
    let text = serde_json::to_string_pretty(catalog)
        .map_err(|e| format!("serialize {}: {e}", path.display()))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

fn source_hash(entry: &Entry) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entry.value.as_bytes());
    hasher.update(b"\0");
    hasher.update(entry.description.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{}", &hex[..16])
}

fn iso8601_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    // Convert epoch seconds to YYYY-MM-DDTHH:MM:SSZ. Hand-rolled to avoid a
    // chrono dep — exact-ish accuracy is fine for a translation timestamp.
    let (y, mo, d, h, mi, s) = epoch_to_components(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Convert Unix epoch seconds (UTC) to (year, month, day, hour, minute,
/// second). Handles 1970-01-01 onwards; leap years computed Gregorian.
fn epoch_to_components(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let s = secs.rem_euclid(60) as u32;
    let m = (secs.div_euclid(60)).rem_euclid(60) as u32;
    let h = (secs.div_euclid(3600)).rem_euclid(24) as u32;
    let mut days = secs.div_euclid(86400);
    let mut year = 1970i32;
    loop {
        let yd = if is_leap(year) { 366 } else { 365 };
        if days < yd as i64 {
            break;
        }
        days -= yd as i64;
        year += 1;
    }
    let month_lengths = [
        31u32,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    let mut remaining = days as u32;
    for ml in month_lengths {
        if remaining < ml {
            break;
        }
        remaining -= ml;
        month += 1;
    }
    (year, month, remaining + 1, h, m, s)
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_placeholders_ignoring_arms() {
        let input =
            "In {distance, number} {distanceUnit, select, meters {meters} feet {feet} other {meters}}, turn left";
        assert_eq!(
            extract_placeholder_names(input),
            vec!["distance".to_owned(), "distanceUnit".to_owned()]
        );
    }

    #[test]
    fn extracts_placeholders_from_simple_template() {
        assert_eq!(
            extract_placeholder_names("{minutes, number} min"),
            vec!["minutes".to_owned()]
        );
    }

    #[test]
    fn placeholder_validation_passes_when_sets_match() {
        let r = validate_placeholders(
            "In {distance, number} meters, turn left",
            "{distance, number} metrin päässä, käänny vasemmalle",
            "cue.turn50m.left",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn placeholder_validation_fails_when_a_placeholder_is_dropped() {
        let r = validate_placeholders(
            "In {distance, number} meters, turn left",
            "Käänny vasemmalle",
            "cue.turn50m.left",
        );
        assert!(r.is_err());
    }

    #[test]
    fn epoch_to_components_handles_known_dates() {
        // 2026-04-30T00:00:00Z = 1777507200 (verified against `date -u -j -f
        // '%Y-%m-%dT%H:%M:%SZ' '2026-04-30T00:00:00Z' +%s`).
        let (y, mo, d, h, mi, s) = epoch_to_components(1_777_507_200);
        assert_eq!((y, mo, d, h, mi, s), (2026, 4, 30, 0, 0, 0));
        // Spot-check a non-midnight stamp.
        let (y, mo, d, h, mi, s) = epoch_to_components(1_704_067_200);
        assert_eq!((y, mo, d, h, mi, s), (2024, 1, 1, 0, 0, 0));
    }

    #[test]
    fn source_hash_is_stable_across_unrelated_fields() {
        let e1 = Entry {
            value: "Route started".to_owned(),
            description: "Voice cue.".to_owned(),
            placeholders: BTreeMap::new(),
            source_hash: String::new(),
            status: None,
            translated_at: None,
            translated_by: None,
        };
        let mut e2 = e1.clone();
        e2.translated_at = Some("2026-01-01T00:00:00Z".to_owned());
        e2.status = Some(EntryStatus::Reviewed);
        assert_eq!(source_hash(&e1), source_hash(&e2));
    }

    #[test]
    fn source_hash_changes_when_value_changes() {
        let e1 = Entry {
            value: "Route started".to_owned(),
            description: "x".to_owned(),
            placeholders: BTreeMap::new(),
            source_hash: String::new(),
            status: None,
            translated_at: None,
            translated_by: None,
        };
        let mut e2 = e1.clone();
        e2.value = "Route begun".to_owned();
        assert_ne!(source_hash(&e1), source_hash(&e2));
    }
}
