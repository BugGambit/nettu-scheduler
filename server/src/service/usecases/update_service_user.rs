use crate::{
    account::domain::Account,
    calendar::repos::ICalendarRepo,
    service::{
        domain::{Service},
        repos::IServiceRepo,
    },
    shared::auth::{protect_account_route, AccountAuthContext},
    user::domain::User,
};
use crate::{api::Context, user::repos::IUserRepo};
use actix_web::{web, HttpRequest, HttpResponse};

use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct PathParams {
    service_id: String,
    user_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyParams {
    calendar_ids: Vec<String>,
}

pub async fn update_service_user_controller(
    http_req: HttpRequest,
    body: web::Json<BodyParams>,
    path_params: web::Path<PathParams>,
    ctx: web::Data<Context>,
) -> HttpResponse {
    let account = match protect_account_route(
        &http_req,
        &AccountAuthContext {
            account_repo: Arc::clone(&ctx.repos.account_repo),
        },
    )
    .await
    {
        Ok(a) => a,
        Err(res) => return res,
    };

    let user_id = User::create_id(&account.id, &path_params.user_id);
    let req = UsecaseReq {
        account,
        calendar_ids: body.calendar_ids.to_owned(),
        service_id: path_params.service_id.to_owned(),
        user_id,
    };

    let res = update_service_user_usecase(
        req,
        UsecaseCtx {
            service_repo: Arc::clone(&ctx.repos.service_repo),
            user_repo: Arc::clone(&ctx.repos.user_repo),
            calendar_repo: Arc::clone(&ctx.repos.calendar_repo),
        },
    )
    .await;

    match res {
        Ok(_) => HttpResponse::Ok().body("Service successfully updated"),
        Err(e) => match e {
            UsecaseErrors::StorageError => HttpResponse::InternalServerError().finish(),
            UsecaseErrors::ServiceNotFoundError => {
                HttpResponse::NotFound().body("The requested service was not found")
            }
            UsecaseErrors::UserNotFoundError => {
                HttpResponse::NotFound().body("The specified user was not found")
            }
            UsecaseErrors::CalendarNotOwnedByUser(calendar_id) => {
                HttpResponse::Forbidden().body(format!(
                    "The calendar: {}, was not found among the calendars for the specified user",
                    calendar_id
                ))
            }
        },
    }
}

struct UsecaseReq {
    pub account: Account,
    pub service_id: String,
    pub user_id: String,
    pub calendar_ids: Vec<String>,
}

struct UsecaseRes {
    pub service: Service,
}

enum UsecaseErrors {
    StorageError,
    ServiceNotFoundError,
    UserNotFoundError,
    CalendarNotOwnedByUser(String),
}

struct UsecaseCtx {
    pub service_repo: Arc<dyn IServiceRepo>,
    pub calendar_repo: Arc<dyn ICalendarRepo>,
    pub user_repo: Arc<dyn IUserRepo>,
}

async fn update_service_user_usecase(
    req: UsecaseReq,
    ctx: UsecaseCtx,
) -> Result<UsecaseRes, UsecaseErrors> {
    let mut service = match ctx.service_repo.find(&req.service_id).await {
        Some(service) if service.account_id == req.account.id => service,
        _ => return Err(UsecaseErrors::ServiceNotFoundError),
    };

    let user_resource = match service.find_user_mut(&req.user_id) {
        Some(res) => res,
        _ => return Err(UsecaseErrors::UserNotFoundError),
    };

    let user_calendars = ctx
        .calendar_repo
        .find_by_user(&req.user_id)
        .await
        .into_iter()
        .map(|cal| cal.id)
        .collect::<Vec<_>>();
    for calendar_id in &req.calendar_ids {
        if !user_calendars.contains(calendar_id) {
            return Err(UsecaseErrors::CalendarNotOwnedByUser(calendar_id.clone()));
        }
    }

    user_resource.set_calendar_ids(&req.calendar_ids);

    let res = ctx.service_repo.save(&service).await;
    match res {
        Ok(_) => Ok(UsecaseRes { service }),
        Err(_) => Err(UsecaseErrors::StorageError),
    }
}