use crate::shared::{
    auth::{account_can_modify_schedule, protect_account_route, protect_route, Permission},
    usecase::{execute, execute_with_policy, PermissionBoundary, UseCaseErrorContainer},
};
use crate::{error::NettuError, shared::usecase::UseCase};
use actix_web::{web, HttpResponse};
use nettu_scheduler_api_structs::delete_schedule::*;
use nettu_scheduler_domain::{Schedule, ID};
use nettu_scheduler_infra::NettuContext;

fn handle_error(e: UseCaseErrors) -> NettuError {
    match e {
        UseCaseErrors::StorageError => NettuError::InternalError,
        UseCaseErrors::NotFound(schedule_id) => NettuError::NotFound(format!(
            "The schedule with id: {}, was not found.",
            schedule_id
        )),
    }
}

pub async fn delete_schedule_admin_controller(
    http_req: web::HttpRequest,
    path: web::Path<PathParams>,
    ctx: web::Data<NettuContext>,
) -> Result<HttpResponse, NettuError> {
    let account = protect_account_route(&http_req, &ctx).await?;
    let schedule = account_can_modify_schedule(&account, &path.schedule_id, &ctx).await?;

    let usecase = DeleteScheduleUseCase {
        user_id: schedule.user_id,
        schedule_id: schedule.id,
    };

    execute(usecase, &ctx)
        .await
        .map(|schedule| HttpResponse::Ok().json(APIResponse::new(schedule)))
        .map_err(handle_error)
}

pub async fn delete_schedule_controller(
    http_req: web::HttpRequest,
    path: web::Path<PathParams>,
    ctx: web::Data<NettuContext>,
) -> Result<HttpResponse, NettuError> {
    let (user, policy) = protect_route(&http_req, &ctx).await?;

    let usecase = DeleteScheduleUseCase {
        user_id: user.id,
        schedule_id: path.schedule_id.clone(),
    };

    execute_with_policy(usecase, &policy, &ctx)
        .await
        .map(|schedule| HttpResponse::Ok().json(APIResponse::new(schedule)))
        .map_err(|e| match e {
            UseCaseErrorContainer::Unauthorized(e) => NettuError::Unauthorized(e),
            UseCaseErrorContainer::UseCase(e) => handle_error(e),
        })
}

#[derive(Debug)]
pub enum UseCaseErrors {
    NotFound(ID),
    StorageError,
}

#[derive(Debug)]
pub struct DeleteScheduleUseCase {
    schedule_id: ID,
    user_id: ID,
}

#[async_trait::async_trait(?Send)]
impl UseCase for DeleteScheduleUseCase {
    type Response = Schedule;

    type Errors = UseCaseErrors;

    const NAME: &'static str = "DeleteSchedule";

    async fn execute(&mut self, ctx: &NettuContext) -> Result<Self::Response, Self::Errors> {
        let schedule = ctx.repos.schedule_repo.find(&self.schedule_id).await;
        match schedule {
            Some(schedule) if schedule.user_id == self.user_id => {
                let res = ctx.repos.schedule_repo.delete(&schedule.id).await;
                if res.is_none() {
                    return Err(UseCaseErrors::StorageError);
                }
                let res = ctx
                    .repos
                    .service_repo
                    .remove_schedule_from_services(&schedule.id)
                    .await;
                if res.is_err() {
                    return Err(UseCaseErrors::StorageError);
                }

                Ok(schedule)
            }
            _ => Err(UseCaseErrors::NotFound(self.schedule_id.clone())),
        }
    }
}

impl PermissionBoundary for DeleteScheduleUseCase {
    fn permissions(&self) -> Vec<Permission> {
        vec![Permission::DeleteSchedule]
    }
}
