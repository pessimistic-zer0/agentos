#!/bin/sh
# E2: 64 parallel 4 MiB downloads from an empty network namespace through
# 127.0.0.1:3128, in both modes. Needs python3, curl, unshare, ip.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dir=$(mktemp -d); cd "$dir"
head -c 4194304 /dev/urandom > big; want=$(sha256sum big | cut -d' ' -f1)
python3 -m http.server 8765 --bind 127.0.0.1 >/dev/null 2>&1 & web=$!
sleep 0.5
cat > load.sh <<EOS
#!/bin/sh
# Inside: no route out except the proxy port.
python3 -c "import socket; s=socket.socket(); s.settimeout(2); s.connect(('1.1.1.1',80))" 2>&1 | tail -1 | sed 's/^/  direct connect: /'
start=\$(date +%s.%N)
for i in \$(seq 64); do curl -s -p -x http://127.0.0.1:3128 -o out.\$i http://127.0.0.1:8765/big & done; wait
end=\$(date +%s.%N)
ok=0; for i in \$(seq 64); do [ "\$(sha256sum out.\$i | cut -d' ' -f1)" = "$want" ] && ok=\$((ok+1)); done
echo "  64 parallel CONNECT downloads of 4 MiB: \$ok intact, \$(python3 -c "print(round(\$end - \$start, 2))") s"
rm -f out.*
EOS
chmod +x load.sh
for mode in unix fdpass; do
  echo "mode $mode"
  python3 "$here/outside.py" $mode "$dir/s.$mode" & out=$!
  sleep 0.3
  unshare -rn sh -c "ip link set lo up; python3 '$here/inside.py' $mode '$dir/s.$mode' -- ./load.sh"
  kill $out
done
kill $web; rm -rf "$dir"
