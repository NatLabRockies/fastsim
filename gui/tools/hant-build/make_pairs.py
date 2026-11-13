"""Emit {simplified: expected_traditional} pairs for the JS converter check."""

import json
from pathlib import Path

import build
import gen_hant

HERE = Path(__file__).parent
texts = gen_hant.collect_corpus()
pairs = {t: build.to_hant(t) for t in texts}
(HERE / "pairs.json").write_text(json.dumps(pairs, ensure_ascii=False, indent=1), encoding="utf-8")
print(f"wrote pairs.json with {len(pairs)} pairs")
