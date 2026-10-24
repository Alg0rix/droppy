#!/usr/bin/env python3
"""Docs lint: every relative link, image and #anchor in the Markdown files must resolve.

    python tools/check_docs.py

Checks README.md, ARCHITECTURE.md and docs/**/*.md. External links are not
fetched (CI shouldn't fail because a website is down).
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = [ROOT / "README.md", ROOT / "ARCHITECTURE.md", *sorted((ROOT / "docs").rglob("*.md"))]

MD_LINK = re.compile(r"!?\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
HTML_ATTR = re.compile(r"""(?:src|href)=["']([^"']+)["']""")
HEADING = re.compile(r"^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$")


def slug(heading: str) -> str:
    """GitHub-style anchor for a heading."""
    text = re.sub(r"<[^>]+>|`|\*|_", "", heading).strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors(path: Path) -> set:
    out, seen, fenced = set(), {}, False
    for line in path.read_text(encoding="utf8").splitlines():
        if line.lstrip().startswith("```"):
            fenced = not fenced
        m = None if fenced else HEADING.match(line)
        if m:
            s = slug(m.group(1))
            n = seen.get(s, 0)
            seen[s] = n + 1
            out.add(s if n == 0 else f"{s}-{n}")
    return out


def targets(path: Path):
    text = path.read_text(encoding="utf8")
    # Ignore fenced code blocks so shell snippets can't produce false links.
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    for m in list(MD_LINK.finditer(text)) + list(HTML_ATTR.finditer(text)):
        yield m.group(1)


def main() -> int:
    problems = []
    for md in FILES:
        if not md.exists():
            continue
        rel = md.relative_to(ROOT)
        for t in targets(md):
            if re.match(r"^[a-z][a-z0-9+.-]*:", t, re.I):  # http:, https:, mailto: ...
                continue
            path_part, _, frag = t.partition("#")
            path_part = path_part.split("?")[0]
            dest = md if not path_part else (md.parent / path_part).resolve()
            if path_part and not dest.exists():
                problems.append(f"{rel}: missing file  {t}")
                continue
            if frag and dest.suffix.lower() == ".md" and frag.lower() not in anchors(dest):
                problems.append(f"{rel}: missing anchor {t}")
    for p in problems:
        print(p)
    checked = ", ".join(str(m.relative_to(ROOT)) for m in FILES if m.exists())
    print(f"{'FAIL' if problems else 'ok'}: {len(problems)} problem(s) in {checked}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
