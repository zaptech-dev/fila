//! Migration: create batchs

use rapina::migration::prelude::*;
use rapina::sea_orm_migration;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Batchs::Table)
                    .col(
                        ColumnDef::new(Batchs::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Batchs::Status).string().not_null())
                    .col(ColumnDef::new(Batchs::CompletedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Batchs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Batchs {
    Table,
    Id,
    Status,
    CompletedAt,
}
