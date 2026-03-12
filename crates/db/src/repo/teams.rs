use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use npm_entity::team_members::{self, Entity as TeamMemberEntity};
use npm_entity::teams::{self, Column, Entity as TeamEntity};

use crate::error::Result;

pub struct TeamRepo;

impl TeamRepo {
    /// Find a team by name.
    pub async fn find_by_name(db: &DatabaseConnection, name: &str) -> Result<Option<teams::Model>> {
        let model = TeamEntity::find()
            .filter(Column::Name.eq(name))
            .one(db)
            .await?;
        Ok(model)
    }

    /// Find a team by ID.
    pub async fn find_by_id(
        db: &DatabaseConnection,
        team_id: Uuid,
    ) -> Result<Option<teams::Model>> {
        let model = TeamEntity::find_by_id(team_id).one(db).await?;
        Ok(model)
    }

    /// Create a new team.
    pub async fn create(
        db: &DatabaseConnection,
        name: impl Into<String>,
        description: Option<String>,
    ) -> Result<teams::Model> {
        let active = teams::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name.into()),
            description: Set(description),
            created_at: Set(chrono::Utc::now().fixed_offset()),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// List all teams.
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<teams::Model>> {
        let models = TeamEntity::find().all(db).await?;
        Ok(models)
    }

    /// Delete a team by ID.
    pub async fn delete(db: &DatabaseConnection, team_id: Uuid) -> Result<()> {
        TeamEntity::delete_by_id(team_id).exec(db).await?;
        Ok(())
    }

    /// Add a user to a team.
    pub async fn add_member(
        db: &DatabaseConnection,
        team_id: Uuid,
        user_id: Uuid,
    ) -> Result<team_members::Model> {
        let active = team_members::ActiveModel {
            id: Set(Uuid::new_v4()),
            team_id: Set(team_id),
            user_id: Set(user_id),
            created_at: Set(chrono::Utc::now().fixed_offset()),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// Remove a user from a team.
    pub async fn remove_member(
        db: &DatabaseConnection,
        team_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        use npm_entity::team_members::Column as MemberColumn;
        TeamMemberEntity::delete_many()
            .filter(MemberColumn::TeamId.eq(team_id))
            .filter(MemberColumn::UserId.eq(user_id))
            .exec(db)
            .await?;
        Ok(())
    }

    /// List all members of a team.
    pub async fn list_members(
        db: &DatabaseConnection,
        team_id: Uuid,
    ) -> Result<Vec<team_members::Model>> {
        use npm_entity::team_members::Column as MemberColumn;
        let models = TeamMemberEntity::find()
            .filter(MemberColumn::TeamId.eq(team_id))
            .all(db)
            .await?;
        Ok(models)
    }
}
