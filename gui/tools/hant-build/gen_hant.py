"""Generate gui/frontend/hant.js — the runtime Simplified -> Traditional map.

The frontend needs to render API-supplied Chinese (parameter descriptions,
group/subgroup names, KPI labels, cycle names) in Traditional Chinese without
asking the backend for a second variant.  This generator derives a compact
character map plus a term table from opencc, then *proves* completeness: every
string the backend can put on screen must convert to exactly the same text that
the build-time pipeline (opencc s2twp + hand fixes) produced.

    python gen_hant.py            # write hant.js + report
    python gen_hant.py --check    # verify only
"""

from __future__ import annotations

import argparse

import json
import re
from pathlib import Path

import opencc

import build  # to_hant() / parse_dict() / PHRASES / CHAR_KEEP

HERE = Path(__file__).parent
FRONTEND = Path(r"C:\Users\Pok fun\FASTSim\gui\frontend")
HANT_JS = FRONTEND / "hant.js"

_TW = opencc.OpenCC("s2tw")
CJK = re.compile(r"[\u3400-\u9fff]")

# Characters that keep their form even though a naive table would swap them, plus
# the genuinely ambiguous ones (制/里/系/借/面 …) whose reading depends on the
# word — for those the term table must decide, never the char map.
CHAR_KEEP = set(build.CHAR_KEEP) | set("制里系借面板松谷蒙旋")

# seeds for the runtime term table (the build-time pipeline's own vocabulary fixes)
SEED_PHRASES = dict(build.PHRASES)
SEED_PHRASES.update({
    "本源": "此來源",
    "连接": "連線",
    "搜索": "搜尋",
    "关系": "關係",
    "制造": "製造",
    "绘制": "繪製",
    "复制": "複製",
    "存储": "儲存",
    "公里": "公里",
})

EXACT = dict(build.EXACT_OVERRIDES)


def charmap_for(chars: set[str]) -> dict[str, str]:
    out = {}
    for ch in sorted(chars):
        if ch in CHAR_KEEP:
            continue
        conv = _TW.convert(ch)
        if conv != ch and len(conv) == 1:
            out[ch] = conv
    return out


def apply_conv(text: str, charmap: dict[str, str], phrases: dict[str, str],
               exact: dict[str, str] | None = None) -> str:
    """Same algorithm as hant.js: exact table, then term table, then chars."""
    if not text:
        return text
    if exact and text in exact:
        return exact[text]
    longest = max((len(k) for k in phrases), default=0)
    out = []
    i = 0
    n = len(text)
    while i < n:
        hit = None
        for length in range(min(longest, n - i), 0, -1):
            cand = text[i:i + length]
            if cand in phrases:
                hit = (length, phrases[cand])
                break
        if hit:
            out.append(hit[1])
            i += hit[0]
        else:
            out.append(charmap.get(text[i], text[i]))
            i += 1
    return "".join(out)


def collect_corpus() -> list[str]:
    """Every Simplified string the UI can display, from both layers."""
    strings = set()
    entries, _s, _e = build.parse_dict((FRONTEND / "i18n.js").read_text(encoding="utf-8"))
    for e in entries:
        strings.add(e["items"][1])
    corpus = json.loads((HERE / "corpus.json").read_text(encoding="utf-8"))
    for item in corpus["strings"]:
        strings.add(item["text"])
    # app.js KPI fallback labels (hard-coded Simplified)
    app = (FRONTEND / "app.js").read_text(encoding="utf-8")
    for m in re.finditer(r"\['[a-z_]+', '(?:[^'\\]|\\.)*', '((?:[^'\\]|\\.)*)',", app):
        strings.add(m.group(1))
    return sorted(s for s in strings if s and CJK.search(s))


