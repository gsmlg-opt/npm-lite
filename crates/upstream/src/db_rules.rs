//! CRUD operations for database-stored upstream routing rules.

use chrono::Utc;
use npm_entity::upstream_configs;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Order, QueryOrder, Set};
use uuid::Uuid;

use crate::config::{PatternRule, UpstreamConfig};

/// A rule loaded from the database, with its ID for admin operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpstreamRule {
    pub id: Uuid,
    pub rule_type: String,
    pub match_value: String,
    pub upstream_url: String,
    pub auth_token_ref: Option<String>,
    pub priority: i32,
    pub enabled: bool,
}

impl From<upstream_configs::Model> for UpstreamRule {
    fn from(m: upstream_configs::Model) -> Self {
        Self {
            id: m.id,
            rule_type: m.rule_type,
            match_value: m.match_value,
            upstream_url: m.upstream_url,
            auth_token_ref: m.auth_token_ref,
            priority: m.priority,
            enabled: m.enabled,
        }
    }
}

/// List all upstream routing rules, ordered by priority ascending.
pub async fn list_rules(db: &DatabaseConnection) -> Result<Vec<UpstreamRule>, sea_orm::DbErr> {
    let models = upstream_configs::Entity::find()
        .order_by(upstream_configs::Column::Priority, Order::Asc)
        .order_by(upstream_configs::Column::CreatedAt, Order::Asc)
        .all(db)
        .await?;
    Ok(models.into_iter().map(UpstreamRule::from).collect())
}

/// Get a single rule by ID.
pub async fn get_rule(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<UpstreamRule>, sea_orm::DbErr> {
    let model = upstream_configs::Entity::find_by_id(id).one(db).await?;
    Ok(model.map(UpstreamRule::from))
}

/// Input for creating or updating a rule.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuleInput {
    pub rule_type: String,
    pub match_value: String,
    pub upstream_url: String,
    pub auth_token_ref: Option<String>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
}

/// Create a new upstream routing rule.
pub async fn create_rule(
    db: &DatabaseConnection,
    input: RuleInput,
) -> Result<UpstreamRule, sea_orm::DbErr> {
    let now = Utc::now().into();
    let model = upstream_configs::ActiveModel {
        id: Set(Uuid::new_v4()),
        rule_type: Set(input.rule_type),
        match_value: Set(input.match_value),
        upstream_url: Set(input.upstream_url),
        auth_token_ref: Set(input.auth_token_ref),
        priority: Set(input.priority.unwrap_or(0)),
        enabled: Set(input.enabled.unwrap_or(true)),
        created_at: Set(now),
        updated_at: Set(now),
    };
    let result = model.insert(db).await?;
    Ok(UpstreamRule::from(result))
}

/// Update an existing upstream routing rule.
pub async fn update_rule(
    db: &DatabaseConnection,
    id: Uuid,
    input: RuleInput,
) -> Result<Option<UpstreamRule>, sea_orm::DbErr> {
    let existing = upstream_configs::Entity::find_by_id(id).one(db).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let now = Utc::now().into();
    let mut active: upstream_configs::ActiveModel = existing.into();
    active.rule_type = Set(input.rule_type);
    active.match_value = Set(input.match_value);
    active.upstream_url = Set(input.upstream_url);
    active.auth_token_ref = Set(input.auth_token_ref);
    if let Some(priority) = input.priority {
        active.priority = Set(priority);
    }
    if let Some(enabled) = input.enabled {
        active.enabled = Set(enabled);
    }
    active.updated_at = Set(now);
    let result = active.update(db).await?;
    Ok(Some(UpstreamRule::from(result)))
}

/// Delete a routing rule by ID.
pub async fn delete_rule(db: &DatabaseConnection, id: Uuid) -> Result<bool, sea_orm::DbErr> {
    let result = upstream_configs::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

/// Apply database-stored rules onto an existing UpstreamConfig.
///
/// DB rules are merged with file/env rules. DB rules for scope/pattern
/// types are appended (they don't replace file-based rules).
pub fn apply_db_rules(config: &mut UpstreamConfig, rules: &[UpstreamRule]) {
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        match rule.rule_type.as_str() {
            "global" => {
                // DB global rule sets/overrides the upstream URL if not already set from env.
                if config.upstream_url.is_none() {
                    config.upstream_url = Some(rule.upstream_url.clone());
                }
            }
            "scope" => {
                if rule.upstream_url == "local" {
                    if !config.local_scopes.contains(&rule.match_value) {
                        config.local_scopes.push(rule.match_value.clone());
                    }
                } else {
                    config
                        .scope_rules
                        .entry(rule.match_value.clone())
                        .or_insert_with(|| rule.upstream_url.clone());
                }
            }
            "pattern" => {
                config.pattern_rules.push(PatternRule {
                    pattern: rule.match_value.clone(),
                    target: rule.upstream_url.clone(),
                });
            }
            _ => {
                tracing::warn!(rule_type = %rule.rule_type, "unknown upstream rule type");
            }
        }
    }
}
