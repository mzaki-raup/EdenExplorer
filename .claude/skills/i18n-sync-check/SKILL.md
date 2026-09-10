---
name: i18n-sync-check
description: Cross-references every i18n.tr("key") call in EdenExplorer's Rust source against all six locales/<lang>/main.ftl files and reports keys that are missing, untranslated, or possibly dead. Run this any time you add, rename, or remove a user-facing string / i18n key, or edit any locales/*/main.ftl file directly - a key missing from a non-English locale fails completely silently (no build error, no panic, just a fallback the user sees), so it's easy to ship an untranslated string without ever noticing. Also use it when asked to "check translations", "audit i18n", or "sync locales".
---

# i18n sync check

EdenExplorer's UI strings go through `i18n.tr("key")`, backed by Fluent
`.ftl` files - one per locale under `locales/<lang>/main.ftl`. There is no
build-time check that a key referenced in code actually exists in every
locale file; a typo'd or forgotten key just silently falls back (to the raw
key string, or to `en-US`'s text) with nothing in the build output to catch
it. This skill's script does that check for you.

## Running it

```bash
python .claude/skills/i18n-sync-check/scripts/check_i18n.py
```

No arguments needed for the normal case - it walks up from its own location
to find the repo root, reads `locales/*/main.ftl`, and scans every `*.rs`
file under `src/` for `.tr("...")` calls.

## Reading the output

It prints up to four sections, then a one-line summary:

- **MISSING FROM en-US** - a key used in code that doesn't exist in the base
  locale at all. This is the one that actually matters most: the user sees
  the raw key string (e.g. `some_button_label`) instead of real text. Fix by
  adding the key to `locales/en-US/main.ftl` (and every other locale, per
  the next section).
- **MISSING FROM `<locale>`** - a key that exists in `en-US` but not in one
  of the other five locales. Not a crash, just an untranslated string for
  that language. Add the key to that locale's `main.ftl` with a real
  translation - don't just copy the English text in as a placeholder unless
  you're going to come back and translate it, since that's indistinguishable
  from "already translated" to this script (and to a reviewer) afterward.
- **POSSIBLY UNUSED** - a key defined in `en-US` with no static
  `.tr("literal")` call found anywhere. Before deleting one of these, check
  the next section first - it might be reached through a *dynamic* call
  (`.tr(some_variable)`) that this script can't resolve without actually
  evaluating the Rust.
- **DYNAMIC .tr(...) CALL SITES** - every call where the argument to `.tr()`
  isn't a plain string literal (a variable holding a key computed elsewhere,
  or something like `TagColumn::Name.i18n_key()`). These are listed so a
  human can cross-reference them against the "possibly unused" list, not
  because they're inherently a problem.

Exit code is `0` when there are no missing-key problems (untranslated/
possibly-unused findings don't affect the exit code, only outright missing
keys do), `2` when there are missing-key problems, `1` on a usage error
(e.g. run from somewhere the repo layout can't be found).

## When a new key needs adding

Reuse an existing key instead of adding a near-duplicate when the English
text is already identical or close enough (e.g. the various
`explorer_cols_*` / `recycle_bin_delete_permanently` keys get reused across
views rather than each view having its own copy). When you do need a new
one, add it to all six locale files in the same change, then run this
script to confirm - don't rely on remembering to do all six by hand.
