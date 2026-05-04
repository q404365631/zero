# Live Trading Evidence

`zero.live_trading_evidence.v1` is the public-safe proof format for
operator-owned live Hyperliquid execution evidence.

It exists because the private ZERO engine can trade live, while the public repo
must not publish raw custody material, raw venue order identifiers, raw client
order IDs, raw trace IDs, private journals, or full exchange payloads.

Build a redacted packet from local private evidence:

```bash
scripts/live_trading_evidence.py build \
  --fills /path/to/md_user_fills.json \
  --orders /path/to/md_orders.json \
  --trades /path/to/trades.jsonl \
  --decisions /path/to/decisions_live.jsonl \
  --reconciliation /path/to/startup_reconcile_last.json \
  --output docs/proof/live/live-trading-evidence.json
```

Verify the public packet:

```bash
scripts/live_trading_evidence.py verify docs/proof/live/live-trading-evidence.json
```

The committed packet is redacted:

- symbols are hashed unless an operator explicitly opts into `--include-symbols`;
- quantities are bucketed;
- prices are not published, only notional buckets;
- timestamps are hour-bucketed;
- raw source files are never embedded;
- raw order IDs, client order IDs, wallet addresses, trace IDs, and private keys
  are forbidden by the verifier.

This proves that live execution evidence exists and is internally consistent. It
does not prove custody ownership to a third party unless the operator chooses to
publish or privately disclose the matching raw Hyperliquid export.
