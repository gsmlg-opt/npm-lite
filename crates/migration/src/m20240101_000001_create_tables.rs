use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000001_create_tables"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── users ──────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(
                        ColumnDef::new(Users::Username)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Users::PasswordHash).text().not_null())
                    .col(
                        ColumnDef::new(Users::Email)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Users::Role)
                            .text()
                            .not_null()
                            .default("read"),
                    )
                    .col(
                        ColumnDef::new(Users::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(Users::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .to_owned(),
            )
            .await?;

        // ── teams ──────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Teams::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Teams::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(
                        ColumnDef::new(Teams::Name)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Teams::Description).text().null())
                    .col(
                        ColumnDef::new(Teams::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .to_owned(),
            )
            .await?;

        // ── team_members ───────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(TeamMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TeamMembers::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(TeamMembers::TeamId).uuid().not_null())
                    .col(ColumnDef::new(TeamMembers::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(TeamMembers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_team_id")
                            .from(TeamMembers::Table, TeamMembers::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_user_id")
                            .from(TeamMembers::Table, TeamMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_team_members_team_user")
                    .table(TeamMembers::Table)
                    .col(TeamMembers::TeamId)
                    .col(TeamMembers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── tokens ─────────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Tokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Tokens::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(Tokens::UserId).uuid().not_null())
                    .col(ColumnDef::new(Tokens::TokenHash).text().not_null())
                    .col(
                        ColumnDef::new(Tokens::Role)
                            .text()
                            .not_null()
                            .default("read"),
                    )
                    .col(ColumnDef::new(Tokens::Name).text().null())
                    .col(
                        ColumnDef::new(Tokens::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(Tokens::RevokedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tokens_user_id")
                            .from(Tokens::Table, Tokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // ── packages ───────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Packages::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Packages::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(
                        ColumnDef::new(Packages::Name)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Packages::Scope).text().null())
                    .col(ColumnDef::new(Packages::Description).text().null())
                    .col(
                        ColumnDef::new(Packages::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(Packages::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .to_owned(),
            )
            .await?;

        // ── package_versions ───────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(PackageVersions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PackageVersions::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(PackageVersions::PackageId).uuid().not_null())
                    .col(ColumnDef::new(PackageVersions::Version).text().not_null())
                    .col(ColumnDef::new(PackageVersions::S3Key).text().not_null())
                    .col(ColumnDef::new(PackageVersions::Sha512).binary().not_null())
                    .col(ColumnDef::new(PackageVersions::Shasum).text().not_null())
                    .col(ColumnDef::new(PackageVersions::Integrity).text().not_null())
                    .col(ColumnDef::new(PackageVersions::Size).big_integer().not_null())
                    .col(
                        ColumnDef::new(PackageVersions::Metadata)
                            .json()
                            .not_null()
                            .default(Expr::cust("'{}'")),
                    )
                    .col(
                        ColumnDef::new(PackageVersions::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(PackageVersions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_package_versions_package_id")
                            .from(PackageVersions::Table, PackageVersions::PackageId)
                            .to(Packages::Table, Packages::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_package_versions_package_version")
                    .table(PackageVersions::Table)
                    .col(PackageVersions::PackageId)
                    .col(PackageVersions::Version)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── dist_tags ──────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(DistTags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DistTags::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(DistTags::PackageId).uuid().not_null())
                    .col(ColumnDef::new(DistTags::Tag).text().not_null())
                    .col(ColumnDef::new(DistTags::VersionId).uuid().not_null())
                    .col(
                        ColumnDef::new(DistTags::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(DistTags::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_dist_tags_package_id")
                            .from(DistTags::Table, DistTags::PackageId)
                            .to(Packages::Table, Packages::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_dist_tags_version_id")
                            .from(DistTags::Table, DistTags::VersionId)
                            .to(PackageVersions::Table, PackageVersions::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_dist_tags_package_tag")
                    .table(DistTags::Table)
                    .col(DistTags::PackageId)
                    .col(DistTags::Tag)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── package_acl ────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(PackageAcl::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PackageAcl::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(PackageAcl::PackageId).uuid().null())
                    .col(ColumnDef::new(PackageAcl::Scope).text().null())
                    .col(ColumnDef::new(PackageAcl::TeamId).uuid().null())
                    .col(ColumnDef::new(PackageAcl::UserId).uuid().null())
                    .col(
                        ColumnDef::new(PackageAcl::Permission)
                            .text()
                            .not_null()
                            .default("read"),
                    )
                    .col(
                        ColumnDef::new(PackageAcl::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_package_acl_package_id")
                            .from(PackageAcl::Table, PackageAcl::PackageId)
                            .to(Packages::Table, Packages::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_package_acl_team_id")
                            .from(PackageAcl::Table, PackageAcl::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_package_acl_user_id")
                            .from(PackageAcl::Table, PackageAcl::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // ── publish_events ─────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(PublishEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PublishEvents::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(PublishEvents::PackageId).uuid().not_null())
                    .col(ColumnDef::new(PublishEvents::VersionId).uuid().null())
                    .col(ColumnDef::new(PublishEvents::Action).text().not_null())
                    .col(ColumnDef::new(PublishEvents::ActorId).uuid().not_null())
                    .col(
                        ColumnDef::new(PublishEvents::Success)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(PublishEvents::ErrorMessage).text().null())
                    .col(
                        ColumnDef::new(PublishEvents::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_publish_events_package_id")
                            .from(PublishEvents::Table, PublishEvents::PackageId)
                            .to(Packages::Table, Packages::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_publish_events_version_id")
                            .from(PublishEvents::Table, PublishEvents::VersionId)
                            .to(PackageVersions::Table, PackageVersions::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_publish_events_actor_id")
                            .from(PublishEvents::Table, PublishEvents::ActorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PublishEvents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PackageAcl::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(DistTags::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PackageVersions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Packages::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Tokens::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(TeamMembers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Teams::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
    Username,
    PasswordHash,
    Email,
    Role,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Teams {
    Table,
    Id,
    Name,
    Description,
    CreatedAt,
}

#[derive(Iden)]
enum TeamMembers {
    Table,
    Id,
    TeamId,
    UserId,
    CreatedAt,
}

#[derive(Iden)]
enum Tokens {
    Table,
    Id,
    UserId,
    TokenHash,
    Role,
    Name,
    CreatedAt,
    RevokedAt,
}

#[derive(Iden)]
enum Packages {
    Table,
    Id,
    Name,
    Scope,
    Description,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum PackageVersions {
    Table,
    Id,
    PackageId,
    Version,
    S3Key,
    Sha512,
    Shasum,
    Integrity,
    Size,
    Metadata,
    DeletedAt,
    CreatedAt,
}

#[derive(Iden)]
enum DistTags {
    Table,
    Id,
    PackageId,
    Tag,
    VersionId,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum PackageAcl {
    Table,
    Id,
    PackageId,
    Scope,
    TeamId,
    UserId,
    Permission,
    CreatedAt,
}

#[derive(Iden)]
enum PublishEvents {
    Table,
    Id,
    PackageId,
    VersionId,
    Action,
    ActorId,
    Success,
    ErrorMessage,
    CreatedAt,
}
