pub use sea_orm_migration::prelude::*;

mod m20240101_000001_create_tables;
mod m20240202_000001_upstream_cache;
mod m20240303_000001_upstream_configs;
mod m20240404_000001_add_version_source;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240101_000001_create_tables::Migration),
            Box::new(m20240202_000001_upstream_cache::Migration),
            Box::new(m20240303_000001_upstream_configs::Migration),
            Box::new(m20240404_000001_add_version_source::Migration),
        ]
    }
}
