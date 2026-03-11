use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter,
};
use uuid::Uuid;

use npm_entity::package_acl::{self, Column, Entity as AclEntity};

use crate::error::Result;

/// Permission levels in ascending order of privilege.
static PERMISSION_ORDER: &[&str] = &["read", "publish", "admin"];

fn permission_level(perm: &str) -> usize {
    PERMISSION_ORDER
        .iter()
        .position(|&p| p == perm)
        .unwrap_or(0)
}

pub struct AclRepo;

impl AclRepo {
    /// Check whether a user has at least `required_role` on a package.
    ///
    /// Checks direct user grants first, then team-based grants.
    pub async fn check_permission(
        db: &DatabaseConnection,
        user_id: Uuid,
        package_name: &str,
        required_role: &str,
    ) -> Result<bool> {
        // Find the package to get its ID and scope.
        use npm_entity::packages::Column as PkgCol;
        use npm_entity::packages::Entity as PkgEntity;

        let package = PkgEntity::find()
            .filter(PkgCol::Name.eq(package_name))
            .one(db)
            .await?;

        let (package_id, scope) = match package {
            Some(p) => (p.id, p.scope),
            None => return Ok(false),
        };

        // Collect all ACL entries that might apply: direct user entries for
        // this package or scope, or team entries where the user is a member.
        let mut pkg_or_scope_cond = sea_orm::Condition::any()
            .add(Column::PackageId.eq(package_id));
        if let Some(ref s) = scope {
            pkg_or_scope_cond = pkg_or_scope_cond.add(Column::Scope.eq(s.as_str()));
        }

        let direct_entries = AclEntity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(pkg_or_scope_cond)
            .all(db)
            .await?;

        let required_level = permission_level(required_role);
        for entry in &direct_entries {
            if permission_level(&entry.permission) >= required_level {
                return Ok(true);
            }
        }

        // Check team-based grants.
        use npm_entity::team_members::Column as MemberCol;
        use npm_entity::team_members::Entity as MemberEntity;

        let memberships = MemberEntity::find()
            .filter(MemberCol::UserId.eq(user_id))
            .all(db)
            .await?;

        for membership in memberships {
            let team_entries = AclEntity::find()
                .filter(Column::TeamId.eq(membership.team_id))
                .filter(
                    sea_orm::Condition::any()
                        .add(Column::PackageId.eq(package_id))
                        .add(Column::PackageId.is_null()),
                )
                .all(db)
                .await?;

            for entry in team_entries {
                if permission_level(&entry.permission) >= required_level {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Grant a permission to a user or team on a package or scope.
    pub async fn grant(
        db: &DatabaseConnection,
        package_id: Option<Uuid>,
        scope: Option<String>,
        user_id: Option<Uuid>,
        team_id: Option<Uuid>,
        permission: impl Into<String>,
    ) -> Result<package_acl::Model> {
        let active = package_acl::ActiveModel {
            id: Set(Uuid::new_v4()),
            package_id: Set(package_id),
            scope: Set(scope),
            user_id: Set(user_id),
            team_id: Set(team_id),
            permission: Set(permission.into()),
            created_at: Set(chrono::Utc::now().fixed_offset()),
        };
        let model = active.insert(db).await?;
        Ok(model)
    }

    /// Revoke an ACL entry by its ID.
    pub async fn revoke(db: &DatabaseConnection, acl_id: Uuid) -> Result<()> {
        AclEntity::delete_by_id(acl_id).exec(db).await?;
        Ok(())
    }

    /// Revoke all grants for a user on a specific package.
    pub async fn revoke_user_package(
        db: &DatabaseConnection,
        user_id: Uuid,
        package_id: Uuid,
    ) -> Result<()> {
        AclEntity::delete_many()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::PackageId.eq(package_id))
            .exec(db)
            .await?;
        Ok(())
    }

    /// List all ACL entries for a package.
    pub async fn list_for_package(
        db: &DatabaseConnection,
        package_id: Uuid,
    ) -> Result<Vec<package_acl::Model>> {
        let models = AclEntity::find()
            .filter(Column::PackageId.eq(package_id))
            .all(db)
            .await?;
        Ok(models)
    }
}
