use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserPositions::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserPositions::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(UserPositions::UserId).integer().not_null())
                    .col(ColumnDef::new(UserPositions::EventId).integer().not_null())
                    .col(ColumnDef::new(UserPositions::OptionId).integer().not_null())
                    .col(ColumnDef::new(UserPositions::Quantity).integer().not_null().default(0))
                    .col(ColumnDef::new(UserPositions::AveragePrice).decimal_len(20, 8).not_null().default(0.0))
                    .col(ColumnDef::new(UserPositions::CreatedAt).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(UserPositions::UpdatedAt).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_positions_user")
                            .from(UserPositions::Table, UserPositions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_positions_event")
                            .from(UserPositions::Table, UserPositions::EventId)
                            .to(Events::Table, Events::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_positions_option")
                            .from(UserPositions::Table, UserPositions::OptionId)
                            .to(EventOptions::Table, EventOptions::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique constraint
        manager
            .create_index(
                Index::create()
                    .name("idx_user_positions_unique")
                    .table(UserPositions::Table)
                    .col(UserPositions::UserId)
                    .col(UserPositions::EventId)
                    .col(UserPositions::OptionId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create additional indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_user_positions_user")
                    .table(UserPositions::Table)
                    .col(UserPositions::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_positions_event_option")
                    .table(UserPositions::Table)
                    .col(UserPositions::EventId)
                    .col(UserPositions::OptionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserPositions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserPositions {
    Table,
    Id,
    UserId,
    EventId,
    OptionId,
    Quantity,
    AveragePrice,
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