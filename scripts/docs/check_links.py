#!/usr/bin/env python3
"""Check documentation-site source links, including bounded external reachability."""
from __future__ import annotations

import concurrent.futures
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "docs-site"
LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
ACCEPTED_EXTERNAL_STATUSES = {403, 405, 429}


def markdown_files() -> list[Path]:
    return sorted(
        path
        for path in SITE.rglob("*.md")
        if "node_modules" not in path.parts and ".vitepress" not in path.parts
    )


def internal_target(source: Path, raw: str) -> Path | None:
    target = unquote(urlsplit(raw).path)
    if not target or target == "/":
        return SITE / "index.md"
    base = SITE if target.startswith("/") else source.parent
    candidate = (base / target.lstrip("/")).resolve()
    if SITE.resolve() not in candidate.parents and candidate != SITE.resolve():
        return None
    options = [candidate]
    if candidate.is_dir():
        options.insert(0, candidate / "index.md")
    elif candidate.suffix == "":
        options.extend([candidate.with_suffix(".md"), candidate / "index.md"])
    elif candidate.suffix == ".html":
        options.append(candidate.with_suffix(".md"))
    return next((option for option in options if option.is_file()), None)


def check_external(url: str) -> str | None:
    request = urllib.request.Request(
        url,
        headers={
            "User-Agent": "Minco-Docs-Link-Checker/0.5.0",
            "Range": "bytes=0-0",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            if 200 <= response.status < 400:
                return None
            return f"{url}: HTTP {response.status}"
    except urllib.error.HTTPError as error:
        if error.code in ACCEPTED_EXTERNAL_STATUSES:
            return None
        return f"{url}: HTTP {error.code}"
    except (urllib.error.URLError, TimeoutError) as error:
        return f"{url}: {error}"


def main() -> int:
    failures: list[str] = []
    external: set[str] = set()
    checked_internal = 0
    for source in markdown_files():
        for match in LINK.finditer(source.read_text()):
            raw = match.group(1)
            if raw.startswith(("mailto:", "tel:")) or raw.startswith("#"):
                continue
            if raw.startswith(("https://", "http://")):
                external.add(raw)
                continue
            checked_internal += 1
            if internal_target(source, raw) is None:
                failures.append(f"{source.relative_to(ROOT)}: unresolved link {raw}")

    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        results = executor.map(check_external, sorted(external))
        failures.extend(result for result in results if result is not None)

    generated = sorted((SITE / ".vitepress" / "dist").rglob("*.html"))
    for page in generated:
        source = page.read_text()
        canonical = re.findall(
            r'<link rel="canonical" href="(https://xicv\.github\.io/minco/[^"]*)">',
            source,
        )
        if len(canonical) != 1:
            failures.append(
                f"{page.relative_to(ROOT)}: expected one production canonical link"
            )
    if not (SITE / ".vitepress" / "dist" / "sitemap.xml").is_file():
        failures.append("docs-site/.vitepress/dist/sitemap.xml: missing")

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(
        f"Documentation links passed: {checked_internal} internal, "
        f"{len(external)} external, {len(generated)} canonical pages."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
