use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "packages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub name: String,
    pub scope: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::package_versions::Entity")]
    PackageVersions,
    #[sea_orm(has_many = "super::dist_tags::Entity")]
    DistTags,
    #[sea_orm(has_many = "super::package_acl::Entity")]
    PackageAcl,
    #[sea_orm(has_many = "super::publish_events::Entity")]
    PublishEvents,
}

impl Related<super::package_versions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PackageVersions.def()
    }
}

impl Related<super::dist_tags::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DistTags.def()
    }
}

impl Related<super::package_acl::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PackageAcl.def()
    }
}

impl Related<super::publish_events::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PublishEvents.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
