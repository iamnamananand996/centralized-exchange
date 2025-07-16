use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Trades::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Trades::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Trades::EventId).integer().not_null())
                    .col(ColumnDef::new(Trades::OptionId).integer().not_null())
                    .col(ColumnDef::new(Trades::BuyerId).integer().not_null())
                    .col(ColumnDef::new(Trades::SellerId).integer().not_null())
                    .col(ColumnDef::new(Trades::BuyOrderId).string().not_null())
                    .col(ColumnDef::new(Trades::SellOrderId).string().not_null())
                    .col(ColumnDef::new(Trades::Price).decimal_len(20, 8).not_null())
                    .col(ColumnDef::new(Trades::Quantity).integer().not_null())
                    .col(
                        ColumnDef::new(Trades::TotalAmount)
                            .decimal_len(20, 8)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Trades::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_event")
                            .from(Trades::Table, Trades::EventId)
                            .to(Events::Table, Events::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_option")
                            .from(Trades::Table, Trades::OptionId)
                            .to(EventOptions::Table, EventOptions::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_buyer")
                            .from(Trades::Table, Trades::BuyerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_seller")
                            .from(Trades::Table, Trades::SellerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_buy_order")
                            .from(Trades::Table, Trades::BuyOrderId)
                            .to(Orders::Table, Orders::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trades_sell_order")
                            .from(Trades::Table, Trades::SellOrderId)
                            .to(Orders::Table, Orders::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_trades_event_option")
                    .table(Trades::Table)
                    .col(Trades::EventId)
                    .col(Trades::OptionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_trades_buyer")
                    .table(Trades::Table)
                    .col(Trades::BuyerId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_trades_seller")
                    .table(Trades::Table)
                    .col(Trades::SellerId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_trades_timestamp")
                    .table(Trades::Table)
                    .col(Trades::Timestamp)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Trades::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Trades {
    Table,
    Id,
    EventId,
    OptionId,
    BuyerId,
    SellerId,
    BuyOrderId,
    SellOrderId,
    Price,
    Quantity,
    TotalAmount,
    Timestamp,
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

#[derive(DeriveIden)]
enum Orders {
    Table,
    Id,
}
