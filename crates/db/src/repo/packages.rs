use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use npm_entity::packages::{self, Column, Entity as PackageEntity};

use crate::error::Result;

pub struct PackageRepo;

impl PackageRepo {
    /// Find a package by its full name (e.g. "react" or "@scope/pkg").
    pub async fn find_by_name(
        db: &DatabaseConnection,
        name: &str,
    ) -> Result<Option<packages::Model>> {
        let model = PackageEntity::find()
            .filter(Column::Name.eq(name))
            .one(db)
            .await?;
        Ok(model)
    }

    /// Create a new package record.
    pub async fn create(
        db: &DatabaseConnection,
        name: impl Into<String>,
        scope: Option<String>,
        description: Option<String>,
    ) -> Result<packages::Model> {
        let now = chrono::Utc::now().fixed_offset();
        let active = packages::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name.into()),
            scope: Set(scope),
            description: Set(description),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// List packages with optional text search, pagination.
    pub async fn list(
        db: &DatabaseConnection,
        search: Option<&str>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<packages::Model>> {
        let mut query = PackageEntity::find().order_by_asc(Column::Name);
        if let Some(term) = search {
            query = query.filter(Column::Name.contains(term));
        }
        let models = query.offset(offset).limit(limit).all(db).await?;
        Ok(models)
    }

    /// Count total packages in the registry.
    pub async fn count(db: &DatabaseConnection) -> Result<u64> {
        let n = PackageEntity::find().count(db).await?;
        Ok(n)
    }
}
