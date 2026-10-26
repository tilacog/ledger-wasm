#!/usr/bin/env bash
# Verify the live Pages deployment with network-waiting budget + error capture.
set -u
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
"$CHROME" --headless=new --disable-gpu --user-data-dir=/tmp/lwasm-live2 \
  --virtual-time-budget=20000 \
  --dump-dom "https://tilacog.github.io/ledger-wasm/" > /tmp/lwasm-live2.html 2>/tmp/lwasm-live2.log
echo "bal-rows: $(grep -c 'class="bal-row' /tmp/lwasm-live2.html)"
echo "title: $(grep -o '<span id="report-title">[^<]*' /tmp/lwasm-live2.html)"
echo "total-row: $(grep -c 'class="bal-total"' /tmp/lwasm-live2.html)"
echo "status: $(grep -o 'id="status" class="status[^"]*">[^<]*' /tmp/lwasm-live2.html)"
echo "error-visible: $(python3 -c "
import re
h=open('/tmp/lwasm-live2.html').read()
m=re.search(r'id=\"error\" class=\"error\"([^>]*)>',h)
print('(hidden)' if m and 'hidden' in m.group(1) else (re.sub(r'<[^>]+>','',m.group(0)) if m else 'none'))")"
echo "--- console errors:"
grep -iE "error|failed|uncaught" /tmp/lwasm-live2.log | grep -v "CVDisplayLink\|GroupMarkerNotSet\|bundle_display_link" | head -5