# cron units

systemd timers + scripts that run on yokai to keep the playground's preset chips fresh.

## refresh-sample-hash

Picks one recent Stellar testnet `invoke_host_function` transaction and writes
its hash to `/var/www/policy/sample-hash.txt`, served as `policy.erentopal.xyz/sample-hash.txt`.

**Filter:** only accepts transactions whose `auto` synthesis produces a
`PolicySlot::Generated` (Track B). Track A (composed primitive) candidates
are skipped so the playground's Source tab is always populated with real
generated Rust for the sample preset.

**Schedule:** boots two minutes after system start, then every hour.

**Failure mode:** if horizon is unreachable, or if no Track B candidate is
found in the horizon batch, leaves the previous file in place and exits
non-zero. No fake fallbacks.

## Install on a fresh server

```bash
sudo install -m 755 -o root -g root refresh-sample-hash.sh /usr/local/bin/
sudo install -m 644 -o root -g root refresh-sample-hash.service /etc/systemd/system/
sudo install -m 644 -o root -g root refresh-sample-hash.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now refresh-sample-hash.timer
```

The script depends on `/usr/local/bin/oz-policy-cli` (build from the
repo with `cargo build --release -p oz-policy-cli`).
