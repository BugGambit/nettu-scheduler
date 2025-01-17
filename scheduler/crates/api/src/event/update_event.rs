use crate::{
    error::NettuError,
    event,
    shared::auth::protect_route,
    shared::{
        auth::{account_can_modify_event, protect_account_route, Permission},
        usecase::{
            execute, execute_with_policy, PermissionBoundary, Subscriber, UseCase,
            UseCaseErrorContainer,
        },
    },
};
use actix_web::{web, HttpRequest, HttpResponse};
use event::subscribers::SyncRemindersOnEventUpdated;
use nettu_scheduler_api_structs::update_event::*;
use nettu_scheduler_domain::{CalendarEvent, CalendarEventReminder, Metadata, RRuleOptions, ID};
use nettu_scheduler_infra::NettuContext;

fn handle_error(e: UseCaseErrors) -> NettuError {
    match e {
        UseCaseErrors::NotFound(entity, event_id) => NettuError::NotFound(format!(
            "The {} with id: {}, was not found.",
            entity, event_id
        )),
        UseCaseErrors::InvalidRecurrenceRule => {
            NettuError::BadClientData("Invalid recurrence rule specified for the event".into())
        }
        UseCaseErrors::InvalidReminder => {
            NettuError::BadClientData("Invalid reminder specified for the event".into())
        }
        UseCaseErrors::StorageError => NettuError::InternalError,
    }
}

pub async fn update_event_admin_controller(
    http_req: HttpRequest,
    body: web::Json<RequestBody>,
    path_params: web::Path<PathParams>,
    ctx: web::Data<NettuContext>,
) -> Result<HttpResponse, NettuError> {
    let account = protect_account_route(&http_req, &ctx).await?;
    let e = account_can_modify_event(&account, &path_params.event_id, &ctx).await?;

    let body = body.0;
    let usecase = UpdateEventUseCase {
        user_id: e.user_id,
        event_id: e.id,
        duration: body.duration,
        start_ts: body.start_ts,
        reminder: body.reminder,
        recurrence: body.recurrence,
        busy: body.busy,
        is_service: body.is_service,
        exdates: body.exdates,
        metadata: body.metadata,
    };

    execute(usecase, &ctx)
        .await
        .map(|event| HttpResponse::Ok().json(APIResponse::new(event)))
        .map_err(handle_error)
}

pub async fn update_event_controller(
    http_req: HttpRequest,
    body: web::Json<RequestBody>,
    path_params: web::Path<PathParams>,
    ctx: web::Data<NettuContext>,
) -> Result<HttpResponse, NettuError> {
    let (user, policy) = protect_route(&http_req, &ctx).await?;

    let body = body.0;
    let usecase = UpdateEventUseCase {
        user_id: user.id.clone(),
        event_id: path_params.event_id.clone(),
        duration: body.duration,
        start_ts: body.start_ts,
        reminder: body.reminder,
        recurrence: body.recurrence,
        busy: body.busy,
        is_service: body.is_service,
        exdates: body.exdates,
        metadata: body.metadata,
    };

    execute_with_policy(usecase, &policy, &ctx)
        .await
        .map(|event| HttpResponse::Ok().json(APIResponse::new(event)))
        .map_err(|e| match e {
            UseCaseErrorContainer::Unauthorized(e) => NettuError::Unauthorized(e),
            UseCaseErrorContainer::UseCase(e) => handle_error(e),
        })
}

#[derive(Debug)]
pub struct UpdateEventUseCase {
    pub user_id: ID,
    pub event_id: ID,
    pub start_ts: Option<i64>,
    pub busy: Option<bool>,
    pub duration: Option<i64>,
    pub reminder: Option<CalendarEventReminder>,
    pub recurrence: Option<RRuleOptions>,
    pub is_service: Option<bool>,
    pub exdates: Option<Vec<i64>>,
    pub metadata: Option<Metadata>,
}

