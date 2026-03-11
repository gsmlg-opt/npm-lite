use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub username: String,
    pub password_hash: String,
    pub email: String,
    /// Role: "read", "publish", or "admin"
    pub role: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::tokens::Entity")]
    Tokens,
    #[sea_orm(has_many = "super::team_members::Entity")]
    TeamMembers,
    #[sea_orm(has_many = "super::package_acl::Entity")]
    PackageAcl,
    #[sea_orm(has_many = "super::publish_events::Entity")]
    PublishEvents,
}

impl Related<super::tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tokens.def()
    }
}

impl Related<super::team_members::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TeamMembers.def()
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
