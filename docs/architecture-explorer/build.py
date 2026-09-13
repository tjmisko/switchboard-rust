#!/usr/bin/env python3
"""Build a standalone, offline explorer. No third-party dependencies."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source-root", type=Path, default=ROOT.parent.parent / ".worktrees/golden",
                    help="Golden checkout used by the source links")
source_root = parser.parse_args().source_root.resolve()
if not (source_root / "src/model.rs").is_file():
    parser.error(f"Golden source not found: {source_root}; use --source-root to select its checkout")
html = (ROOT / "shell.html").read_text()
for marker, filename in {
    "APP_CSS": "app.css", "SYSTEM_JS": "system.js", "TYPES_JS": "types.js",
    "SCENARIOS_JS": "scenarios.js", "APP_JS": "app.js",
}.items():
    content = (ROOT / filename).read_text()
    if filename == "app.js":
        content = f"window.SB_SOURCE_BASE = {json.dumps(source_root.as_uri() + '/')};\n" + content
    if filename.endswith(".js"):
        content = content.replace("</script", "<\\/script")
    html = html.replace(f"/* {marker} */", content)
(ROOT / "index.html").write_text(html)
print(f"Built {ROOT / 'index.html'} ({len(html.encode()):,} bytes)")
