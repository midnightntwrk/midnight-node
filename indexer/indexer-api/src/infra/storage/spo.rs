// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    domain::{
        spo::{
            CommitteeMember, EpochInfo, EpochPerf, FirstValidEpoch, PoolMetadata, PresenceEvent,
            RegisteredStat, RegisteredTotals, Spo, SpoComposite, SpoIdentity, StakeShare,
        },
        storage::spo::SpoStorage,
    },
    infra::storage::Storage,
};
use fastrace::trace;
use indoc::indoc;

impl SpoStorage for Storage {
    #[trace]
    async fn get_spo_identities(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SpoIdentity>, sqlx::Error> {
        let query = indoc! {"
            SELECT pool_id AS pool_id_hex,
                   mainchain_pubkey AS mainchain_pubkey_hex,
                   sidechain_pubkey AS sidechain_pubkey_hex,
                   aura_pubkey AS aura_pubkey_hex,
                   'UNKNOWN' AS validator_class
            FROM spo_identity
            WHERE pool_id IS NOT NULL
            ORDER BY mainchain_pubkey
            LIMIT $1 OFFSET $2
        "};

        sqlx::query_as::<_, (String, String, String, Option<String>, String)>(query)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(
                        |(
                            pool_id_hex,
                            mainchain_pubkey_hex,
                            sidechain_pubkey_hex,
                            aura_pubkey_hex,
                            validator_class,
                        )| SpoIdentity {
                            pool_id_hex,
                            mainchain_pubkey_hex,
                            sidechain_pubkey_hex,
                            aura_pubkey_hex,
                            validator_class,
                        },
                    )
                    .collect()
            })
    }

