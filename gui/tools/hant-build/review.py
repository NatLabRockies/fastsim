"""Step 1: dump the s2twp conversion of every string for human review."""

import json
from pathlib import Path

import opencc

HERE = Path(__file__).parent
TW = opencc.OpenCC("s2twp")

# frontend i18n zh column
import re
i18n = Path(r"C:\Users\Pok fun\FASTSim\gui\frontend\i18n.js").read_text(encoding="utf-8")


def js_dict_pairs(text: str) -> list[tuple[str, str, str]]:
    """Extract (key, en, zh) triples from the DICT literal in i18n.js."""
    start = text.index("var DICT = {") + len("var DICT = {")
    end = text.index("\n  };", start)
    body = text[start:end]
    out = []
    i = 0
    n = len(body)
    while i < n:
        m = re.compile(r"'([^'\\]*)'\s*:\s*\[").search(body, i)
        if not m:
            break
        key = m.group(1)
        j = m.end()
        items = []
        cur = ""
        while j < n and body[j] != "]":
            ch = body[j]
            if ch == "'":
                k = j + 1
                while k < n and body[k] != "'":
                    if body[k] == "\\":
                        k += 1
                    k += 1
                items.append(body[j + 1:k])
                j = k + 1
                continue
            j += 1
        out.append((key, items[0], items[1]))
        i = j + 1
    return out


pairs = js_dict_pairs(i18n)
print("i18n keys:", len(pairs))
corpus = json.loads((HERE / "corpus.json").read_text(encoding="utf-8"))

lines = []
lines.append("# review: simplified -> s2twp")
lines.append("")
lines.append("## frontend i18n (zh column)")
for key, en, zh in pairs:
    lines.append(f"{key}\n  {zh}\n  {TW.convert(zh)}")
lines.append("")
lines.append("## backend strings")
for item in corpus["strings"]:
    lines.append(f"[{item['kind']}]\n  {item['text']}\n  {TW.convert(item['text'])}")

(HERE / "review-s2twp.txt").write_text("\n".join(lines), encoding="utf-8")
print("wrote review-s2twp.txt")
