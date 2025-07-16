use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Transaction::Table)
                    .if_not_exists()
                    .col(pk_auto(Transaction::Id))
                    .col(integer(Transaction::UserId).not_null())
                    .col(string_len(Transaction::Type, 20).not_null())
                    .col(decimal_len(Transaction::Amount, 10, 2).not_null())
                    .col(decimal_len(Transaction::BalanceBefore, 10, 2).not_null())
                    .col(decimal_len(Transaction::BalanceAfter, 10, 2).not_null())
                    .col(string_len(Transaction::Status, 20).default("pending"))
                    .col(string_len(Transaction::ReferenceId, 100))
                    .col(timestamp(Transaction::CreatedAt).default(Expr::current_timestamp()))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_transactions_user_id")
                            .from(Transaction::Table, Transaction::UserId)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Transaction::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Transaction {
    Table,
    Id,
    UserId,
    Type,
    Amount,
    BalanceBefore,
    BalanceAfter,
    Status,
    ReferenceId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
