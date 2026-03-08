//! Migration: add installation_id to pull_requests

use rapina::migration::prelude::*;
use rapina::sea_orm_migration;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PullRequests::Table)
                    .add_column(
                        ColumnDef::new(PullRequests::InstallationId)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PullRequests::Table)
                    .drop_column(PullRequests::InstallationId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PullRequests {
    Table,
    InstallationId,
}
