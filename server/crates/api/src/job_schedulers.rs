use crate::{
    event::usecases::{
        get_upcoming_reminders::GetUpcomingRemindersUseCase,
        sync_event_reminders::{SyncEventRemindersTrigger, SyncEventRemindersUseCase},
    },
    shared::usecase::execute,
};
use actix_web::client::Client;
use actix_web::rt::time::{delay_until, interval, Instant};
use nettu_scheduler_api_structs::dtos::CalendarEventDTO;
use nettu_scheduler_infra::NettuContext;
use serde::Serialize;
use std::time::Duration;

pub fn get_start_delay(now_ts: usize, secs_before_min: usize) -> usize {
    let secs_to_next_minute = 60 - (now_ts / 1000) % 60;
    if secs_to_next_minute > secs_before_min {
        secs_to_next_minute - secs_before_min
    } else {
        secs_to_next_minute + (60 - secs_before_min)
    }
}

pub async fn start_reminders_expansion_job_scheduler(ctx: NettuContext) {
    actix_web::rt::spawn(async move {
        let mut interval = interval(Duration::from_secs(30 * 60 * 1000));
        loop {
            interval.tick().await;

            let usecase = SyncEventRemindersUseCase {
                request: SyncEventRemindersTrigger::JobScheduler,
            };
            let _ = execute(usecase, &ctx).await;
        }
    });
}

#[derive(Serialize)]
struct AccountEventRemindersDTO {
    events: Vec<CalendarEventDTO>,
}

pub async fn start_send_reminders_job(ctx: NettuContext) {
    actix_web::rt::spawn(async move {
        let now = ctx.sys.get_timestamp_millis();
        let secs_to_next_run = get_start_delay(now as usize, 0);
        let start = Instant::now() + Duration::from_secs(secs_to_next_run as u64);

        delay_until(start).await;
        let mut minutely_interval = interval(Duration::from_secs(60));
        loop {
            minutely_interval.tick().await;
            let context = ctx.clone();
            actix_web::rt::spawn(async move {
                let client = Client::new();
                println!("Minute tick at: {:?}", context.sys.get_timestamp_millis());

                let usecase = GetUpcomingRemindersUseCase {};
                let account_reminders = match execute(usecase, &context).await {
                    Ok(res) => res,
                    Err(_) => return,
                };

                let send_instant = account_reminders.1;
                delay_until(send_instant).await;
                println!(
                    "Finished the delay at: {:?}",
                    context.sys.get_timestamp_millis()
                );
                println!("Reminders to send: {:?}", account_reminders);

                for (acc, reminders) in account_reminders.0 {
                    match acc.settings.webhook {
                        None => continue,
                        Some(webhook) => {
                            if let Err(e) = client
                                .post(webhook.url)
                                .header("nettu-scheduler-webhook-key", webhook.key)
                                .send_json(&AccountEventRemindersDTO {
                                    events: reminders
                                        .events
                                        .iter()
                                        .map(|e| CalendarEventDTO::new(e))
                                        .collect(),
                                })
                                .await
                            {
                                println!("Error informing client of reminders: {:?}", e);
                            }
                        }
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_delay_works() {
        assert_eq!(get_start_delay(50 * 1000, 5), 5);
        assert_eq!(get_start_delay(50 * 1000, 10), 60);
        assert_eq!(get_start_delay(50 * 1000, 15), 55);
        assert_eq!(get_start_delay(60 * 1000, 60), 60);
        assert_eq!(get_start_delay(60 * 1000, 10), 50);
        assert_eq!(get_start_delay(59 * 1000, 0), 1);
        assert_eq!(get_start_delay(59 * 1000, 1), 60);
    }
}