def derive(texts: list[str]):
    chars = {ch for t in texts for ch in t if CJK.match(ch)}
    charmap = charmap_for(chars)
    phrases = dict(SEED_PHRASES)
    targets = {t: build.to_hant(t) for t in texts}

    for round_no in range(1, 12):
        bad = []
        for t in texts:
            got = apply_conv(t, charmap, phrases, EXACT)
            if got != targets[t]:
                bad.append((t, got, targets[t]))
        if not bad:
            print(f"[derive] converged after {round_no - 1} repair round(s); "
                  f"phrases={len(phrases)} chars={len(charmap)}")
            return charmap, phrases, targets, []
        for t, got, want in bad:
            charwise = "".join(charmap.get(c, c) for c in t)
            if len(charwise) != len(t):
                raise SystemExit("charmap value is not a single character")
            # one rule per string: the span between the common prefix and the
            # common suffix (this also covers 存储 -> 儲存 transpositions, where
            # a character-level diff would produce an empty source span).
            head = 0
            limit = min(len(charwise), len(want))
            while head < limit and charwise[head] == want[head]:
                head += 1
            tail = 0
            while (tail < limit - head
                   and charwise[len(charwise) - 1 - tail] == want[len(want) - 1 - tail]):
                tail += 1
            src = t[head:len(t) - tail]
            dst = want[head:len(want) - tail]
            if not src:
                raise SystemExit(f"cannot derive a rule for {t!r} ({got!r} vs {want!r})")
            # widen short spans into a real term (a single-character rule would
            # fire everywhere that character appears, e.g. 制 -> 製)
            if len(src) < 3 and len(charwise) == len(want):
                lo = head
                hi = len(t) - tail
                while hi - lo < min(3, len(t)) and (lo > 0 or hi < len(t)):
                    if lo > 0:
                        lo -= 1
                    if hi < len(t) and hi - lo < min(3, len(t)):
                        hi += 1
                src = t[lo:hi]
                dst = want[lo:hi]
            phrases[src] = dst
        print(f"[derive] round {round_no}: repaired {len(bad)} string(s)")
    raise SystemExit("did not converge")


