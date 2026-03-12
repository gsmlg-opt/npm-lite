use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "upstream_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Rule type: "global", "scope", or "pattern"
    pub rule_type: String,
    /// Scope name, regex pattern, or "*" for global
    pub match_value: String,
    /// Target URL or "local"
    pub upstream_url: String,
    /// Optional auth token reference (e.g. "env:VAR_NAME")
    pub auth_token_ref: Option<String>,
    /// Evaluation order (lower = higher priority)
    pub priority: i32,
    /// Soft-disable without deleting
    pub enabled: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
