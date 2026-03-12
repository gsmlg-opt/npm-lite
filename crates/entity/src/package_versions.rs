use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "package_versions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub package_id: Uuid,
    pub version: String,
    pub s3_key: String,
    /// Binary SHA-512 digest of the tarball
    pub sha512: Vec<u8>,
    /// Hex-encoded SHA-1 shasum (npm legacy field)
    pub shasum: String,
    /// Subresource Integrity string (e.g. "sha512-<base64>")
    pub integrity: String,
    pub size: i64,
    /// Full package.json metadata stored as JSON
    pub metadata: Json,
    /// Package source: "local" or "upstream".
    #[sea_orm(default_value = "local")]
    pub source: String,
    /// Origin upstream URL for cached/proxied versions (null for local).
    pub upstream_url: Option<String>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::packages::Entity",
        from = "Column::PackageId",
        to = "super::packages::Column::Id"
    )]
    Package,
    #[sea_orm(has_many = "super::dist_tags::Entity")]
    DistTags,
    #[sea_orm(has_many = "super::publish_events::Entity")]
    PublishEvents,
}

impl Related<super::packages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Package.def()
    }
}

impl Related<super::dist_tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DistTags.def()
    }
}

impl Related<super::publish_events::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PublishEvents.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
