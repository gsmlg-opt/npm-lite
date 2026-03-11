use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use uuid::Uuid;

use npm_entity::package_versions::{self, Column, Entity as VersionEntity};

use crate::error::Result;

pub struct VersionRepo;

impl VersionRepo {
    /// Find a specific version of a package (including soft-deleted ones).
    pub async fn find_by_package_and_version(
        db: &DatabaseConnection,
        package_id: Uuid,
        version: &str,
    ) -> Result<Option<package_versions::Model>> {
        let model = VersionEntity::find()
            .filter(Column::PackageId.eq(package_id))
            .filter(Column::Version.eq(version))
            .one(db)
            .await?;
        Ok(model)
    }

    /// List all non-deleted versions for a package, ordered by creation time.
    pub async fn list_by_package(
        db: &DatabaseConnection,
        package_id: Uuid,
    ) -> Result<Vec<package_versions::Model>> {
        let models = VersionEntity::find()
            .filter(Column::PackageId.eq(package_id))
            .filter(Column::DeletedAt.is_null())
            .all(db)
            .await?;
        Ok(models)
    }

    /// Soft-delete a version by setting `deleted_at` to now.
    pub async fn soft_delete(db: &DatabaseConnection, version_id: Uuid) -> Result<()> {
        let version = VersionEntity::find_by_id(version_id).one(db).await?;
        let version = match version {
            Some(v) => v,
            None => return Ok(()),
        };

        let mut active: package_versions::ActiveModel = version.into();
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update(db).await?;
        Ok(())
    }

    /// Count total (non-deleted) versions across all packages.
    pub async fn count(db: &DatabaseConnection) -> Result<u64> {
        let n = VersionEntity::find()
            .filter(Column::DeletedAt.is_null())
            .count(db)
            .await?;
        Ok(n)
    }
}
