pub use sea_orm_migration::prelude::*;

mod m20240101_000000_create_users_table;
mod m20250109_100000_create_orders_table;
mod m20250109_110000_create_trades_table;
mod m20250109_120000_create_user_positions_table;
mod m20250707_223909_transactions;
mod m20250708_130208_events;
mod m20250708_150357_event_options;
mod m20250709_000000_add_user_role;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240101_000000_create_users_table::Migration),
            Box::new(m20250707_223909_transactions::Migration),
            Box::new(m20250708_130208_events::Migration),
            Box::new(m20250708_150357_event_options::Migration),
            Box::new(m20250109_100000_create_orders_table::Migration),
            Box::new(m20250109_110000_create_trades_table::Migration),
            Box::new(m20250109_120000_create_user_positions_table::Migration),
            Box::new(m20250709_000000_add_user_role::Migration),
        ]
    }
}
