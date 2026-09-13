#!/usr/bin/env python3
"""Build a standalone, offline explorer. No third-party dependencies."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent
html = (ROOT / "shell.html").read_text()
for marker, filename in {
    "APP_CSS": "app.css", "SYSTEM_JS": "system.js", "TYPES_JS": "types.js",
    "SCENARIOS_JS": "scenarios.js", "APP_JS": "app.js",
}.items():
    content = (ROOT / filename).read_text()
    if filename.endswith(".js"):
        content = content.replace("</script", "<\\/script")
    html = html.replace(f"/* {marker} */", content)
(ROOT / "index.html").write_text(html)
print(f"Built {ROOT / 'index.html'} ({len(html.encode()):,} bytes)")
