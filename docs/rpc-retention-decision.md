# Soroban RPC retention — Phase 1 finding & Phase 8 decision

## Source

Stellar developer docs: `https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getTransaction` (verified 2026-05-15).

## Retention windows

- **Default retention window: 24 hours.** *"The stellar-rpc system maintains a restricted history of recently processed transactions, with the default retention window set at 24 hours."*
- **Maximum retention window: 7 days.** *"For private soroban-rpc instances, operators can adjust this window up to a maximum of 7 days,"* with the docs explicitly cautioning *"we do not recommend values longer than 7 days."*

## Behavior of public SDF endpoints (testnet, mainnet)

The public docs do **not explicitly state** whether SDF-operated testnet/mainnet RPCs override the 24h default. The documented language frames the 7-day cap as a privilege of *"private soroban-rpc instances"* — strongly implying public SDF endpoints run with the default 24h retention. This matches anecdotal community reports (referenced by research §5).

**Operating assumption for this project:** public SDF testnet RPC = 24h, public SDF mainnet RPC = 24h. We treat `getTransaction` for any hash older than ~24h on a public RPC as an expected miss and route to the Hubble BigQuery fallback (below).

If a Phase 8 walkthrough requires older history during a single eval run, we are responsible for re-recording before drift; we do not chase private extended-retention RPCs.

## Decision for the project

### Phase 8 walkthrough source-tx ingest

- **Do NOT operate a private extended-retention RPC.** The toolkit is Apache-2.0 and the walkthroughs are part of the public eval suite; pulling them through a private RPC would couple the project to an infrastructure dependency the community can't reproduce.
- **Re-record before drift.** Each walkthrough's `source.json` includes a `recorded_at` ledger sequence and a `source_tx_hash`. CI re-records freshness on a 12-hour cadence so the public-RPC 24h window is never the bottleneck. If a recorder run misses the window, CI emits `E_RECORDER_HASH_NOT_FOUND` and the walkthrough auto-falls-back to Hubble (see next).

### Hubble BigQuery fallback

For any source hash older than ~24h (or any time the public RPC returns `not_found` for a hash known to have existed historically), the recorder falls back to **Stellar's Hubble dataset on Google BigQuery** (`crypto-stellar.crypto_stellar.history_transactions`, plus `history_operations` and `history_effects` as needed). Hubble retains full mainnet/testnet history and is the canonical archive for anything beyond the 24h-7d window.

Implementation notes for Phase 1.5 / Phase 8:
- Hubble access requires GCP credentials. The toolkit ships a `gcloud auth application-default login` flow and a `BQ_QUERY_PROJECT` env var. No credentials are bundled.
- Hubble queries are billed per TB scanned; the recorder constrains queries by `closed_at` and `transaction_hash` to keep typical scans under 10 MB.
- The recorder's `RpcAccess` trait abstracts over `Live(stellar_rpc_client::Client)` and `Hubble(gcp_bigquery_client)`; tests use a fixture-backed mock.
- For each walkthrough, both an RPC-fresh and a Hubble-archive ingest path must produce **byte-identical `Recording` outputs** (a regression-gated property test). Hubble's XDR is reassembled from `tx_envelope` / `tx_result` / `tx_meta` columns to match what the RPC `getTransaction` returns.

## Summary table

| Endpoint | Default retention | Configurable? | This project's usage |
|---|---|---|---|
| Public SDF mainnet RPC | 24h (assumed; not contradicted by docs) | No | Primary path for fresh recordings |
| Public SDF testnet RPC | 24h (assumed) | No | Primary path for fresh recordings; sandbox for walkthroughs |
| Private soroban-rpc | 24h default, up to 7d max | Yes, operator-controlled | Not used by this project |
| Hubble BigQuery | Full history | n/a (archive) | Mandatory fallback for hashes >24h old |

## Action items

- [ ] (Phase 1) — done: this decision recorded.
- [ ] (Phase 2 recorder) — implement `RpcAccess` trait with `LiveRpc` + `HubbleArchive` impls and a `GetTransaction` adapter.
- [ ] (Phase 8 walkthroughs) — each walkthrough's `source.json` records `recorded_at`, `source_tx_hash`, `network` (testnet/mainnet), and `archive_backend` (rpc | hubble). CI runs both ingest paths and asserts byte-equality of the produced `Recording`.
- [ ] (Phase 8) — operate a 12h CI cadence to re-anchor the walkthroughs against fresh source hashes when SDF RPC eviction would otherwise break the suite.
