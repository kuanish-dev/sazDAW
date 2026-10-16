#!/usr/bin/env python3
"""Builds THIRD_PARTY_NOTICES.md: every third-party dependency of SazDAW (Rust workspace, frontend) with its
version, license and a link to its source.

Usage: `scripts/third_party_notices.py` rewrites the file; `--check` only checks that the file is up to date and that
no dependency with an unknown license has appeared (if one has, decide whether it may be distributed and extend
PERMISSIVE/COPYLEFT/NOTES). Why the list is generated rather than written by hand: there are more than five hundred
dependencies, and a hand-written list would go stale with the very first Cargo.lock update.
"""
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "THIRD_PARTY_NOTICES.md")

# Licenses we have reviewed: permissive ones need only the notice and the license text; the others get a separate
# explanation in NOTES.
PERMISSIVE = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "Unicode-DFS-2016",
              "CC0-1.0", "Unlicense", "BSL-1.0", "0BSD", "Apache-2.0 WITH LLVM-exception"}
COPYLEFT = {"MPL-2.0", "LGPL-3.0", "GPLv3", "GPL-3.0", "GPL-3.0-or-later"}

NOTES = """\
## Special licenses

- **LAME** (`mp3lame-sys`, `mp3lame-encoder`; MP3 encoding on export) — **LGPL-3.0**. The library is built from
  source and linked statically. LAME sources: <https://lame.sourceforge.io> and the `mp3lame-sys` package on
  crates.io. To build SazDAW with a modified LAME, replace the sources in `mp3lame-sys` (for example via `[patch]` in
  `Cargo.toml`) and rebuild the app: the source code of all of SazDAW is open (GPL-3.0-or-later). Texts: `licenses/LGPL-3.0.txt` and
  `licenses/GPL-3.0.txt`.
- **Symphonia** (`symphonia-*`; reading audio files) and a few other crates under **MPL-2.0** are used unmodified;
  their sources are available on crates.io via the links below. Text: `licenses/MPL-2.0.txt`.
- The VST3 test fixtures in `test-plugins/` (GPL-3.0-or-later, built on nih-plug) are used only by the host tests
  and are not distributed, so their dependencies are not listed here. SazDAW itself hosts plugins through other bindings
  (`vst3`, MIT/Apache-2.0).
- **Apache-2.0** — text in `licenses/Apache-2.0.txt`. The MIT, BSD, ISC, Zlib and Unicode licenses require keeping
  the copyright notice: it is in the sources of each package via the links below.
"""


def cargo_packages(manifest_dir):
    """Third-party packages of the Cargo workspace in `manifest_dir` (without its own crates)."""
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked"], cwd=manifest_dir,
                         check=True, capture_output=True, text=True).stdout
    meta = json.loads(out)
    own = set(meta["workspace_members"])
    rows = []
    for p in meta["packages"]:
        if p["id"] in own:
            continue
        source = p.get("repository") or f"https://crates.io/crates/{p['name']}"
        rows.append((p["name"], p["version"], (p.get("license") or "not specified").replace("/", " OR "), source))
    return sorted(set(rows))


def npm_packages(app_dir):
    """Packages that end up in the built interface: the `dependencies` and their dependencies."""
    with open(os.path.join(app_dir, "package.json"), encoding="utf-8") as f:
        stack = list(json.load(f).get("dependencies", {}))
    rows, seen = [], set()
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        seen.add(name)
        path = os.path.join(app_dir, "node_modules", name, "package.json")
        with open(path, encoding="utf-8") as f:
            pkg = json.load(f)
        repo = pkg.get("repository")
        repo = repo.get("url") if isinstance(repo, dict) else repo
        source = repo or f"https://www.npmjs.com/package/{name}"
        rows.append((name, pkg.get("version", "?"), str(pkg.get("license", "not specified")), source))
        stack += list(pkg.get("dependencies", {}))
    return sorted(rows)


def unknown_licenses(rows):
    """Licenses we haven't reviewed yet. "A OR B" is fine if at least one option is known; "A AND B" means both apply
    at once, so every part of an option must be known."""
    known = PERMISSIVE | COPYLEFT
    bad = []
    for name, _, license, _ in rows:
        options = [[part.strip("() ") for part in option.split(" AND ")] for option in license.split(" OR ")]
        if not any(all(part in known for part in option) for option in options):
            bad.append(f"{name}: {license}")
    return bad


def table(rows):
    lines = ["| Package | Version | License | Source |", "|---|---|---|---|"]
    lines += [f"| {n} | {v} | {lic} | {src} |" for n, v, lic, src in rows]
    return "\n".join(lines)


def render():
    sections = [
        ("SazDAW app (Rust, root `Cargo.lock`)", cargo_packages(ROOT)),
        ("Interface (npm, `app/package.json`)", npm_packages(os.path.join(ROOT, "app"))),
    ]
    bad = [b for _, rows in sections for b in unknown_licenses(rows)]
    text = ["# Third-party components", "",
            "SazDAW is distributed under the GPL-3.0-or-later license (`LICENSE`). Below are the third-party components included",
            "in the build and their licenses. This file is generated by `scripts/third_party_notices.py`.", "", NOTES]
    for title, rows in sections:
        text += [f"## {title}", "", f"Packages: {len(rows)}.", "", table(rows), ""]
    return "\n".join(text), bad


def main():
    text, bad = render()
    if bad:
        print("Unknown licenses — decide whether they may be distributed and extend PERMISSIVE/COPYLEFT/NOTES:")
        print("\n".join(f"  {b}" for b in bad))
        return 1
    if "--check" in sys.argv:
        try:
            with open(OUT, encoding="utf-8") as f:
                current = f.read()
        except FileNotFoundError:
            current = ""
        if current != text:
            print("THIRD_PARTY_NOTICES.md is out of date: run scripts/third_party_notices.py")
            return 1
        return 0
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(text)
    print(f"Wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
