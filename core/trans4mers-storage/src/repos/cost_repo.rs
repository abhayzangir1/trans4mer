use chrono::Utc;
use rusqlite::{Connection, params};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::{CostBudget, CostEntry};

pub fn log_cost(conn: &Connection, entry: &CostEntry) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO cost_entries (
            id, provider, model, project_id, execution_id,
            input_tokens, output_tokens, cost_usd, is_fallback, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            entry.id,
            entry.provider,
            entry.model,
            entry.project_id,
            entry.execution_id,
            entry.input_tokens,
            entry.output_tokens,
            entry.cost_usd,
            if entry.is_fallback { 1 } else { 0 },
            entry.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_daily_cost(
    conn: &Connection,
    provider: &str,
    project_id: Option<&str>,
) -> Result<f32, Trans4mersError> {
    let now = Utc::now();
    let today_start = now.format("%Y-%m-%dT00:00:00+00:00").to_string();

    let cost: f32 = if let Some(p_id) = project_id {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_entries
             WHERE provider = ?1 AND project_id = ?2 AND created_at >= ?3",
            params![provider, p_id, today_start],
            |r| r.get(0),
        )
        .unwrap_or(0.0)
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_entries
             WHERE provider = ?1 AND created_at >= ?2",
            params![provider, today_start],
            |r| r.get(0),
        )
        .unwrap_or(0.0)
    };

    Ok(cost)
}

pub fn get_monthly_cost(
    conn: &Connection,
    provider: &str,
    project_id: Option<&str>,
) -> Result<f32, Trans4mersError> {
    let now = Utc::now();
    let month_start = now.format("%Y-%m-01T00:00:00+00:00").to_string();

    let cost: f32 = if let Some(p_id) = project_id {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_entries
             WHERE provider = ?1 AND project_id = ?2 AND created_at >= ?3",
            params![provider, p_id, month_start],
            |r| r.get(0),
        )
        .unwrap_or(0.0)
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_entries
             WHERE provider = ?1 AND created_at >= ?2",
            params![provider, month_start],
            |r| r.get(0),
        )
        .unwrap_or(0.0)
    };

    Ok(cost)
}

pub fn get_budget(
    conn: &Connection,
    provider: &str,
    project_id: Option<&str>,
) -> Result<Option<CostBudget>, Trans4mersError> {
    let mut stmt = if let Some(_p_id) = project_id {
        conn.prepare(
            "SELECT id, project_id, provider, monthly_ceiling_usd, daily_ceiling_usd,
                    hard_block, alert_thresholds, created_at, updated_at
             FROM cost_budgets WHERE provider = ?1 AND project_id = ?2",
        )
    } else {
        conn.prepare(
            "SELECT id, project_id, provider, monthly_ceiling_usd, daily_ceiling_usd,
                    hard_block, alert_thresholds, created_at, updated_at
             FROM cost_budgets WHERE provider = ?1 AND project_id IS NULL",
        )
    }
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = if let Some(p_id) = project_id {
        stmt.query(params![provider, p_id])
    } else {
        stmt.query(params![provider])
    }
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_budget(row)?))
    } else {
        Ok(None)
    }
}

pub fn upsert_budget(conn: &Connection, budget: &CostBudget) -> Result<(), Trans4mersError> {
    let thresholds_json = serde_json::to_string(&budget.alert_thresholds)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO cost_budgets (
            id, project_id, provider, monthly_ceiling_usd, daily_ceiling_usd,
            hard_block, alert_thresholds, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            monthly_ceiling_usd = excluded.monthly_ceiling_usd,
            daily_ceiling_usd = excluded.daily_ceiling_usd,
            hard_block = excluded.hard_block,
            alert_thresholds = excluded.alert_thresholds,
            updated_at = excluded.updated_at",
        params![
            budget.id,
            budget.project_id,
            budget.provider,
            budget.monthly_ceiling_usd,
            budget.daily_ceiling_usd,
            if budget.hard_block { 1 } else { 0 },
            thresholds_json,
            budget.created_at.to_rfc3339(),
            budget.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_budgets(conn: &Connection) -> Result<Vec<CostBudget>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, provider, monthly_ceiling_usd, daily_ceiling_usd,
                hard_block, alert_thresholds, created_at, updated_at
         FROM cost_budgets",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], row_to_budget)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

fn row_to_budget(row: &rusqlite::Row) -> rusqlite::Result<CostBudget> {
    let id: String = row.get(0)?;
    let project_id: Option<String> = row.get(1)?;
    let provider: String = row.get(2)?;
    let monthly_ceiling_usd: Option<f32> = row.get(3)?;
    let daily_ceiling_usd: Option<f32> = row.get(4)?;
    let hard_block_int: i32 = row.get(5)?;
    let alerts_json: String = row.get(6)?;
    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;

    let alert_thresholds: Vec<f32> =
        serde_json::from_str(&alerts_json).unwrap_or_else(|_| vec![0.5, 0.8, 1.0]);

    Ok(CostBudget {
        id,
        project_id,
        provider,
        monthly_ceiling_usd,
        daily_ceiling_usd,
        hard_block: hard_block_int != 0,
        alert_thresholds,
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
    })
}
