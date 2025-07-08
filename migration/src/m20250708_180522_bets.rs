use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Bets::Table)
                    .if_not_exists()
                    .col(pk_auto(Bets::Id))
                    .col(integer(Bets::UserId).not_null())
                    .col(integer(Bets::EventId).not_null())
                    .col(integer(Bets::OptionId).not_null())
                    .col(integer(Bets::Quantity).not_null())
                    .col(decimal_len(Bets::PricePerShare, 4, 2).not_null())
                    .col(decimal_len(Bets::TotalAmount, 10, 2).not_null())
                    .col(string_len(Bets::Status, 20).default("active"))
                    .col(timestamp(Bets::PlacedAt).default(Expr::current_timestamp()))
                    .col(timestamp(Bets::SettledAt).null())
                    .col(decimal_len(Bets::PayoutAmount, 10, 2).default(0.00))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bets_user_id")
                            .from(Bets::Table, Bets::UserId)
                            .to(Users::Table, Users::Id)
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bets_event_id")
                            .from(Bets::Table, Bets::EventId)
                            .to(Events::Table, Events::Id)
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_bets_option_id")
                            .from(Bets::Table, Bets::OptionId)
                            .to(EventOptions::Table, EventOptions::Id)
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Bets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Bets {
    Table,
    Id,
    UserId,
    EventId,
    OptionId,
    Quantity,
    PricePerShare,
    TotalAmount,
    Status,
    PlacedAt,
    SettledAt,
    PayoutAmount,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Events {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum EventOptions {
    Table,
    Id,
}
