use crate::{
    error::NettuError,
    shared::{
        auth::{protect_route, Permission},
        usecase::{execute_with_policy, PermissionBoundary, UseCaseErrorContainer},
    },
};
use crate::{schedule::dtos::ScheduleDTO, shared::usecase::UseCase};
use actix_web::{web, HttpResponse};
use chrono_tz::Tz;
use nettu_scheduler_core::domain::{Schedule, ScheduleRule};
use nettu_scheduler_infra::Context;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UpdateScheduleSettigsPathParams {
    schedule_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScheduleBody {
    timezone: Option<String>,
    rules: Option<Vec<ScheduleRule>>,
}

pub async fn update_schedule_controller(
    http_req: web::HttpRequest,
    ctx: web::Data<Context>,
    path_params: web::Path<UpdateScheduleSettigsPathParams>,
    body_params: web::Json<UpdateScheduleBody>,
) -> Result<HttpResponse, NettuError> {
    let (user, policy) = protect_route(&http_req, &ctx).await?;

    let usecase = UpdateScheduleUseCase {
        user_id: user.id,
        schedule_id: path_params.schedule_id.clone(),
        timezone: body_params.timezone.to_owned(),
        rules: body_params.rules.to_owned(),
    };

    execute_with_policy(usecase, &policy, &ctx)
        .await
        .map(|res| {
            let dto = ScheduleDTO::new(&res.schedule);
            HttpResponse::Ok().json(dto)
        })
        .map_err(|e| match e {
            UseCaseErrorContainer::Unauthorized(e) => NettuError::Unauthorized(e),
            UseCaseErrorContainer::UseCase(e) => match e {
                UseCaseErrors::StorageError => NettuError::InternalError,
                UseCaseErrors::ScheduleNotFoundError => {
                    NettuError::NotFound("The schedule was not found.".into())
                }
                UseCaseErrors::InvalidSettings(err) => NettuError::BadClientData(format!(
                    "Bad schedule settings provided. Error message: {}",
                    err
                )),
            },
        })
}

struct UpdateScheduleUseCase {
    pub user_id: String,
    pub schedule_id: String,
    pub timezone: Option<String>,
    pub rules: Option<Vec<ScheduleRule>>,
}

#[derive(Debug)]
enum UseCaseErrors {
    ScheduleNotFoundError,
    StorageError,
    InvalidSettings(String),
}
struct UseCaseRes {
    pub schedule: Schedule,
}

#[async_trait::async_trait(?Send)]
impl UseCase for UpdateScheduleUseCase {
    type Response = UseCaseRes;

    type Errors = UseCaseErrors;

    type Context = Context;

    async fn execute(&mut self, ctx: &Self::Context) -> Result<Self::Response, Self::Errors> {
        let mut schedule = match ctx.repos.schedule_repo.find(&self.schedule_id).await {
            Some(cal) if cal.user_id == self.user_id => cal,
            _ => return Err(UseCaseErrors::ScheduleNotFoundError),
        };

        if let Some(tz) = &self.timezone {
            match tz.parse::<Tz>() {
                Ok(tz) => schedule.timezone = tz,
                Err(_) => {
                    return Err(UseCaseErrors::InvalidSettings(format!(
                        "Invalid timezone provided: {}",
                        tz
                    )))
                }
            }
        };
        if let Some(rules) = &self.rules {
            schedule.set_rules(rules);
        }

        let repo_res = ctx.repos.schedule_repo.save(&schedule).await;
        match repo_res {
            Ok(_) => Ok(UseCaseRes { schedule }),
            Err(_) => Err(UseCaseErrors::StorageError),
        }
    }
}

impl PermissionBoundary for UpdateScheduleUseCase {
    fn permissions(&self) -> Vec<Permission> {
        vec![Permission::UpdateSchedule]
    }
}
