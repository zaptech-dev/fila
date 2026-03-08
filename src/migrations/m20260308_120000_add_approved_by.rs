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
                    .add_column(ColumnDef::new(PullRequests::ApprovedBy).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PullRequests::Table)
                    .drop_column(PullRequests::ApprovedBy)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PullRequests {
    Table,
    ApprovedBy,
}
