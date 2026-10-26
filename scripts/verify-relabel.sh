#!/usr/bin/env bash
# After the format-relabel + button move: verify reg and accounts still work.
set -u
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
for view in reg accounts; do
  "$CHROME" --headless=new --disable-gpu --user-data-dir=/tmp/lwasm-p3 \
    --virtual-time-budget=12000 \
    --dump-dom "http://127.0.0.1:8931/index.html#view=$view" > /tmp/lwasm-v-$view.html 2>/dev/null
  echo "$view: $(grep -o '<span id="report-title">[^<]*' /tmp/lwasm-v-$view.html | sed 's/<span id="report-title">//')"
done
# error path still wired after variable rename
"$CHROME" --headless=new --disable-gpu --user-data-dir=/tmp/lwasm-p3 \
  --virtual-time-budget=12000 \
  --dump-dom "http://127.0.0.1:8931/index.html#broken=1" > /tmp/lwasm-v-broken.html 2>/dev/null
python3 - /tmp/lwasm-v-broken.html <<'PY'
import re, sys
h = open(sys.argv[1]).read()
m = re.search(r'id="error" class="error"([^>]*)>(.*?)</div>', h, re.S)
txt = re.sub(r'<[^>]+>', '', m.group(2)).strip() if m else '(none)'
print("broken: error hidden =", bool(m and 'hidden' in m.group(1)), "| text:", txt[:60])
PY