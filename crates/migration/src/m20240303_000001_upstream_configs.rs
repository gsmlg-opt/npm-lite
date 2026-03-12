use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240303_000001_upstream_configs"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UpstreamConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UpstreamConfigs::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .default(Expr::cust("gen_random_uuid()")),
                    )
                    .col(ColumnDef::new(UpstreamConfigs::RuleType).text().not_null())
                    .col(
                        ColumnDef::new(UpstreamConfigs::MatchValue)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UpstreamConfigs::UpstreamUrl)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(UpstreamConfigs::AuthTokenRef).text().null())
                    .col(
                        ColumnDef::new(UpstreamConfigs::Priority)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(UpstreamConfigs::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(UpstreamConfigs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(UpstreamConfigs::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on rule_type for filtered queries.
        manager
            .create_index(
                Index::create()
                    .name("idx_upstream_configs_rule_type")
                    .table(UpstreamConfigs::Table)
                    .col(UpstreamConfigs::RuleType)
                    .to_owned(),
            )
            .await?;

        // Index on priority for ordered queries.
        manager
            .create_index(
                Index::create()
                    .name("idx_upstream_configs_priority")
                    .table(UpstreamConfigs::Table)
                    .col(UpstreamConfigs::Priority)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UpstreamConfigs::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum UpstreamConfigs {
    Table,
    Id,
    RuleType,
    MatchValue,
    UpstreamUrl,
    AuthTokenRef,
    Priority,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
