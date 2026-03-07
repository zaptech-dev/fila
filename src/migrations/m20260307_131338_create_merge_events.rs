//! Migration: create merge_events

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
                    .table(MergeEvents::Table)
                    .col(
                        ColumnDef::new(MergeEvents::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(MergeEvents::PullRequestId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(MergeEvents::BatchId).integer().not_null())
                    .col(ColumnDef::new(MergeEvents::EventType).string().not_null())
                    .col(ColumnDef::new(MergeEvents::Details).string())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MergeEvents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MergeEvents {
    Table,
    Id,
    PullRequestId,
    BatchId,
    EventType,
    Details,
}
