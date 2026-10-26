#!/usr/bin/env bash
# Verify accounts view + broken-journal error path in headless Chrome.
set -u
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
"$CHROME" --headless=new --disable-gpu --user-data-dir=/tmp/lwasm-p3 \
  --dump-dom "http://127.0.0.1:8931/index.html#view=accounts" > /tmp/lwasm-r3.html 2>/dev/null
echo "accounts: $(grep -c 'class="chip"' /tmp/lwasm-r3.html) chips | $(grep -o '<span id="report-title">[^<]*' /tmp/lwasm-r3.html)"

"$CHROME" --headless=new --disable-gpu --user-data-dir=/tmp/lwasm-p3 \
  --dump-dom "http://127.0.0.1:8931/index.html#broken=1" > /tmp/lwasm-r4.html 2>/dev/null

python3 - /tmp/lwasm-r4.html <<'PY'
import re, sys
html = open(sys.argv[1]).read()
m = re.search(r'id="error" class="error"([^>]*)>(.*?)</div>', html, re.S)
print("error hidden:", bool(m and m.group(1).strip() == 'hidden=""'))
if m:
    txt = re.sub(r'<[^>]+>', '', m.group(2)).strip()
    print("error text:", txt[:240])
ph = re.search(r'class="placeholder">([^<]*)', html)
print("report body:", ph.group(1).strip() if ph else "(n/a)")
PY