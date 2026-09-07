use rusqlite::{Connection, params};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::TokenUsage;

pub fn insert_token_usage(conn: &Connection, usage: &TokenUsage) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO token_usages (
            id, execution_id, agent_instance_id, conversation_id,
            provider, model, prompt_tokens, completion_tokens,
            total_tokens, estimated_cost_usd, compute_time_ms, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            usage.id.as_str(),
            usage.execution_id.as_str(),
            usage.agent_instance_id.as_str(),
            usage.conversation_id.as_str(),
            usage.provider,
            usage.model,
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
            usage.estimated_cost_usd,
            usage.compute_time_ms,
            usage.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_token_usages_by_execution(
    conn: &Connection,
    execution_id: &trans4mers_domain::ids::ExecutionId,
) -> Result<Vec<TokenUsage>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, execution_id, agent_instance_id, conversation_id,
                    provider, model, prompt_tokens, completion_tokens,
                    total_tokens, estimated_cost_usd, compute_time_ms, created_at
             FROM token_usages
             WHERE execution_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![execution_id.as_str()], |row| {
            let id_str: String = row.get(0)?;
            let exec_str: String = row.get(1)?;
            let agent_str: String = row.get(2)?;
            let conv_str: String = row.get(3)?;
            let provider: String = row.get(4)?;
            let model: String = row.get(5)?;
            let prompt_tokens: u32 = row.get(6)?;
            let completion_tokens: u32 = row.get(7)?;
            let total_tokens: u32 = row.get(8)?;
            let estimated_cost_usd: Option<f64> = row.get(9)?;
            let compute_time_ms: i64 = row.get(10)?;
            let created_at_str: String = row.get(11)?;

            let id = trans4mers_domain::ids::TokenUsageId::from_str(&id_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let execution_id =
                trans4mers_domain::ids::ExecutionId::from_str(&exec_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
            let agent_instance_id = trans4mers_domain::ids::AgentInstanceId::from_str(&agent_str)
                .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let conversation_id = trans4mers_domain::ids::ConversationId::from_str(&conv_str)
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;

            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(TokenUsage {
                id,
                execution_id,
                agent_instance_id,
                conversation_id,
                provider,
                model,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                estimated_cost_usd,
                compute_time_ms: compute_time_ms as u64,
                created_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut usages = Vec::new();
    for row in rows {
        usages.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(usages)
}

pub fn get_total_tokens_by_execution(
    conn: &Connection,
    execution_id: &trans4mers_domain::ids::ExecutionId,
) -> Result<i64, Trans4mersError> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(SUM(total_tokens), 0) FROM token_usages WHERE execution_id = ?1")
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let total: i64 = stmt
        .query_row(params![execution_id.as_str()], |row| row.get(0))
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(total)
}
