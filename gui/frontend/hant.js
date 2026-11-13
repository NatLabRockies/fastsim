/* ==========================================================================
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
(function (global) {
  'use strict';

  /* exact whole-string labels */
  var EXACT = {
    "列": "欄位",
  };

  /* "src=dst|src=dst|…" — keyed on Simplified input, longest first. */
  var PHRASE_DATA = [
    "作展示，不可覆盖=供顯示，不可覆寫|复现当前配置=重現此設定|运行仿真=執行模擬|分辨率=解析度|工作台=工作台|已应用=已套用",
    "数据表=資料表|服务器=伺服器|交互=互動|代码=程式碼|仿真=模擬|信息=資訊",
    "公里=公里|关系=關係|制造=製造|加载=載入|参数=參數|只读=唯讀",
    "响应=回應|回路=迴路|图像=圖像|复制=複製|字段=欄位|存储=儲存",
    "导入=匯入|导出=匯出|引数=參數|搜索=搜尋|数据=資料|文件=檔案",
    "本源=此來源|点击=點選|用户=使用者|矢量=向量|类型=類型|系数=係數",
    "绘制=繪製|网络=網路|脚本=腳本|荷电=電量|覆盖=覆寫|视图=檢視",
    "设置=設定|运行=執行|返回=回傳|连接=連線|重置=重設|默认=預設",
  ].join('|');

  /* "src dst" pairs, 2 characters each, packed into a string. */
  var CHAR_DATA = [
    "与與个個为為于於仅僅优優传傳体體余餘储儲关關冲衝决決况況减減击擊务務动動单單占佔却卻厂廠历歷参參发發变變号號后後启啟员員响響图圖复復学學实實对對导導将將",
    "并並库庫应應开開弃棄强強当當录錄径徑态態总總惯慣担擔拟擬择擇换換据據数數断斷无無时時显顯机機条條来來构構标標气氣没沒济濟温溫滚滾滞滯满滿点點热熱牵牽状狀",
    "独獨环環现現电電畅暢盖蓋盘盤着著码碼础礎离離积積称稱稳穩筛篩类類线線组組终終经經结結绘繪绝絕统統绪緒维維缓緩缩縮脚腳舱艙节節视視触觸计計认認记記许許设設",
    "识識试試该該语語误誤请請读讀调調负負败敗货貨质質车車转轉轮輪轴軸载載辆輛输輸过過运運连連迟遲选選键鍵闭閉间間阈閾际際随隨隐隱静靜顶頂项項须須预預风風驱驅",
    "驶駛驻駐验驗",
  ].join('');

  var MAX_PHRASE = 8;

  var PHRASES = (function (raw) {
    var map = {};
    if (!raw) return map;
    var parts = raw.split('|');
    for (var i = 0; i < parts.length; i++) {
      var at = parts[i].indexOf('=');
      if (at > 0) map[parts[i].slice(0, at)] = parts[i].slice(at + 1);
    }
    return map;
  })(PHRASE_DATA);

  var CHARS = (function (raw) {
    var map = {};
    for (var i = 0; i + 1 < raw.length; i += 2) map[raw.charAt(i)] = raw.charAt(i + 1);
    return map;
  })(CHAR_DATA);

  var cache = {};
  var cacheSize = 0;
  var CACHE_LIMIT = 2000;

  /** Convert one Simplified string.  Anything that is not a string is returned as-is. */
  function toHant(text) {
    if (typeof text !== 'string' || text === '') return text;
    var hit = cache[text];
    if (hit !== undefined) return hit;
    if (Object.prototype.hasOwnProperty.call(EXACT, text)) return EXACT[text];

    var out = [];
    var i = 0;
    var n = text.length;
    while (i < n) {
      var used = 0;
      var repl = null;
      for (var len = MAX_PHRASE; len > 0; len--) {
        if (i + len > n) continue;
        var cand = text.substr(i, len);
        if (Object.prototype.hasOwnProperty.call(PHRASES, cand)) {
          used = len;
          repl = PHRASES[cand];
          break;
        }
      }
      if (used) {
        out.push(repl);
        i += used;
      } else {
        var ch = text.charAt(i);
        var mapped = CHARS[ch];
        out.push(mapped === undefined ? ch : mapped);
        i += 1;
      }
    }
    var result = out.join('');
    if (cacheSize < CACHE_LIMIT) {
      cache[text] = result;
      cacheSize += 1;
    }
    return result;
  }

  global.FSHant = {
    toHant: toHant,
    charCount: Object.keys(CHARS).length,
    phraseCount: Object.keys(PHRASES).length
  };
})(typeof window !== 'undefined' ? window : this);
