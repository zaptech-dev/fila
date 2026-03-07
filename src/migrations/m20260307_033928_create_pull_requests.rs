//! Migration: create pull_requests

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
                    .table(PullRequests::Table)
                    .col(
                        ColumnDef::new(PullRequests::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PullRequests::RepoOwner).string().not_null())
                    .col(ColumnDef::new(PullRequests::RepoName).string().not_null())
                    .col(ColumnDef::new(PullRequests::PrNumber).integer().not_null())
                    .col(ColumnDef::new(PullRequests::Title).string().not_null())
                    .col(ColumnDef::new(PullRequests::Author).string().not_null())
                    .col(ColumnDef::new(PullRequests::HeadSha).string().not_null())
                    .col(ColumnDef::new(PullRequests::Status).string().not_null())
                    .col(ColumnDef::new(PullRequests::Priority).integer().not_null())
                    .col(
                        ColumnDef::new(PullRequests::QueuedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(PullRequests::MergedAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PullRequests::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PullRequests {
    Table,
    Id,
    RepoOwner,
    RepoName,
    PrNumber,
    Title,
    Author,
    HeadSha,
    Status,
    Priority,
    QueuedAt,
    MergedAt,
}
