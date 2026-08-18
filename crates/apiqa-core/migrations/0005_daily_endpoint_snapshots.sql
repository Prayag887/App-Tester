-- Migration: 0005_daily_endpoint_snapshots
-- Keep one response snapshot per endpoint and UTC day while preserving a
-- compact count of meaningful changes. Approved baselines are never removed.

CREATE TABLE IF NOT EXISTS endpoint_daily_snapshots (
    endpoint_key TEXT NOT NULL,
    observed_day TEXT NOT NULL,
    transaction_id TEXT NOT NULL UNIQUE,
    change_count INTEGER NOT NULL DEFAULT 0,
    last_changed_at TEXT,
    PRIMARY KEY(endpoint_key, observed_day)
);

CREATE INDEX IF NOT EXISTS endpoint_daily_snapshots_transaction
    ON endpoint_daily_snapshots(transaction_id);

WITH eligible AS (
    SELECT
        t.id,
        t.updated_at,
        substr(t.created_at, 1, 10) AS observed_day,
        coalesce(
            json_extract(t.payload_json, '$.endpoint_identity.method'),
            json_extract(t.payload_json, '$.request.method')
        ) || ' ' || lower(coalesce(
            json_extract(t.payload_json, '$.endpoint_identity.host'),
            json_extract(t.payload_json, '$.request.host')
        )) || ' ' || coalesce(
            json_extract(t.payload_json, '$.endpoint_identity.path_template'),
            json_extract(t.payload_json, '$.request.path')
        ) AS endpoint_key,
        coalesce(json_extract(t.payload_json, '$.response.status'), '') || '|' ||
        coalesce(json_extract(t.payload_json, '$.response.content_type'), '') || '|' ||
        coalesce(json_extract(t.payload_json, '$.response.body'), '') AS response_signature,
        CASE WHEN EXISTS (
            SELECT 1
            FROM json_each(t.payload_json, '$.comparison.differences')
            WHERE COALESCE(json_extract(value, '$.ignored'), 0) = 0
        ) THEN 1 ELSE 0 END AS changed
    FROM transactions t
    WHERE t.state = 'ResponseComplete'
      AND json_extract(t.payload_json, '$.request.method') IS NOT NULL
), ranked AS (
    SELECT *, row_number() OVER (
        PARTITION BY endpoint_key, observed_day
        ORDER BY updated_at DESC, id DESC
    ) AS recency
    FROM eligible
)
INSERT INTO endpoint_daily_snapshots(
    endpoint_key,
    observed_day,
    transaction_id,
    change_count,
    last_changed_at
)
SELECT
    endpoint_key,
    observed_day,
    max(CASE WHEN recency = 1 THEN id END),
    max(sum(changed), count(DISTINCT response_signature) - 1),
    CASE
        WHEN count(DISTINCT response_signature) > 1 THEN max(updated_at)
        ELSE max(CASE WHEN changed = 1 THEN updated_at END)
    END
FROM ranked
GROUP BY endpoint_key, observed_day;

DELETE FROM transactions
WHERE id IN (
    SELECT t.id
    FROM transactions t
    JOIN endpoint_daily_snapshots d
      ON d.endpoint_key = (
          coalesce(
              json_extract(t.payload_json, '$.endpoint_identity.method'),
              json_extract(t.payload_json, '$.request.method')
          ) || ' ' || lower(coalesce(
              json_extract(t.payload_json, '$.endpoint_identity.host'),
              json_extract(t.payload_json, '$.request.host')
          )) || ' ' || coalesce(
              json_extract(t.payload_json, '$.endpoint_identity.path_template'),
              json_extract(t.payload_json, '$.request.path')
          )
      )
     AND d.observed_day = substr(t.created_at, 1, 10)
    WHERE t.state = 'ResponseComplete'
      AND t.id <> d.transaction_id
      AND NOT EXISTS (
          SELECT 1 FROM approved_baselines b WHERE b.transaction_id = t.id
      )
);