#[derive(Debug)]
pub enum UseCaseErrors {
    NotFound(String, ID),
    InvalidReminder,
    StorageError,
    InvalidRecurrenceRule,
}

#[async_trait::async_trait(?Send)]
impl UseCase for UpdateEventUseCase {
    type Response = CalendarEvent;

    type Errors = UseCaseErrors;

    const NAME: &'static str = "UpdateEvent";

    async fn execute(&mut self, ctx: &NettuContext) -> Result<Self::Response, Self::Errors> {
        let UpdateEventUseCase {
            user_id,
            event_id,
            start_ts,
            busy,
            duration,
            recurrence,
            exdates,
            reminder,
            is_service,
            metadata,
        } = self;

        let mut e = match ctx.repos.event_repo.find(&event_id).await {
            Some(event) if event.user_id == *user_id => event,
            _ => {
                return Err(UseCaseErrors::NotFound(
                    "Calendar Event".into(),
                    event_id.clone(),
                ))
            }
        };

        if let Some(is_service) = is_service {
            e.is_service = *is_service;
        }

        if let Some(exdates) = exdates {
            e.exdates = exdates.clone();
        }
        if let Some(metadata) = metadata {
            e.metadata = metadata.clone();
        }

        if let Some(reminder) = &e.reminder {
            if !reminder.is_valid() {
                return Err(UseCaseErrors::InvalidReminder);
            }
        }
        e.reminder = reminder.clone();

        let calendar = match ctx.repos.calendar_repo.find(&e.calendar_id).await {
            Some(cal) => cal,
            _ => {
                return Err(UseCaseErrors::NotFound(
                    "Calendar".into(),
                    e.calendar_id.clone(),
                ))
            }
        };

        let mut start_or_duration_change = false;

        if let Some(start_ts) = start_ts {
            if e.start_ts != *start_ts {
                e.start_ts = *start_ts;
                e.exdates = vec![];
                start_or_duration_change = true;
            }
        }
        if let Some(duration) = duration {
            if e.duration != *duration {
                e.duration = *duration;
                start_or_duration_change = true;
            }
        }
        if let Some(busy) = busy {
            e.busy = *busy;
        }

        let valid_recurrence = if let Some(rrule_opts) = recurrence.clone() {
            // ? should exdates be deleted when rrules are updated
            e.set_recurrence(rrule_opts, &calendar.settings, true)
        } else if start_or_duration_change && e.recurrence.is_some() {
            e.set_recurrence(e.recurrence.clone().unwrap(), &calendar.settings, true)
        } else {
            true
        };

        if !valid_recurrence {
            return Err(UseCaseErrors::InvalidRecurrenceRule);
        };

        e.updated = ctx.sys.get_timestamp_millis();

        let repo_res = ctx.repos.event_repo.save(&e).await;
        if repo_res.is_err() {
            return Err(UseCaseErrors::StorageError);
        }

        Ok(e)
    }

    fn subscribers() -> Vec<Box<dyn Subscriber<Self>>> {
        vec![Box::new(SyncRemindersOnEventUpdated)]
    }
}

impl PermissionBoundary for UpdateEventUseCase {
    fn permissions(&self) -> Vec<Permission> {
        vec![Permission::UpdateCalendarEvent]
    }
}

#[cfg(test)]
mod test {
    use nettu_scheduler_infra::setup_context;

    use super::*;

    #[actix_web::main]
    #[test]
    async fn update_notexisting_event() {
        let mut usecase = UpdateEventUseCase {
            event_id: Default::default(),
            start_ts: Some(500),
            duration: Some(800),
            reminder: None,
            recurrence: None,
            busy: Some(false),
            user_id: Default::default(),
            is_service: None,
            exdates: None,
            metadata: None,
        };
        let ctx = setup_context().await;
        let res = usecase.execute(&ctx).await;
        assert!(res.is_err());
    }
}
