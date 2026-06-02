-- Returns transactions where UTXOs span multiple distinct intent_hashes (segments).
-- Only between-segment ordering is affected by the HashMap→BTreeMap fix;
-- within-segment order is deterministic.
--
-- For created UTXOs, ORDER BY u.id reflects the indexer's insertion order during
-- block processing, which matches the original runtime event order.
--
-- For spent UTXOs, ORDER BY u.id gives creation order which is a best-effort
-- approximation. The indexer doesn't record spending event order, so overrides
-- for multi-segment spends may need manual correction if u.id order doesn't
-- match the original HashMap iteration order.
WITH interesting_txs AS (
  -- Transactions where created UTXOs span multiple segments
  SELECT creating_transaction_id AS tx_id
  FROM unshielded_utxos
  GROUP BY creating_transaction_id
  HAVING COUNT(DISTINCT intent_hash) > 1
  UNION
  -- Transactions where spent UTXOs span multiple segments
  SELECT spending_transaction_id AS tx_id
  FROM unshielded_utxos
  WHERE spending_transaction_id IS NOT NULL
  GROUP BY spending_transaction_id
  HAVING COUNT(DISTINCT intent_hash) > 1
),
created_utxos AS (
  SELECT
    u.creating_transaction_id AS tx_id,
    json_agg(
      json_build_object(
        'intent_hash', encode(u.intent_hash, 'hex'),
        'output_index', u.output_index
      ) ORDER BY u.id
    ) AS utxos
  FROM unshielded_utxos u
  WHERE u.creating_transaction_id IN (SELECT tx_id FROM interesting_txs)
  GROUP BY u.creating_transaction_id
),
spent_utxos AS (
  SELECT
    u.spending_transaction_id AS tx_id,
    json_agg(
      json_build_object(
        'intent_hash', encode(u.intent_hash, 'hex'),
        'output_index', u.output_index
      ) ORDER BY u.id
    ) AS utxos
  FROM unshielded_utxos u
  WHERE u.spending_transaction_id IN (SELECT tx_id FROM interesting_txs)
  GROUP BY u.spending_transaction_id
)
SELECT COALESCE(json_agg(row_data ORDER BY row_data->>'block_height'), '[]'::json)
FROM (
  SELECT
    json_build_object(
      'tx_hash', encode(t.hash, 'hex'),
      'block_height', b.height,
      'created', COALESCE(c.utxos, '[]'::json),
      'spent', COALESCE(s.utxos, '[]'::json)
    ) AS row_data
  FROM interesting_txs it
  JOIN transactions t ON t.id = it.tx_id
  JOIN blocks b ON b.id = t.block_id
  LEFT JOIN created_utxos c ON c.tx_id = it.tx_id
  LEFT JOIN spent_utxos s ON s.tx_id = it.tx_id
) sub;
