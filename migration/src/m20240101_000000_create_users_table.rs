use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(pk_auto(Users::Id))
                    .col(string_len(Users::Username, 50).not_null().unique_key())
                    .col(string_len(Users::Email, 255).not_null().unique_key())
                    .col(string_len(Users::Phone, 20))
                    .col(string_len(Users::PasswordHash, 255).not_null())
                    .col(string_len(Users::FullName, 100))
                    .col(decimal_len(Users::WalletBalance, 10, 2).default(0.00))
                    .col(boolean(Users::IsActive).default(true))
                    .col(timestamp(Users::CreatedAt).default(Expr::current_timestamp()))
                    .col(timestamp(Users::UpdatedAt).default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Username,
    Email,
    Phone,
    PasswordHash,
    FullName,
    WalletBalance,
    IsActive,
    CreatedAt,
    UpdatedAt,
}
