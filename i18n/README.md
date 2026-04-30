# Companion app i18n catalog

This directory holds the **canonical** strings for all three companion apps
(web, iOS, Android). The platforms each consume a generated form (JSON for
web, `Localizable.xcstrings` for iOS, `res/values-<locale>/strings.xml` for
Android).

## Files

- `catalog.config.json` — locale list, OpenAI model, output paths.
- `glossary.json` — terms the OpenAI translator must leave untranslated.
- `schema.json` — JSON Schema for the per-locale catalog files.
- `locales/<locale>.json` — per-locale catalog. `en.json` is authored by hand;
  other locales are populated by `cargo xtask i18n-sync` and reviewed by humans.
- `parity/cue-en-snapshots.json` — generated EN snapshots of every `CueEvent`
  shape, used by all three apps' parity tests as a single source of truth.

## Workflow

### Adding a new key

1. Add the entry to `i18n/locales/en.json`.
2. Run `cargo xtask i18n-gen` to regenerate platform outputs.
3. Run `OPENAI_API_KEY=... cargo xtask i18n-sync --locale fi` to fill in
   missing translations. Review the diff in `i18n/locales/fi.json`. For any
   entry you're confident in, change `status` from `"ai-translated"` to
   `"reviewed"` so future syncs don't overwrite your work.
4. Run `cargo xtask i18n-gen` again so the platform outputs include the
   new translations.
5. Commit `i18n/locales/*.json` along with the regenerated platform files.

### Editing an existing translation

Open `i18n/locales/<locale>.json`, edit the `value`, set the entry's `status`
to `"reviewed"` (or `"locked"` to freeze even when the English source
changes). Re-run `cargo xtask i18n-gen` and commit.

### Status semantics

- `missing` — key is absent from this locale (implicit; no entry written).
- `ai-translated` — produced by `i18n-sync`. Re-translated when the English
  source changes, or when sync is run without a hash match.
- `reviewed` — a human has approved the translation. Sync skips this entry
  *unless* the English source's hash has drifted (then it re-translates and
  resets to `ai-translated`).
- `locked` — sync **never** modifies. Use for hand-tuned strings that should
  remain stable across English-source changes.

### Changing the OpenAI model

Edit `i18n/catalog.config.json` → `openai.model`. No code change needed.

## Conventions

- Keys use dotted hierarchy: `cue.turn50m.left`, `settings.activity.audioCues.title`.
- Voice cues live under `cue.*` and tend to be short imperative phrases.
- UI strings live under `ui.*`, `settings.*`, `home.*`.
- Format placeholders use ICU MessageFormat: `{distance, number}`,
  `{distanceUnit, select, meters {meters} feet {feet} other {meters}}`.
- `description` is mandatory for any key that has placeholders, and recommended
  for any key whose meaning is not self-evident from the key alone — the
  description is included in the OpenAI prompt as translator context.
