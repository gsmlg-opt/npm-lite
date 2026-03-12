use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240404_000001_add_version_source"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add `source` column: 'local' or 'upstream', default 'local'.
        manager
            .alter_table(
                Table::alter()
                    .table(PackageVersions::Table)
                    .add_column(
                        ColumnDef::new(PackageVersions::Source)
                            .text()
                            .not_null()
                            .default("local"),
                    )
                    .to_owned(),
            )
            .await?;

        // Add `upstream_url` column: nullable, origin upstream URL for cached versions.
        manager
            .alter_table(
                Table::alter()
                    .table(PackageVersions::Table)
                    .add_column(
                        ColumnDef::new(PackageVersions::UpstreamUrl)
                            .text()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PackageVersions::Table)
                    .drop_column(PackageVersions::UpstreamUrl)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(PackageVersions::Table)
                    .drop_column(PackageVersions::Source)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum PackageVersions {
    Table,
    Source,
    UpstreamUrl,
}
