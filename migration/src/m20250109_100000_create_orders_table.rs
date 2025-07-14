use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Orders::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Orders::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Orders::UserId).integer().not_null())
                    .col(ColumnDef::new(Orders::EventId).integer().not_null())
                    .col(ColumnDef::new(Orders::OptionId).integer().not_null())
                    .col(ColumnDef::new(Orders::Side).string().not_null())
                    .col(ColumnDef::new(Orders::OrderType).string().not_null())
                    .col(ColumnDef::new(Orders::TimeInForce).string().not_null().default("GTC"))
                    .col(ColumnDef::new(Orders::Price).decimal_len(20, 8).not_null())
                    .col(ColumnDef::new(Orders::Quantity).integer().not_null())
                    .col(ColumnDef::new(Orders::FilledQuantity).integer().not_null().default(0))
                    .col(ColumnDef::new(Orders::Status).string().not_null())
                    .col(ColumnDef::new(Orders::CreatedAt).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(Orders::UpdatedAt).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_orders_user")
                            .from(Orders::Table, Orders::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_orders_event")
                            .from(Orders::Table, Orders::EventId)
                            .to(Events::Table, Events::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_orders_option")
                            .from(Orders::Table, Orders::OptionId)
                            .to(EventOptions::Table, EventOptions::Id)
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
                    .name("idx_orders_user_id")
                    .table(Orders::Table)
                    .col(Orders::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_event_option")
                    .table(Orders::Table)
                    .col(Orders::EventId)
                    .col(Orders::OptionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_orders_status")
                    .table(Orders::Table)
                    .col(Orders::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Orders::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Orders {
    Table,
    Id,
    UserId,
    EventId,
    OptionId,
    Side,
    OrderType,
    TimeInForce,
    Price,
    Quantity,
    FilledQuantity,
    Status,
    CreatedAt,
    UpdatedAt,
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