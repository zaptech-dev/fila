use rapina::migration::prelude::*;
use rapina::sea_orm_migration;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Index for PR lookups by repo + number + status (find_queued_pr, find_active_pr)
        manager
            .create_index(
                Index::create()
                    .name("idx_pr_repo_number_status")
                    .table(PullRequests::Table)
                    .col(PullRequests::RepoOwner)
                    .col(PullRequests::RepoName)
                    .col(PullRequests::PrNumber)
                    .col(PullRequests::Status)
                    .to_owned(),
            )
            .await?;

        // Index for queue ordering (get_queue, get_next_queued, dashboard active PRs)
        manager
            .create_index(
                Index::create()
                    .name("idx_pr_status_priority_queued")
                    .table(PullRequests::Table)
                    .col(PullRequests::Status)
                    .col(PullRequests::Priority)
                    .col(PullRequests::QueuedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_pr_repo_number_status")
                    .table(PullRequests::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_pr_status_priority_queued")
                    .table(PullRequests::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum PullRequests {
    Table,
    RepoOwner,
    RepoName,
    PrNumber,
    Status,
    Priority,
    QueuedAt,
}
