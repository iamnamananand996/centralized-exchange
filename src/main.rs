use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use deadpool_redis::{Config, Pool, Runtime};
use dotenv::dotenv;
use migration::sea_orm::{Database, DatabaseConnection};
use std::time::Duration;

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

    // Redis connection pool
    let redis_url = constants::config::get_redis_url();
    let redis_config = Config::from_url(&redis_url);
    let redis_pool: Pool = redis_config
        .create_pool(Some(Runtime::Tokio1))
        .expect("Failed to create Redis pool");

    let server_address = constants::config::get_server_address();
    println!("🚀 Starting Centralized Exchange API server...");
    println!("📊 Database connected successfully");
    println!("🗄️  Redis cache configured at: {}", redis_url);
    println!("🌐 Server will be available at http://{}", server_address);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .app_data(web::Data::new(redis_pool.clone()))
            .wrap(
                Cors::default()
                    .allowed_origin(&constants::config::get_cors_origin())
                    .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
                    .allowed_headers(vec!["Content-Type", "Authorization"])
                    .max_age(3600),
            )
            .service(routes::api::configure_routes())
    })
    .bind(&server_address)?
    .run()
    .await
}