    #[trace]
    async fn get_spo_identity_by_pool_id(
        &self,
        pool_id: &str,
    ) -> Result<Option<SpoIdentity>, sqlx::Error> {
        let query = indoc! {"
            SELECT pool_id AS pool_id_hex,
                   mainchain_pubkey AS mainchain_pubkey_hex,
                   sidechain_pubkey AS sidechain_pubkey_hex,
                   aura_pubkey AS aura_pubkey_hex,
                   'UNKNOWN' AS validator_class
            FROM spo_identity
            WHERE pool_id = $1
            LIMIT 1
        "};

        sqlx::query_as::<_, (String, String, String, Option<String>, String)>(query)
            .bind(pool_id)
            .fetch_optional(&*self.pool)
            .await
            .map(|opt| {
                opt.map(
                    |(
                        pool_id_hex,
                        mainchain_pubkey_hex,
                        sidechain_pubkey_hex,
                        aura_pubkey_hex,
                        validator_class,
                    )| SpoIdentity {
                        pool_id_hex,
                        mainchain_pubkey_hex,
                        sidechain_pubkey_hex,
                        aura_pubkey_hex,
                        validator_class,
                    },
                )
            })
    }

    #[trace]
    async fn get_spo_count(&self) -> Result<i64, sqlx::Error> {
        let query = indoc! {"
            SELECT COUNT(1) FROM spo_stake_snapshot
        "};

        sqlx::query_scalar::<_, i64>(query)
            .fetch_one(&*self.pool)
            .await
    }

    #[trace]
    async fn get_pool_metadata(&self, pool_id: &str) -> Result<Option<PoolMetadata>, sqlx::Error> {
        let query = indoc! {"
            SELECT pool_id AS pool_id_hex,
                   hex_id AS hex_id,
                   name, ticker, homepage_url, url AS logo_url
            FROM pool_metadata_cache
            WHERE pool_id = $1
            LIMIT 1
        "};

        sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(pool_id)
        .fetch_optional(&*self.pool)
        .await
        .map(|opt| {
            opt.map(
                |(pool_id_hex, hex_id, name, ticker, homepage_url, logo_url)| PoolMetadata {
                    pool_id_hex,
                    hex_id,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                },
            )
        })
    }

    #[trace]
    async fn get_pool_metadata_list(
        &self,
        limit: i64,
        offset: i64,
        with_name_only: bool,
    ) -> Result<Vec<PoolMetadata>, sqlx::Error> {
        let query = if with_name_only {
            indoc! {"
                SELECT pool_id AS pool_id_hex,
                       hex_id AS hex_id,
                       name, ticker, homepage_url, url AS logo_url
                FROM pool_metadata_cache
                WHERE name IS NOT NULL OR ticker IS NOT NULL
                ORDER BY pool_id
                LIMIT $1 OFFSET $2
            "}
        } else {
            indoc! {"
                SELECT pool_id AS pool_id_hex,
                       hex_id AS hex_id,
                       name, ticker, homepage_url, url AS logo_url
                FROM pool_metadata_cache
                ORDER BY pool_id
                LIMIT $1 OFFSET $2
            "}
        };

        sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(
                    |(pool_id_hex, hex_id, name, ticker, homepage_url, logo_url)| PoolMetadata {
                        pool_id_hex,
                        hex_id,
                        name,
                        ticker,
                        homepage_url,
                        logo_url,
                    },
                )
                .collect()
        })
    }

    #[trace]
    async fn get_spo_by_pool_id(&self, pool_id: &str) -> Result<Option<Spo>, sqlx::Error> {
        let query = indoc! {"
            SELECT si.pool_id AS pool_id_hex,
                   'UNKNOWN' AS validator_class,
                   si.sidechain_pubkey AS sidechain_pubkey_hex,
                   si.aura_pubkey AS aura_pubkey_hex,
                   pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url
            FROM spo_identity si
            LEFT JOIN pool_metadata_cache pm ON pm.pool_id = si.pool_id
            WHERE si.pool_id = $1
            LIMIT 1
        "};

        sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(pool_id)
        .fetch_optional(&*self.pool)
        .await
        .map(|opt| {
            opt.map(
                |(
                    pool_id_hex,
                    validator_class,
                    sidechain_pubkey_hex,
                    aura_pubkey_hex,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                )| Spo {
                    pool_id_hex,
                    validator_class,
                    sidechain_pubkey_hex,
                    aura_pubkey_hex,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                },
            )
        })
    }

    #[trace]
    async fn get_spo_list(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Spo>, sqlx::Error> {
        let rows = if let Some(s) = search {
            let s_like = format!("%{s}%");
            let s_hex = normalize_hex(s).unwrap_or_else(|| s.to_ascii_lowercase());
            let s_hex_like = format!("%{s_hex}%");

            #[cfg(feature = "cloud")]
            let query = indoc! {"
                SELECT s.pool_id AS pool_id_hex,
                       'UNKNOWN' AS validator_class,
                       si.sidechain_pubkey AS sidechain_pubkey_hex,
                       si.aura_pubkey AS aura_pubkey_hex,
                       pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url
                FROM spo_stake_snapshot s
                LEFT JOIN spo_identity si ON si.pool_id = s.pool_id
                LEFT JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                WHERE (
                        pm.name ILIKE $3 OR pm.ticker ILIKE $3 OR pm.homepage_url ILIKE $3 OR s.pool_id ILIKE $4
                     OR si.sidechain_pubkey ILIKE $4 OR si.aura_pubkey ILIKE $4 OR si.mainchain_pubkey ILIKE $4
                  )
                ORDER BY COALESCE(si.mainchain_pubkey, s.pool_id)
                LIMIT $1 OFFSET $2
            "};

            #[cfg(feature = "standalone")]
            let query = indoc! {"
                SELECT s.pool_id AS pool_id_hex,
                       'UNKNOWN' AS validator_class,
                       si.sidechain_pubkey AS sidechain_pubkey_hex,
                       si.aura_pubkey AS aura_pubkey_hex,
                       pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url
                FROM spo_stake_snapshot s
                LEFT JOIN spo_identity si ON si.pool_id = s.pool_id
                LEFT JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                WHERE (
                        LOWER(pm.name) LIKE LOWER($3)
                     OR LOWER(pm.ticker) LIKE LOWER($3)
                     OR LOWER(pm.homepage_url) LIKE LOWER($3)
                     OR LOWER(s.pool_id) LIKE LOWER($4)
                     OR LOWER(si.sidechain_pubkey) LIKE LOWER($4)
                     OR LOWER(si.aura_pubkey) LIKE LOWER($4)
                     OR LOWER(si.mainchain_pubkey) LIKE LOWER($4)
                  )
                ORDER BY COALESCE(si.mainchain_pubkey, s.pool_id)
                LIMIT $1 OFFSET $2
            "};

            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                ),
            >(query)
            .bind(limit)
            .bind(offset)
            .bind(s_like)
            .bind(s_hex_like)
            .fetch_all(&*self.pool)
            .await?
        } else {
            let query = indoc! {"
                SELECT s.pool_id AS pool_id_hex,
                       'UNKNOWN' AS validator_class,
                       si.sidechain_pubkey AS sidechain_pubkey_hex,
                       si.aura_pubkey AS aura_pubkey_hex,
                       pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url
                FROM spo_stake_snapshot s
                LEFT JOIN spo_identity si ON si.pool_id = s.pool_id
                LEFT JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                ORDER BY COALESCE(si.mainchain_pubkey, s.pool_id)
                LIMIT $1 OFFSET $2
            "};

            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                ),
            >(query)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(
                |(
                    pool_id_hex,
                    validator_class,
                    sidechain_pubkey_hex,
                    aura_pubkey_hex,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                )| Spo {
                    pool_id_hex,
                    validator_class,
                    sidechain_pubkey_hex,
                    aura_pubkey_hex,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                },
            )
            .collect())
    }

    #[trace]
    async fn get_spo_composite_by_pool_id(
        &self,
        pool_id: &str,
        perf_limit: i64,
    ) -> Result<Option<SpoComposite>, sqlx::Error> {
        // Get identity.
        let identity = self.get_spo_identity_by_pool_id(pool_id).await?;

        // Get metadata.
        let metadata = self.get_pool_metadata(pool_id).await?;

        // Get performance if identity exists.
        let performance = if let Some(ref id) = identity {
            self.get_spo_performance_by_spo_sk(&id.sidechain_pubkey_hex, perf_limit, 0)
                .await?
        } else {
            vec![]
        };

        // Return None only if both identity and metadata are missing.
        if identity.is_none() && metadata.is_none() {
            return Ok(None);
        }

        Ok(Some(SpoComposite {
            identity,
            metadata,
            performance,
        }))
    }

    #[trace]
    async fn get_stake_pool_operator_ids(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let query = indoc! {"
            SELECT sep.spo_sk
            FROM spo_epoch_performance sep
            GROUP BY sep.spo_sk
            ORDER BY MAX(sep.produced_blocks) DESC
            LIMIT $1
        "};

        sqlx::query_scalar::<_, String>(query)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
    }

    #[trace]
    async fn get_spo_performance_latest(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        let query = indoc! {"
            SELECT sep.epoch_no,
                   sep.spo_sk AS spo_sk_hex,
                   sep.produced_blocks,
                   sep.expected_blocks,
                   sep.identity_label,
                   CAST(NULL AS TEXT) AS stake_snapshot,
                   si.pool_id AS pool_id_hex,
                   'UNKNOWN' AS validator_class
            FROM spo_epoch_performance sep
            LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
            ORDER BY sep.epoch_no DESC, sep.produced_blocks DESC
            LIMIT $1 OFFSET $2
        "};

        sqlx::query_as::<
            _,
            (
                i64,
                String,
                i32,
                i32,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map(|rows| rows.into_iter().map(epoch_perf_from_row).collect())
    }

    #[trace]
    async fn get_spo_performance_by_spo_sk(
        &self,
        spo_sk: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        let query = indoc! {"
            SELECT sep.epoch_no,
                   sep.spo_sk AS spo_sk_hex,
                   sep.produced_blocks,
                   sep.expected_blocks,
                   sep.identity_label,
                   CAST(NULL AS TEXT) AS stake_snapshot,
                   si.pool_id AS pool_id_hex,
                   'UNKNOWN' AS validator_class
            FROM spo_epoch_performance sep
            LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
            WHERE sep.spo_sk = $1
            ORDER BY sep.epoch_no DESC
            LIMIT $2 OFFSET $3
        "};

        sqlx::query_as::<
            _,
            (
                i64,
                String,
                i32,
                i32,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(spo_sk)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map(|rows| rows.into_iter().map(epoch_perf_from_row).collect())
    }

    #[trace]
    async fn get_epoch_performance(
        &self,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EpochPerf>, sqlx::Error> {
        let query = indoc! {"
            SELECT sep.epoch_no,
                   sep.spo_sk AS spo_sk_hex,
                   sep.produced_blocks,
                   sep.expected_blocks,
                   sep.identity_label,
                   CAST(NULL AS TEXT) AS stake_snapshot,
                   si.pool_id AS pool_id_hex,
                   'UNKNOWN' AS validator_class
            FROM spo_epoch_performance sep
            LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
            WHERE sep.epoch_no = $1
            ORDER BY sep.produced_blocks DESC
            LIMIT $2 OFFSET $3
        "};

        sqlx::query_as::<
            _,
            (
                i64,
                String,
                i32,
                i32,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(epoch)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map(|rows| rows.into_iter().map(epoch_perf_from_row).collect())
    }

    #[trace]
    async fn get_current_epoch_info(&self) -> Result<Option<EpochInfo>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            WITH last AS (
                SELECT
                    epoch_no,
                    EXTRACT(EPOCH FROM starts_at)::BIGINT AS starts_s,
                    EXTRACT(EPOCH FROM ends_at)::BIGINT AS ends_s,
                    EXTRACT(EPOCH FROM (ends_at - starts_at))::BIGINT AS dur_s,
                    EXTRACT(EPOCH FROM NOW())::BIGINT AS now_s
                FROM epochs
                ORDER BY epoch_no DESC
                LIMIT 1
            ), calc AS (
                SELECT
                    epoch_no, starts_s, ends_s, dur_s, now_s,
                    CASE WHEN ends_s > now_s THEN 0
                         WHEN dur_s <= 0 THEN 0
                         ELSE ((now_s - ends_s) / dur_s)::BIGINT + 1 END AS n
                FROM last
            ), synth AS (
                SELECT
                    (epoch_no + n) AS epoch_no,
                    dur_s AS duration_seconds,
                    CASE WHEN dur_s <= 0 THEN 0
                         WHEN n = 0 THEN LEAST(GREATEST(now_s - starts_s, 0), dur_s)
                         ELSE LEAST(GREATEST(now_s - (ends_s + (n - 1) * dur_s), 0), dur_s)
                    END AS elapsed_seconds
                FROM calc
            )
            SELECT epoch_no, duration_seconds, elapsed_seconds FROM synth
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            WITH last AS (
                SELECT
                    epoch_no,
                    CAST(strftime('%s', starts_at) AS INTEGER) AS starts_s,
                    CAST(strftime('%s', ends_at) AS INTEGER) AS ends_s,
                    CAST(strftime('%s', ends_at) AS INTEGER) - CAST(strftime('%s', starts_at) AS INTEGER) AS dur_s,
                    CAST(strftime('%s', 'now') AS INTEGER) AS now_s
                FROM epochs
                ORDER BY epoch_no DESC
                LIMIT 1
            ), calc AS (
                SELECT
                    epoch_no, starts_s, ends_s, dur_s, now_s,
                    CASE WHEN ends_s > now_s THEN 0
                         WHEN dur_s <= 0 THEN 0
                         ELSE CAST((now_s - ends_s) / dur_s AS INTEGER) + 1 END AS n
                FROM last
            ), synth AS (
                SELECT
                    (epoch_no + n) AS epoch_no,
                    dur_s AS duration_seconds,
                    CASE WHEN dur_s <= 0 THEN 0
                         WHEN n = 0 THEN MIN(MAX(now_s - starts_s, 0), dur_s)
                         ELSE MIN(MAX(now_s - (ends_s + (n - 1) * dur_s), 0), dur_s)
                    END AS elapsed_seconds
                FROM calc
            )
            SELECT epoch_no, duration_seconds, elapsed_seconds FROM synth
        "};

        sqlx::query_as::<_, (i64, i64, i64)>(query)
            .fetch_optional(&*self.pool)
            .await
            .map(|opt| {
                opt.map(|(epoch_no, duration_seconds, elapsed_seconds)| EpochInfo {
                    epoch_no,
                    duration_seconds,
                    elapsed_seconds,
                })
            })
    }

    #[trace]
    async fn get_epoch_utilization(&self, epoch: i64) -> Result<Option<f64>, sqlx::Error> {
        #[cfg(feature = "cloud")]
        let query = indoc! {"
            SELECT COALESCE(
                CASE WHEN SUM(expected_blocks) > 0
                     THEN SUM(produced_blocks)::DOUBLE PRECISION / SUM(expected_blocks)
                     ELSE 0.0 END,
                0.0) AS utilization
            FROM spo_epoch_performance
            WHERE epoch_no = $1
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            SELECT COALESCE(
                CASE WHEN SUM(expected_blocks) > 0
                     THEN SUM(produced_blocks) * 1.0 / SUM(expected_blocks)
                     ELSE 0.0 END,
                0.0) AS utilization
            FROM spo_epoch_performance
            WHERE epoch_no = $1
        "};

        sqlx::query_scalar::<_, Option<f64>>(query)
            .bind(epoch)
            .fetch_one(&*self.pool)
            .await
            .map(|v| v.or(Some(0.0)))
    }

    #[trace]
    async fn get_committee(&self, epoch: i64) -> Result<Vec<CommitteeMember>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                cm.epoch_no,
                cm.position,
                cm.sidechain_pubkey AS sidechain_pubkey_hex,
                cm.expected_slots,
                si.aura_pubkey AS aura_pubkey_hex,
                si.pool_id AS pool_id_hex,
                si.spo_sk AS spo_sk_hex
            FROM committee_membership cm
            LEFT JOIN spo_identity si ON si.sidechain_pubkey = cm.sidechain_pubkey
            WHERE cm.epoch_no = $1
            ORDER BY cm.position
        "};

        sqlx::query_as::<
            _,
            (
                i64,
                i32,
                String,
                i32,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(query)
        .bind(epoch)
        .fetch_all(&*self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(
                    |(
                        epoch_no,
                        position,
                        sidechain_pubkey_hex,
                        expected_slots,
                        aura_pubkey_hex,
                        pool_id_hex,
                        spo_sk_hex,
                    )| CommitteeMember {
                        epoch_no,
                        position,
                        sidechain_pubkey_hex,
                        expected_slots,
                        aura_pubkey_hex,
                        pool_id_hex,
                        spo_sk_hex,
                    },
                )
                .collect()
        })
    }

    #[trace]
    async fn get_registered_totals_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredTotals>, sqlx::Error> {
        let start = from_epoch.min(to_epoch);
        let end = to_epoch.max(from_epoch);

        #[cfg(feature = "cloud")]
        let query = indoc! {"
            WITH rng AS (
                SELECT generate_series($1::BIGINT, $2::BIGINT) AS epoch_no
            ),
            cur AS (
                SELECT s.pool_id
                FROM spo_stake_snapshot s
            ),
            union_firsts AS (
                SELECT si.pool_id AS pool_id, MIN(sh.epoch_no)::BIGINT AS first_seen_epoch
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
                UNION ALL
                SELECT si.pool_id AS pool_id, MIN(cm.epoch_no)::BIGINT AS first_seen_epoch
                FROM committee_membership cm
                LEFT JOIN spo_identity si ON si.sidechain_pubkey = cm.sidechain_pubkey
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
                UNION ALL
                SELECT si.pool_id AS pool_id, MIN(sep.epoch_no)::BIGINT AS first_seen_epoch
                FROM spo_epoch_performance sep
                LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
            ),
            firsts0 AS (
                SELECT pool_id, MIN(first_seen_epoch)::BIGINT AS first_seen_epoch
                FROM union_firsts
                GROUP BY pool_id
            ),
            firsts_cur AS (
                SELECT c.pool_id,
                       COALESCE(f0.first_seen_epoch, $2::BIGINT) AS first_seen_epoch
                FROM cur c
                LEFT JOIN firsts0 f0 ON f0.pool_id = c.pool_id
            ),
            agg AS (
                SELECT r.epoch_no,
                       COUNT(*) FILTER (WHERE fc.first_seen_epoch <= r.epoch_no) AS total_registered,
                       COUNT(*) FILTER (WHERE fc.first_seen_epoch = r.epoch_no) AS newly_registered
                FROM rng r
                CROSS JOIN firsts_cur fc
                GROUP BY r.epoch_no
            )
            SELECT epoch_no, total_registered, newly_registered
            FROM agg
            ORDER BY epoch_no
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            WITH RECURSIVE rng(epoch_no) AS (
                SELECT $1
                UNION ALL
                SELECT epoch_no + 1
                FROM rng
                WHERE epoch_no < $2
            ),
            cur AS (
                SELECT s.pool_id
                FROM spo_stake_snapshot s
            ),
            union_firsts AS (
                SELECT si.pool_id AS pool_id, MIN(sh.epoch_no) AS first_seen_epoch
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
                UNION ALL
                SELECT si.pool_id AS pool_id, MIN(cm.epoch_no) AS first_seen_epoch
                FROM committee_membership cm
                LEFT JOIN spo_identity si ON si.sidechain_pubkey = cm.sidechain_pubkey
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
                UNION ALL
                SELECT si.pool_id AS pool_id, MIN(sep.epoch_no) AS first_seen_epoch
                FROM spo_epoch_performance sep
                LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
                WHERE si.pool_id IS NOT NULL
                GROUP BY si.pool_id
            ),
            firsts0 AS (
                SELECT pool_id, MIN(first_seen_epoch) AS first_seen_epoch
                FROM union_firsts
                GROUP BY pool_id
            ),
            firsts_cur AS (
                SELECT c.pool_id,
                       COALESCE(f0.first_seen_epoch, $2) AS first_seen_epoch
                FROM cur c
                LEFT JOIN firsts0 f0 ON f0.pool_id = c.pool_id
            ),
            agg AS (
                SELECT r.epoch_no,
                       SUM(CASE WHEN fc.first_seen_epoch <= r.epoch_no THEN 1 ELSE 0 END) AS total_registered,
                       SUM(CASE WHEN fc.first_seen_epoch = r.epoch_no THEN 1 ELSE 0 END) AS newly_registered
                FROM rng r
                CROSS JOIN firsts_cur fc
                GROUP BY r.epoch_no
            )
            SELECT epoch_no, total_registered, newly_registered
            FROM agg
            ORDER BY epoch_no
        "};

        sqlx::query_as::<_, (i64, i64, i64)>(query)
            .bind(start)
            .bind(end)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(
                        |(epoch_no, total_registered, newly_registered)| RegisteredTotals {
                            epoch_no,
                            total_registered,
                            newly_registered,
                        },
                    )
                    .collect()
            })
    }

    #[trace]
    async fn get_registered_spo_series(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<RegisteredStat>, sqlx::Error> {
        let start = from_epoch.min(to_epoch);
        let end = to_epoch.max(from_epoch);

        #[cfg(feature = "cloud")]
        let query = indoc! {"
            WITH rng AS (
                SELECT generate_series($1::BIGINT, $2::BIGINT) AS epoch_no
            ),
            hist_valid AS (
                SELECT sh.epoch_no,
                       COUNT(DISTINCT si.pool_id) AS cnt
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE sh.status IN ('VALID','Valid')
                  AND sh.epoch_no BETWEEN $1::BIGINT AND $2::BIGINT
                  AND si.pool_id IS NOT NULL
                GROUP BY sh.epoch_no
            ),
            hist_invalid AS (
                SELECT sh.epoch_no,
                       COUNT(DISTINCT si.pool_id) AS cnt
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE sh.status IN ('INVALID','Invalid')
                  AND sh.epoch_no BETWEEN $1::BIGINT AND $2::BIGINT
                  AND si.pool_id IS NOT NULL
                GROUP BY sh.epoch_no
            ),
            fed AS (
                SELECT c.epoch_no,
                       COUNT(DISTINCT c.sidechain_pubkey) FILTER (WHERE c.expected_slots > 0) AS federated_valid_count,
                       0::BIGINT AS federated_invalid_count
                FROM committee_membership c
                WHERE c.epoch_no BETWEEN $1::BIGINT AND $2::BIGINT
                GROUP BY c.epoch_no
            )
            SELECT r.epoch_no,
                   COALESCE(f.federated_valid_count, 0) AS federated_valid_count,
                   COALESCE(f.federated_invalid_count, 0) AS federated_invalid_count,
                   COALESCE(hv.cnt, 0) AS registered_valid_count,
                   COALESCE(hi.cnt, 0) AS registered_invalid_count,
                   COALESCE(hv.cnt, 0)::DOUBLE PRECISION AS dparam
            FROM rng r
            LEFT JOIN hist_valid hv ON hv.epoch_no = r.epoch_no
            LEFT JOIN hist_invalid hi ON hi.epoch_no = r.epoch_no
            LEFT JOIN fed f ON f.epoch_no = r.epoch_no
            ORDER BY r.epoch_no
        "};

        #[cfg(feature = "standalone")]
        let query = indoc! {"
            WITH RECURSIVE rng(epoch_no) AS (
                SELECT $1
                UNION ALL
                SELECT epoch_no + 1
                FROM rng
                WHERE epoch_no < $2
            ),
            hist_valid AS (
                SELECT sh.epoch_no,
                       COUNT(DISTINCT si.pool_id) AS cnt
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE sh.status IN ('VALID','Valid')
                  AND sh.epoch_no BETWEEN $1 AND $2
                  AND si.pool_id IS NOT NULL
                GROUP BY sh.epoch_no
            ),
            hist_invalid AS (
                SELECT sh.epoch_no,
                       COUNT(DISTINCT si.pool_id) AS cnt
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE sh.status IN ('INVALID','Invalid')
                  AND sh.epoch_no BETWEEN $1 AND $2
                  AND si.pool_id IS NOT NULL
                GROUP BY sh.epoch_no
            ),
            fed AS (
                SELECT c.epoch_no,
                       COUNT(DISTINCT CASE WHEN c.expected_slots > 0 THEN c.sidechain_pubkey END) AS federated_valid_count,
                       0 AS federated_invalid_count
                FROM committee_membership c
                WHERE c.epoch_no BETWEEN $1 AND $2
                GROUP BY c.epoch_no
            )
            SELECT r.epoch_no,
                   COALESCE(f.federated_valid_count, 0) AS federated_valid_count,
                   COALESCE(f.federated_invalid_count, 0) AS federated_invalid_count,
                   COALESCE(hv.cnt, 0) AS registered_valid_count,
                   COALESCE(hi.cnt, 0) AS registered_invalid_count,
                   COALESCE(hv.cnt, 0) * 1.0 AS dparam
            FROM rng r
            LEFT JOIN hist_valid hv ON hv.epoch_no = r.epoch_no
            LEFT JOIN hist_invalid hi ON hi.epoch_no = r.epoch_no
            LEFT JOIN fed f ON f.epoch_no = r.epoch_no
            ORDER BY r.epoch_no
        "};

        sqlx::query_as::<_, (i64, i64, i64, i64, i64, Option<f64>)>(query)
            .bind(start)
            .bind(end)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(
                        |(
                            epoch_no,
                            federated_valid_count,
                            federated_invalid_count,
                            registered_valid_count,
                            registered_invalid_count,
                            dparam,
                        )| RegisteredStat {
                            epoch_no,
                            federated_valid_count,
                            federated_invalid_count,
                            registered_valid_count,
                            registered_invalid_count,
                            dparam,
                        },
                    )
                    .collect()
            })
    }

    #[trace]
    async fn get_registered_presence(
        &self,
        from_epoch: i64,
        to_epoch: i64,
    ) -> Result<Vec<PresenceEvent>, sqlx::Error> {
        let start = from_epoch.min(to_epoch);
        let end = to_epoch.max(from_epoch);

        let query = indoc! {"
            WITH history AS (
                SELECT sh.epoch_no AS epoch_no,
                       COALESCE(si.pool_id, sh.spo_sk) AS id_key,
                       'history' AS source,
                       sh.status AS status
                FROM spo_history sh
                LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
                WHERE sh.epoch_no BETWEEN $1 AND $2
            ),
            committee AS (
                SELECT cm.epoch_no AS epoch_no,
                       COALESCE(si.pool_id, cm.sidechain_pubkey) AS id_key,
                       'committee' AS source,
                       CAST(NULL AS TEXT) AS status
                FROM committee_membership cm
                LEFT JOIN spo_identity si ON si.sidechain_pubkey = cm.sidechain_pubkey
                WHERE cm.epoch_no BETWEEN $1 AND $2
            ),
            performance AS (
                SELECT sep.epoch_no AS epoch_no,
                       COALESCE(si.pool_id, sep.spo_sk) AS id_key,
                       'performance' AS source,
                       CAST(NULL AS TEXT) AS status
                FROM spo_epoch_performance sep
                LEFT JOIN spo_identity si ON si.spo_sk = sep.spo_sk
                WHERE sep.epoch_no BETWEEN $1 AND $2
            )
            SELECT epoch_no, id_key, source, status FROM history
            UNION ALL
            SELECT epoch_no, id_key, source, status FROM committee
            UNION ALL
            SELECT epoch_no, id_key, source, status FROM performance
            ORDER BY epoch_no, source, id_key
        "};

        sqlx::query_as::<_, (i64, String, String, Option<String>)>(query)
            .bind(start)
            .bind(end)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|(epoch_no, id_key, source, status)| PresenceEvent {
                        epoch_no,
                        id_key,
                        source,
                        status,
                    })
                    .collect()
            })
    }

    #[trace]
    async fn get_registered_first_valid_epochs(
        &self,
        upto_epoch: Option<i64>,
    ) -> Result<Vec<FirstValidEpoch>, sqlx::Error> {
        let query = indoc! {"
            SELECT COALESCE(si.pool_id, sh.spo_sk) AS id_key,
                   MIN(sh.epoch_no) AS first_valid_epoch
            FROM spo_history sh
            LEFT JOIN spo_identity si ON si.spo_sk = sh.spo_sk
            WHERE sh.status IN ('VALID','Valid')
              AND ($1 IS NULL OR sh.epoch_no <= $1)
            GROUP BY 1
            ORDER BY first_valid_epoch
        "};

        sqlx::query_as::<_, (String, i64)>(query)
            .bind(upto_epoch)
            .fetch_all(&*self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|(id_key, first_valid_epoch)| FirstValidEpoch {
                        id_key,
                        first_valid_epoch,
                    })
                    .collect()
            })
    }

    #[trace]
    async fn get_stake_distribution(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
        order_desc: bool,
    ) -> Result<(Vec<StakeShare>, f64), sqlx::Error> {
        // First get total live stake.
        let total_query = indoc! {"
            SELECT CAST(COALESCE(SUM(s.live_stake), 0) AS TEXT)
            FROM spo_stake_snapshot s
        "};
        let total_live_str: String = sqlx::query_scalar(total_query)
            .fetch_one(&*self.pool)
            .await?;
        let total_live_f64: f64 = total_live_str.parse().unwrap_or(0.0);

        // Build the main query.
        let base_select = if search.is_some() {
            #[cfg(feature = "cloud")]
            {
                indoc! {"
                    SELECT
                        pm.pool_id AS pool_id_hex,
                        pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url,
                        CAST(s.live_stake AS TEXT), CAST(s.active_stake AS TEXT), s.live_delegators, s.live_saturation,
                        CAST(s.declared_pledge AS TEXT), CAST(s.live_pledge AS TEXT)
                    FROM spo_stake_snapshot s
                    JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                    WHERE (
                        pm.name ILIKE $3
                        OR pm.ticker ILIKE $3
                        OR pm.homepage_url ILIKE $3
                        OR pm.pool_id ILIKE $4
                    )
                    ORDER BY COALESCE(s.live_stake, 0) DESC, pm.pool_id
                    LIMIT $1 OFFSET $2
                "}
            }

            #[cfg(feature = "standalone")]
            {
                indoc! {"
                    SELECT
                        pm.pool_id AS pool_id_hex,
                        pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url,
                        CAST(s.live_stake AS TEXT), CAST(s.active_stake AS TEXT), s.live_delegators, s.live_saturation,
                        CAST(s.declared_pledge AS TEXT), CAST(s.live_pledge AS TEXT)
                    FROM spo_stake_snapshot s
                    JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                    WHERE (
                        LOWER(pm.name) LIKE LOWER($3)
                        OR LOWER(pm.ticker) LIKE LOWER($3)
                        OR LOWER(pm.homepage_url) LIKE LOWER($3)
                        OR LOWER(pm.pool_id) LIKE LOWER($4)
                    )
                    ORDER BY COALESCE(s.live_stake, 0) DESC, pm.pool_id
                    LIMIT $1 OFFSET $2
                "}
            }
        } else {
            indoc! {"
                SELECT
                    pm.pool_id AS pool_id_hex,
                    pm.name, pm.ticker, pm.homepage_url, pm.url AS logo_url,
                    CAST(s.live_stake AS TEXT), CAST(s.active_stake AS TEXT), s.live_delegators, s.live_saturation,
                    CAST(s.declared_pledge AS TEXT), CAST(s.live_pledge AS TEXT)
                FROM spo_stake_snapshot s
                JOIN pool_metadata_cache pm ON pm.pool_id = s.pool_id
                ORDER BY COALESCE(s.live_stake, 0) DESC, pm.pool_id
                LIMIT $1 OFFSET $2
            "}
        };

        let sql = if order_desc {
            base_select.to_string()
        } else {
            base_select.replace("DESC", "ASC")
        };

        let rows = if let Some(s) = search {
            let s_like = format!("%{s}%");
            sqlx::query_as::<
                _,
                (
                    String,         // pool_id_hex
                    Option<String>, // name
                    Option<String>, // ticker
                    Option<String>, // homepage_url
                    Option<String>, // logo_url
                    Option<String>, // live_stake
                    Option<String>, // active_stake
                    Option<i32>,    // live_delegators
                    Option<f64>,    // live_saturation
                    Option<String>, // declared_pledge
                    Option<String>, // live_pledge
                ),
            >(&sql)
            .bind(limit)
            .bind(offset)
            .bind(s_like.clone())
            .bind(s_like)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<
                _,
                (
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<i32>,
                    Option<f64>,
                    Option<String>,
                    Option<String>,
                ),
            >(&sql)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*self.pool)
            .await?
        };

        let stake_shares = rows
            .into_iter()
            .map(
                |(
                    pool_id_hex,
                    name,
                    ticker,
                    homepage_url,
                    logo_url,
                    live_stake,
                    active_stake,
                    live_delegators,
                    live_saturation,
                    declared_pledge,
                    live_pledge,
                )| {
                    let share = {
                        let ls = live_stake.as_deref().unwrap_or("0");
                        let lv = ls.parse::<f64>().unwrap_or(0.0);
                        if total_live_f64 > 0.0 {
                            lv / total_live_f64
                        } else {
                            0.0
                        }
                    };
                    let live_delegators_i64 = live_delegators.map(|v| v as i64);
                    StakeShare {
                        pool_id_hex,
                        name,
                        ticker,
                        homepage_url,
                        logo_url,
                        live_stake,
                        active_stake,
                        live_delegators: live_delegators_i64,
                        live_saturation,
                        declared_pledge,
                        live_pledge,
                        stake_share: Some(share),
                    }
                },
            )
            .collect();

        Ok((stake_shares, total_live_f64))
    }
}

/// Row type for epoch performance query results.
type EpochPerfRow = (
    i64,
    String,
    i32,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Helper to convert epoch performance row to domain type.
fn epoch_perf_from_row(row: EpochPerfRow) -> EpochPerf {
    let (
        epoch_no,
        spo_sk_hex,
        produced_i32,
        expected_i32,
        identity_label,
        stake_snapshot,
        pool_id_hex,
        validator_class,
    ) = row;
    EpochPerf {
        epoch_no,
        spo_sk_hex,
        produced: produced_i32 as i64,
        expected: expected_i32 as i64,
        identity_label,
        stake_snapshot,
        pool_id_hex,
        validator_class,
    }
}

/// Normalize hex string by stripping 0x prefix and lowercasing.
fn normalize_hex(input: &str) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    let s = input
        .strip_prefix("0x")
        .unwrap_or(input)
        .strip_prefix("0X")
        .unwrap_or(input);
    if !s.len().is_multiple_of(2) || s.len() > 256 {
        return None;
    }
    // Validate hex characters.
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(s.to_ascii_lowercase())
}
