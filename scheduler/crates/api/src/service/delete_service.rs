use crate::{
    error::NettuError,
    shared::{
        auth::protect_account_route,
        usecase::{execute, UseCase},
    },
};
use actix_web::{web, HttpRequest, HttpResponse};
use nettu_scheduler_api_structs::delete_service::*;
use nettu_scheduler_domain::{Account, Service, ID};
use nettu_scheduler_infra::NettuContext;

pub async fn delete_service_controller(
    http_req: HttpRequest,
    path_params: web::Path<PathParams>,
    ctx: web::Data<NettuContext>,
) -> Result<HttpResponse, NettuError> {
    let account = protect_account_route(&http_req, &ctx).await?;

    let usecase = DeleteServiceUseCase {
        account,
        service_id: path_params.service_id.clone(),
    };

    execute(usecase, &ctx)
        .await
        .map(|usecase_res| HttpResponse::Ok().json(APIResponse::new(usecase_res.service)))
        .map_err(|e| match e {
            UseCaseErrors::NotFound => NettuError::NotFound(format!(
                "The service with id: {} was not found.",
                path_params.service_id
            )),
            UseCaseErrors::StorageError => NettuError::InternalError,
        })
}

#[derive(Debug)]
struct DeleteServiceUseCase {
    account: Account,
    service_id: ID,
}

#[derive(Debug)]
struct UseCaseRes {
    pub service: Service,
}

#[derive(Debug)]
enum UseCaseErrors {
    NotFound,
    StorageError,
}

#[async_trait::async_trait(?Send)]
impl UseCase for DeleteServiceUseCase {
    type Response = UseCaseRes;

    type Errors = UseCaseErrors;

    const NAME: &'static str = "DeleteService";

    async fn execute(&mut self, ctx: &NettuContext) -> Result<Self::Response, Self::Errors> {
        let res = ctx.repos.service_repo.find(&self.service_id).await;
        match res {
            Some(service) if service.account_id == self.account.id => {
                if ctx
                    .repos
                    .service_repo
                    .delete(&self.service_id)
                    .await
                    .is_none()
                {
                    return Err(UseCaseErrors::StorageError);
                }
                Ok(UseCaseRes { service })
            }
            _ => Err(UseCaseErrors::NotFound),
        }
    }
}
