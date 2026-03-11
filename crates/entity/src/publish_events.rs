use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "publish_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub package_id: Uuid,
    pub version_id: Option<Uuid>,
    /// Action: "publish" or "unpublish"
    pub action: String,
    pub actor_id: Uuid,
    pub success: bool,
    pub error_message: Option<String>,
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
    #[sea_orm(
        belongs_to = "super::package_versions::Entity",
        from = "Column::VersionId",
        to = "super::package_versions::Column::Id"
    )]
    PackageVersion,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorId",
        to = "super::users::Column::Id"
    )]
    Actor,
}

impl Related<super::packages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Package.def()
    }
}

impl Related<super::package_versions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PackageVersion.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
