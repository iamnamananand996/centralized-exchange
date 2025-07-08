use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EventOptions::Table)
                    .if_not_exists()
                    .col(pk_auto(EventOptions::Id))
                    .col(integer(EventOptions::EventId).not_null())
                    .col(string(EventOptions::OptionText).not_null())
                    .col(decimal_len(EventOptions::CurrentPrice, 4, 2).default(50.00))
                    .col(decimal_len(EventOptions::TotalBacking, 12, 2).default(0.00))
                    .col(
                        ColumnDef::new(EventOptions::IsWinningOption)
                            .boolean()
                            .null()
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_event_options_event_id")
                            .from(EventOptions::Table, EventOptions::EventId)
                            .to(Events::Table, Events::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EventOptions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum EventOptions {
    Table,
    Id,
    EventId,
    OptionText,
    CurrentPrice,
    TotalBacking,
    IsWinningOption,
}

#[derive(DeriveIden)]
enum Events {
    Table,
    Id,
}
