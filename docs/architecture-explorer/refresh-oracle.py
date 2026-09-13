#!/usr/bin/env python3
"""Regenerate the browser lab's oracle using the actual golden Rust library."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source-root", type=Path, default=root.parent.parent / ".worktrees/golden",
                    help="Golden checkout to execute, rather than the learner scaffold")
golden = parser.parse_args().source_root.resolve()
if not (golden / "src/model.rs").is_file():
    parser.error(f"Golden source not found: {golden}; use --source-root to select its checkout")
# Keep build products outside the learner tree. TemporaryDirectory removes only
# the temporary project it creates, after the completed probe exits.
with tempfile.TemporaryDirectory(prefix="switchboard-model-oracle-") as directory:
    project = Path(directory)
    manifest = (
        '[package]\nname = "switchboard-explorer-oracle"\n'
        'version = "0.0.0"\nedition = "2024"\n'
        '[dependencies]\nserde_json = "1"\n'
        f'switchboard = {{ package = "switchboard-rust", path = {json.dumps(str(golden))} }}\n'
        '[[bin]]\nname = "oracle"\n'
        f'path = {json.dumps(str(root / "verify-model.rs"))}\n'
    )
    (project / "Cargo.toml").write_text(manifest)
    result = subprocess.run(
        ["cargo", "run", "--offline", "--quiet", "--manifest-path", str(project / "Cargo.toml")],
        check=True, text=True, capture_output=True,
    )
    oracle = json.loads(result.stdout)
    (root / "model-parity.json").write_text(json.dumps(oracle, indent=2) + "\n")
    print(f"Generated {len(oracle['phaseCases'])} phase cases and {len(oracle['shownCases'])} display cases.")
