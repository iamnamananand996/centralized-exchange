use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Events::Table)
                    .if_not_exists()
                    .col(pk_auto(Events::Id))
                    .col(string_len(Events::Title, 255).not_null())
                    .col(text(Events::Description))
                    .col(string_len(Events::Category, 100).default("general"))
                    .col(string_len(Events::Status, 20).default("draft"))
                    .col(timestamp(Events::EndTime).not_null())
                    .col(decimal_len(Events::MinBetAmount, 8, 2).default(10.00))
                    .col(decimal_len(Events::MaxBetAmount, 10, 2).default(1000.00))
                    .col(decimal_len(Events::TotalVolume, 12, 2).default(0.00))
                    .col(string_len(Events::ImageUrl, 500))
                    .col(integer(Events::CreatedBy).not_null())
                    .col(integer(Events::ResolvedBy))
                    .col(integer(Events::WinningOptionId))
                    .col(text(Events::ResolutionNote))
                    .col(timestamp(Events::ResolvedAt))
                    .col(timestamp(Events::CreatedAt).default(Expr::current_timestamp()))
                    .col(timestamp(Events::UpdatedAt).default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_events_created_by")
                            .from(Events::Table, Events::CreatedBy)
                            .to(Users::Table, Users::Id)
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_events_resolved_by")
                            .from(Events::Table, Events::ResolvedBy)
                            .to(Users::Table, Users::Id)
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Events::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Events {
    Table,
    Id,
    Title,
    Description,
    Category,
    Status,
    EndTime,
    MinBetAmount,
    MaxBetAmount,
    TotalVolume,
    ImageUrl,
    CreatedBy,
    ResolvedBy,
    WinningOptionId,
    ResolutionNote,
    ResolvedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
