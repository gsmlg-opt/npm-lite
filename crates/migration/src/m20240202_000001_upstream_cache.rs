use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240202_000001_upstream_cache"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── upstream_cache ──────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(UpstreamCache::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UpstreamCache::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(
                        ColumnDef::new(UpstreamCache::PackageName)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(UpstreamCache::UpstreamUrl).text().not_null())
                    .col(
                        ColumnDef::new(UpstreamCache::PackumentJson)
                            .json()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UpstreamCache::FetchedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(UpstreamCache::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on package_name for fast lookups (unique constraint already creates one,
        // but being explicit doesn't hurt for clarity).

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UpstreamCache::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum UpstreamCache {
    Table,
    Id,
    PackageName,
    UpstreamUrl,
    PackumentJson,
    FetchedAt,
    CreatedAt,
}
