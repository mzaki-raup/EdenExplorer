#!/usr/bin/env python3
"""Cross-references i18n.tr("key") calls in src/ against locales/*/main.ftl.

Three checks:
  1. Keys used in Rust code but missing from en-US (the base locale) - a
     real bug, Fluent falls back to showing the raw key string to the user.
  2. Keys present in en-US but missing from one or more OTHER locales - not
     a crash, but a silent untranslated string (Fluent falls back to
     en-US's text, or the key, with no build error either way).
  3. Keys defined in en-US but never referenced anywhere in src/ - probably
     dead, worth a second look before deleting (could be reached via a
     dynamic .tr(some_variable) call this script can't resolve statically -
     see the "dynamic call sites" section of the output).

Exit code is 0 if there are no missing-key problems (checks 1 and 2 clean),
2 if there are, 1 on a usage/environment error - so it's safe to wire into
a pre-commit hook or CI step later if that's ever wanted, not just run by
hand.
"""

import argparse
import re
import sys
from pathlib import Path

KEY_RE_FTL = re.compile(r"^([A-Za-z0-9_-]+)\s*=")
# Static string-literal calls: i18n.tr("some_key")
KEY_RE_CODE_STATIC = re.compile(r'\.tr\(\s*"([A-Za-z0-9_-]+)"\s*\)')
# Anything else passed to .tr(...) that isn't a plain string literal - can't
# be resolved without evaluating Rust, so these are reported separately for
# a human to eyeball rather than silently ignored.
KEY_RE_CODE_DYNAMIC = re.compile(r'\.tr\(\s*([^")][^)]*)\)')


def parse_ftl_keys(path: Path) -> set[str]:
    keys = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        m = KEY_RE_FTL.match(line)
        if m:
            keys.add(m.group(1))
    return keys


def find_code_calls(src_dir: Path) -> tuple[set[str], list[tuple[str, str]]]:
    static_keys: set[str] = set()
    dynamic_sites: list[tuple[str, str]] = []
    for rs_file in src_dir.rglob("*.rs"):
        text = rs_file.read_text(encoding="utf-8", errors="replace")
        for m in KEY_RE_CODE_STATIC.finditer(text):
            static_keys.add(m.group(1))
        # Find dynamic call sites, but skip ones that were already matched
        # as static (a static call also matches the looser dynamic regex).
        for m in KEY_RE_CODE_DYNAMIC.finditer(text):
            arg = m.group(1).strip()
            if arg.startswith('"'):
                continue
            line_no = text.count("\n", 0, m.start()) + 1
            dynamic_sites.append((f"{rs_file.relative_to(src_dir.parent)}:{line_no}", arg))
    return static_keys, dynamic_sites


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[4],
        help="Repo root (default: walks up from this script's location)",
    )
    parser.add_argument(
        "--base-locale",
        default="en-US",
        help="Locale treated as the source of truth for 'what keys should exist' (default: en-US)",
    )
    args = parser.parse_args()

    locales_dir = args.repo_root / "locales"
    src_dir = args.repo_root / "src"
    if not locales_dir.is_dir() or not src_dir.is_dir():
        print(f"ERROR: expected {locales_dir} and {src_dir} to exist - "
              f"pass --repo-root if this script moved relative to the repo.", file=sys.stderr)
        return 1

    locale_keys: dict[str, set[str]] = {}
    for locale_dir in sorted(locales_dir.iterdir()):
        ftl = locale_dir / "main.ftl"
        if ftl.is_file():
            locale_keys[locale_dir.name] = parse_ftl_keys(ftl)

    if args.base_locale not in locale_keys:
        print(f"ERROR: base locale '{args.base_locale}' not found under {locales_dir}", file=sys.stderr)
        return 1

    base_keys = locale_keys[args.base_locale]
    code_keys, dynamic_sites = find_code_calls(src_dir)

    problems = 0

    missing_from_base = sorted(code_keys - base_keys)
    if missing_from_base:
        problems += len(missing_from_base)
        print(f"MISSING FROM {args.base_locale} (used in code, will show the raw key to users):")
        for k in missing_from_base:
            print(f"  - {k}")
        print()

    for locale, keys in locale_keys.items():
        if locale == args.base_locale:
            continue
        missing = sorted(base_keys - keys)
        if missing:
            problems += len(missing)
            print(f"MISSING FROM {locale} (present in {args.base_locale}, untranslated - silently falls back):")
            for k in missing:
                print(f"  - {k}")
            print()

    unused = sorted(base_keys - code_keys)
    if unused:
        print(f"POSSIBLY UNUSED (in {args.base_locale}, no static .tr(\"...\") call found - "
              f"check the dynamic call sites below before deleting):")
        for k in unused:
            print(f"  - {k}")
        print()

    if dynamic_sites:
        print("DYNAMIC .tr(...) CALL SITES (not statically checkable - review by hand "
              "if a key above might actually be reached through one of these):")
        for loc, arg in dynamic_sites:
            print(f"  - {loc}: .tr({arg})")
        print()

    if problems == 0:
        print(f"OK - every .tr(\"...\") key used in code exists in all {len(locale_keys)} locales.")
        return 0
    else:
        print(f"{problems} missing-key problem(s) found across {len(locale_keys)} locales.")
        return 2


if __name__ == "__main__":
    sys.exit(main())
