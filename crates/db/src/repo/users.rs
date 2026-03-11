use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter,
};
use uuid::Uuid;

use npm_entity::users::{self, Column, Entity as UserEntity};

use crate::error::Result;

pub struct UserRepo;

impl UserRepo {
    /// Find a user by username.
    pub async fn find_by_username(
        db: &DatabaseConnection,
        username: &str,
    ) -> Result<Option<users::Model>> {
        let model = UserEntity::find()
            .filter(Column::Username.eq(username))
            .one(db)
            .await?;
        Ok(model)
    }

    /// Create a new user.
    pub async fn create(
        db: &DatabaseConnection,
        username: impl Into<String>,
        password_hash: impl Into<String>,
        email: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<users::Model> {
        let now = chrono::Utc::now().fixed_offset();
        let active = users::ActiveModel {
            id: Set(Uuid::new_v4()),
            username: Set(username.into()),
            password_hash: Set(password_hash.into()),
            email: Set(email.into()),
            role: Set(role.into()),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// List all users.
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<users::Model>> {
        let models = UserEntity::find().all(db).await?;
        Ok(models)
    }
}
