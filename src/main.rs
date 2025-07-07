use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use migration::sea_orm::{Database, DatabaseConnection};

use migration::MigratorTrait;

mod constants;
mod handlers;
mod middleware;
mod routes;
mod types;
mod utils;

// Import the migration module
use migration::Migrator;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    // Database connection
    let database_url = constants::config::get_database_url()
        .expect("DATABASE_URL environment variable is required");

    let db: DatabaseConnection = Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Run database migrations
    println!("🔄 Running database migrations...");
    Migrator::up(&db, None)
        .await
        .expect("Failed to run database migrations");
    println!("✅ Database migrations completed successfully");

    let server_address = constants::config::get_server_address();
    println!("🚀 Starting Centralized Exchange API server...");
    println!("📊 Database connected successfully");
    println!("🌐 Server will be available at http://{}", server_address);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .service(routes::api::configure_routes())
    })
    .bind(&server_address)?
    .run()
    .await
}
