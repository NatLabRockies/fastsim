# Traditional Chinese build pipeline

gui/frontend/hant.js and the third column of gui/frontend/i18n.js are GENERATED.
Do not hand-edit them.

To regenerate after adding UI strings or changing backend Chinese text:

    cd gui/tools/hant-build
    python build.py      # collect every Chinese string, convert with opencc s2twp, apply hand fixes
    python gen_hant.py   # emit gui/frontend/hant.js

eview-final.txt is the Simplified -> Traditional diff that was hand-reviewed.
collect.py gathers the strings from gui/backend + gui/frontend.
The rest are verification harnesses used when the conversion was produced.
