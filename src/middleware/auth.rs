use crate::utils::jwt::verify_jwt_token;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorUnauthorized,
    http::header,
    web, Error, HttpMessage,
};
use entity::users;
use futures_util::future::{ready, LocalBoxFuture, Ready};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub id: String,
    pub role: String,
}

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let auth_header = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "));

            match auth_header {
                Some(token) => {
                    match verify_jwt_token(token) {
                        Ok(user_id) => {
                            // Get database connection from request data
                            let db = req.app_data::<web::Data<DatabaseConnection>>();

                            if let Some(db) = db {
                                // Fetch user from database to get role
                                let user_id_int: i32 = user_id.parse().unwrap_or(0);
                                let user = users::Entity::find_by_id(user_id_int)
                                    .one(db.get_ref())
                                    .await
                                    .map_err(|_| ErrorUnauthorized("Database error"))?;

                                match user {
                                    Some(user) => {
                                        if !user.is_active {
                                            return Err(ErrorUnauthorized(
                                                "User account is deactivated",
                                            ));
                                        }

                                        let auth_user = AuthenticatedUser {
                                            id: user_id.clone(),
                                            role: user.role,
                                        };

                                        // Insert both user_id (for backward compatibility) and auth_user
                                        req.extensions_mut().insert(user_id);
                                        req.extensions_mut().insert(auth_user);

                                        let res = svc.call(req).await?;
                                        Ok(res)
                                    }
                                    None => Err(ErrorUnauthorized("User not found")),
                                }
                            } else {
                                // Fallback to just user_id if database is not available
                                req.extensions_mut().insert(user_id);
                                let res = svc.call(req).await?;
                                Ok(res)
                            }
                        }
                        Err(_) => Err(ErrorUnauthorized("Invalid token")),
                    }
                }
                None => Err(ErrorUnauthorized("Missing authorization header")),
            }
        })
    }
}