def js_string(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def render_js(charmap: dict[str, str], phrases: dict[str, str]) -> str:
    for k, v in list(charmap.items()) + list(phrases.items()) + list(EXACT.items()):
        if "|" in k or "|" in v or "=" in k or "=" in v:
            raise SystemExit(f"separator clash in {k!r} -> {v!r}")

    char_entries = [f"{src}{dst}" for src, dst in sorted(charmap.items())]
    # pack into lines of ~76 characters, one entry is 2 characters wide
    lines = []
    for i in range(0, len(char_entries), 38):
        lines.append("    " + js_string("".join(char_entries[i:i + 38])) + ",")
    char_block = "\n".join(lines)

    phrase_items = sorted(phrases.items(), key=lambda kv: (-len(kv[0]), kv[0]))
    phrase_lines = []
    for i in range(0, len(phrase_items), 6):
        chunk = phrase_items[i:i + 6]
        phrase_lines.append("    " + js_string("|".join(f"{k}={v}" for k, v in chunk)) + ",")
    phrase_block = "\n".join(phrase_lines)
    longest = max(len(k) for k in phrases)

    exact_lines = []
    for k, v in sorted(EXACT.items()):
        exact_lines.append(f"    {js_string(k)}: {js_string(v)},")
    exact_block = "\n".join(exact_lines)

    return f"""/* ==========================================================================
   hant.js — Simplified -> Traditional Chinese (Taiwan / Hong Kong usage).
   GENERATED FILE — do not hand-edit; regenerate with the build pipeline.
   Source: opencc `s2twp` (Taiwan idioms) + hand-reviewed vocabulary fixes.

   Why it exists: the API ships Simplified Chinese (`description_zh`,
   `group_zh`, `subgroup_zh`, `label_zh`, `name_zh`).  The UI has to render
   those in Traditional Chinese when the user picks 繁體, and it does so here,
   on the client, so language switching stays instantaneous and the backend
   payload for `en`/`zh` never changes.

   Two tables, applied left to right:
     EXACT    a handful of labels whose whole string is a different word
              (列 -> 欄位); they must never fire inside a longer word
     PHRASES  whole terms that are *re-worded*, not just re-glyphed
              (数据 -> 資料, 仿真 -> 模擬, 覆盖 -> 覆寫, 参数 -> 參數 …)
     CHARS    single-character glyph conversion for everything else
   ========================================================================== */
(function (global) {{
  'use strict';

  /* exact whole-string labels */
  var EXACT = {{
{exact_block}
  }};

  /* "src=dst|src=dst|…" — keyed on Simplified input, longest first. */
  var PHRASE_DATA = [
{phrase_block}
  ].join('|');

  /* "src dst" pairs, 2 characters each, packed into a string. */
  var CHAR_DATA = [
{char_block}
  ].join('');

  var MAX_PHRASE = {longest};

  var PHRASES = (function (raw) {{
    var map = {{}};
    if (!raw) return map;
    var parts = raw.split('|');
    for (var i = 0; i < parts.length; i++) {{
      var at = parts[i].indexOf('=');
      if (at > 0) map[parts[i].slice(0, at)] = parts[i].slice(at + 1);
    }}
    return map;
  }})(PHRASE_DATA);

  var CHARS = (function (raw) {{
    var map = {{}};
    for (var i = 0; i + 1 < raw.length; i += 2) map[raw.charAt(i)] = raw.charAt(i + 1);
    return map;
  }})(CHAR_DATA);

  var cache = {{}};
  var cacheSize = 0;
  var CACHE_LIMIT = 2000;

  /** Convert one Simplified string.  Anything that is not a string is returned as-is. */
  function toHant(text) {{
    if (typeof text !== 'string' || text === '') return text;
    var hit = cache[text];
    if (hit !== undefined) return hit;
    if (Object.prototype.hasOwnProperty.call(EXACT, text)) return EXACT[text];

    var out = [];
    var i = 0;
    var n = text.length;
    while (i < n) {{
      var used = 0;
      var repl = null;
      for (var len = MAX_PHRASE; len > 0; len--) {{
        if (i + len > n) continue;
        var cand = text.substr(i, len);
        if (Object.prototype.hasOwnProperty.call(PHRASES, cand)) {{
          used = len;
          repl = PHRASES[cand];
          break;
        }}
      }}
      if (used) {{
        out.push(repl);
        i += used;
      }} else {{
        var ch = text.charAt(i);
        var mapped = CHARS[ch];
        out.push(mapped === undefined ? ch : mapped);
        i += 1;
      }}
    }}
    var result = out.join('');
    if (cacheSize < CACHE_LIMIT) {{
      cache[text] = result;
      cacheSize += 1;
    }}
    return result;
  }}

  global.FSHant = {{
    toHant: toHant,
    charCount: Object.keys(CHARS).length,
    phraseCount: Object.keys(PHRASES).length
  }};
}})(typeof window !== 'undefined' ? window : this);
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    texts = collect_corpus()
    print(f"[corpus] {len(texts)} distinct Chinese strings")
    charmap, phrases, targets, _ = derive(texts)

    source = render_js(charmap, phrases)
    if args.check:
        if HANT_JS.exists() and HANT_JS.read_text(encoding="utf-8") != source:
            raise SystemExit("hant.js is out of date")
        print("[check] hant.js is up to date")
    else:
        HANT_JS.write_text(source, encoding="utf-8")
        print(f"[hant] wrote {HANT_JS.name} ({len(source)} bytes, "
              f"{len(charmap)} chars, {len(phrases)} phrases)")

    report = []
    for t in texts:
        report.append(f"{t}\n  {apply_conv(t, charmap, phrases)}")
    (HERE / "hant-targets.txt").write_text("\n".join(report), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
