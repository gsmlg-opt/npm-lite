use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter,
};
use uuid::Uuid;

use npm_entity::tokens::{self, Column, Entity as TokenEntity};

use crate::error::Result;

pub struct TokenRepo;

impl TokenRepo {
    /// Create a new authentication token.
    pub async fn create(
        db: &DatabaseConnection,
        user_id: Uuid,
        token_hash: impl Into<String>,
        role: impl Into<String>,
        name: Option<String>,
    ) -> Result<tokens::Model> {
        let active = tokens::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            token_hash: Set(token_hash.into()),
            role: Set(role.into()),
            name: Set(name),
            created_at: Set(chrono::Utc::now().fixed_offset()),
            revoked_at: Set(None),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// Find a token by its hash. Returns `None` if not found or already revoked.
    pub async fn find_by_hash(
        db: &DatabaseConnection,
        token_hash: &str,
    ) -> Result<Option<tokens::Model>> {
        let model = TokenEntity::find()
            .filter(Column::TokenHash.eq(token_hash))
            .filter(Column::RevokedAt.is_null())
            .one(db)
            .await?;
        Ok(model)
    }

    /// List all non-revoked tokens for a user.
    pub async fn list_by_user(
        db: &DatabaseConnection,
        user_id: Uuid,
    ) -> Result<Vec<tokens::Model>> {
        let models = TokenEntity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::RevokedAt.is_null())
            .all(db)
            .await?;
        Ok(models)
    }

    /// Revoke a token by setting `revoked_at` to now.
    pub async fn revoke(db: &DatabaseConnection, token_id: Uuid) -> Result<()> {
        let token = TokenEntity::find_by_id(token_id).one(db).await?;
        let token = match token {
            Some(t) => t,
            None => return Ok(()),
        };
        let mut active: tokens::ActiveModel = token.into();
        active.revoked_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update(db).await?;
        Ok(())
    }
}
