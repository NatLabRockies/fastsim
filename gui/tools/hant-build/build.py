"""Build the Traditional-Chinese artifacts.

    python build.py            # rewrite i18n.js DICT column 3, write hant.js
    python build.py --check    # report only, change nothing

Pipeline for every Simplified string::

    opencc s2twp  ->  OUTPUT_FIXES (Taiwan/HK vocabulary, per-term)
                  ->  FULL_OVERRIDES (whole strings that must be re-worded)

s2twp is the right base converter (it already yields 執行/匯出/程式碼/資料/預設/
載入/互動檢視/向量), but it also carries a few mappings that are wrong for this
domain (参数 -> 引數, 图像 -> 影象) plus mainland vocabulary that is merely
glyph-converted (覆盖 -> 覆蓋 instead of 覆寫, 响应 -> 響應 instead of 回應,
重置 -> 重置 instead of 重設, 只读 -> 只讀 instead of 唯讀, 脚本 -> 指令碼
instead of 腳本).  Those are corrected by OUTPUT_FIXES.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

import opencc

HERE = Path(__file__).parent
FRONTEND = Path(r"C:\Users\Pok fun\FASTSim\gui\frontend")
I18N = FRONTEND / "i18n.js"

_TW = opencc.OpenCC("s2twp")
_S2T = opencc.OpenCC("s2t")

# --------------------------------------------------------------------------
# Hand-reviewed vocabulary corrections, applied to the converted output.
# Longest source term first so that a longer phrase always wins.
# --------------------------------------------------------------------------
OUTPUT_FIXES: tuple[tuple[str, str], ...] = (
    ("引數", "參數"),        # s2twp renders 参数 as the *argument* term — wrong here
    ("影象", "圖像"),        # s2twp renders 图像 as 影象 — wrong term
    ("工作臺", "工作台"),    # keep 台 (the common TW/HK glyph, and the product's own label)
    ("覆蓋", "覆寫"),        # override, not "to cover"
    ("響應", "回應"),        # response
    ("只讀", "唯讀"),        # read-only
    ("指令碼", "腳本"),      # script
    ("已應用", "已套用"),    # apply -> 套用
    ("重置", "重設"),        # reset -> 重設
    ("燃料型別", "燃料類型"),
    ("系數", "係數"),        # coefficient is 係數 in TW/HK, not 系數
    ("荷電", "電量"),        # State of Charge -> 電量狀態, not 荷電狀態
    ("返回", "回傳"),        # "returns a value" -> 回傳
)

# Same corrections expressed for the *runtime* converter (Simplified -> Traditional),
# i.e. keyed on the Simplified term and applied before character mapping.
PHRASES: dict[str, str] = {
    "工作台": "工作台",
    "引数": "參數",
    "参数": "參數",
    "图像": "圖像",
    "覆盖": "覆寫",
    "响应": "回應",
    "只读": "唯讀",
    "脚本": "腳本",
    "已应用": "已套用",
    "重置": "重設",
    "类型": "類型",
    "数据": "資料",
    "默认": "預設",
    "运行": "執行",
    "运行仿真": "執行模擬",
    "仿真": "模擬",
    "导出": "匯出",
    "导入": "匯入",
    "加载": "載入",
    "代码": "程式碼",
    "服务器": "伺服器",
    "视图": "檢視",
    "交互": "互動",
    "矢量": "向量",
    "分辨率": "解析度",
    "字段": "欄位",
    "点击": "點選",
    "回路": "迴路",
    "文件": "檔案",
    "设置": "設定",
    "信息": "資訊",
    "用户": "使用者",
    "数据表": "資料表",
    "网络": "網路",
    "系数": "係數",
    "荷电": "電量",
    "返回": "回傳",
}

# Whole-string re-wordings that no term table can do (keyed by the Simplified text).
FULL_OVERRIDES: dict[str, str] = {
    "无法连接服务器，后端是否已在本源运行？": "無法連線伺服器，後端是否已在此來源執行？",
    "可复现当前配置的独立脚本。": "可重現此設定的獨立腳本。",
    "结构性参数——仅作展示，不可覆盖。": "結構性參數——僅供顯示，不可覆寫。",
}

# Exact-string mappings: fire only when the whole input equals the key, so that a
# bare label like 列 (a table column header) does not leak into 序列 / 列表.
EXACT_OVERRIDES: dict[str, str] = {
    "列": "欄位",
}

# Characters that must keep their form even though a naive char table would swap
# them (台 is the everyday TW/HK glyph; 臺 only in fixed compounds).
CHAR_KEEP = set("台")


def fix(text: str) -> str:
    """Apply the hand-reviewed vocabulary corrections, longest term first."""
    if text in FULL_OVERRIDES:
        return FULL_OVERRIDES[text]
    for src, dst in sorted(OUTPUT_FIXES, key=lambda p: -len(p[0])):
        text = text.replace(src, dst)
    return text


def to_hant(zh: str) -> str:
    """Simplified Chinese -> Traditional Chinese (Taiwan/Hong Kong usage)."""
    if zh in FULL_OVERRIDES:
        return FULL_OVERRIDES[zh]
    if zh in EXACT_OVERRIDES:
        return EXACT_OVERRIDES[zh]
    return fix(_TW.convert(zh))


# --------------------------------------------------------------------------
# i18n.js DICT: parse, extend to three columns, write back
# --------------------------------------------------------------------------
_ENTRY = re.compile(r"'([^'\\]*)'\s*:\s*\[")


def parse_dict(text: str):
    """Return (entries, body_start, body_end).

    entries: list of dicts {key, items, blank_before, style}
    """
    start = text.index("var DICT = {") + len("var DICT = {")
    end = text.index("\n  };", start)
    body = text[start:end]
    entries = []
    i = 0
    prev_end = 0
    n = len(body)
    while True:
        m = _ENTRY.search(body, i)
        if not m:
            break
        blank_before = "\n\n" in body[prev_end:m.start()]
        j = m.end()
        items = []
        closed = None
        while j < n:
            ch = body[j]
            if ch == "'":
                k = j + 1
                while k < n and body[k] != "'":
                    k += 2 if body[k] == "\\" else 1
                items.append(body[j + 1:k])
                j = k + 1
                continue
            if ch == "]":
                closed = j
                break
            j += 1
        if closed is None:
            raise SystemExit(f"unterminated array for key {m.group(1)!r}")
        style = "single" if "\n" not in body[m.start():closed] else "multi"
        entries.append({"key": m.group(1), "items": items,
                        "blank_before": blank_before, "style": style})
        prev_end = closed + 1
        i = closed + 1
    return entries, start, end


def js_quote(s: str) -> str:
    return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"


def render_dict(entries, *, single_line_limit: int = 116) -> str:
    out = []
    for e in entries:
        if e["blank_before"]:
            out.append("")
        items = ", ".join(js_quote(v) for v in e["items"])
        one = f"    {js_quote(e['key'])}: [{items}],"
        if len(one) <= single_line_limit and e["style"] == "single":
            out.append(one)
        else:
            out.append(f"    {js_quote(e['key'])}: [")
            for idx, v in enumerate(e["items"]):
                tail = "," if idx < len(e["items"]) - 1 else ""
                out.append(f"      {js_quote(v)}{tail}")
            out.append("    ],")
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="report only, write nothing")
    args = ap.parse_args()

    text = I18N.read_text(encoding="utf-8")
    entries, start, end = parse_dict(text)
    print(f"[i18n] parsed {len(entries)} keys")

    missing = [e["key"] for e in entries if len(e["items"]) not in (2, 3)]
    if missing:
        raise SystemExit(f"keys without exactly 2 or 3 columns: {missing}")

    changes = []
    for e in entries:
        key = e["key"]
        zh = e["items"][1]
        hant = FULL_OVERRIDES.get(key) or to_hant(zh)
        e["items"] = [e["items"][0], zh, hant]
        if hant == zh and re.search(r"[\u4e00-\u9fff]", zh):
            changes.append(key)

    if changes:
        print(f"[warn] {len(changes)} keys converted to an identical string: {changes}")

    body = render_dict(entries)
    new_text = text[:start] + "\n" + body + "\n" + text[end:]
    if args.check:
        print("[check] i18n.js not written")
    else:
        I18N.write_text(new_text, encoding="utf-8")
        print(f"[i18n] rewrote {I18N.name} with a third column ({len(entries)} keys)")

    # a review sheet for the hand-review pass
    lines = []
    for e in entries:
        if e["items"][1] != e["items"][2]:
            lines.append(f"{e['key']}\n  {e['items'][1]}\n  {e['items'][2]}")
    (HERE / "review-final.txt").write_text("\n".join(lines), encoding="utf-8")
    print("[review] wrote review-final.txt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
