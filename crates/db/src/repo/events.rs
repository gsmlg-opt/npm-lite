use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use npm_entity::publish_events::{self, Column, Entity as EventEntity};

use crate::error::Result;

pub struct EventRepo;

impl EventRepo {
    /// Record a publish or unpublish event.
    pub async fn record(
        db: &DatabaseConnection,
        package_id: Uuid,
        version_id: Option<Uuid>,
        action: impl Into<String>,
        actor_id: Uuid,
        success: bool,
        error: Option<String>,
    ) -> Result<()> {
        let active = publish_events::ActiveModel {
            id: Set(Uuid::new_v4()),
            package_id: Set(package_id),
            version_id: Set(version_id),
            action: Set(action.into()),
            actor_id: Set(actor_id),
            success: Set(success),
            error_message: Set(error),
            created_at: Set(chrono::Utc::now().fixed_offset()),
        };
        active.insert(db).await?;
        Ok(())
    }

    /// List the most recent events, newest first.
    pub async fn list_recent(
        db: &DatabaseConnection,
        limit: u64,
    ) -> Result<Vec<publish_events::Model>> {
        let models = EventEntity::find()
            .order_by_desc(Column::CreatedAt)
            .limit(limit)
            .all(db)
            .await?;
        Ok(models)
    }
}
