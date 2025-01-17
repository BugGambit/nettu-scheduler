use crate::{date, event_instance::EventInstance, CompatibleInstances, ID};
use chrono::prelude::*;

use chrono_tz::Tz;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BookingSlot {
    pub start: i64,
    pub duration: i64,
    pub available_until: i64,
}

fn is_cursor_in_events(
    cursor: i64,
    duration: i64,
    events: &CompatibleInstances,
) -> Option<&EventInstance> {
    for event in events.as_ref() {
        if event.start_ts <= cursor && event.end_ts >= cursor + duration {
            return Some(event);
        }
    }
    None
}

pub struct BookingSlotsOptions {
    pub start_ts: i64,
    pub end_ts: i64,
    pub duration: i64,
    pub interval: i64,
}

#[derive(Debug)]
pub struct UserFreeEvents {
    pub free_events: CompatibleInstances,
    pub user_id: ID,
}

#[derive(PartialEq, Debug)]
pub struct ServiceBookingSlot {
    pub start: i64,
    pub duration: i64,
    pub user_ids: Vec<ID>,
}

pub fn get_service_bookingslots(
    users_free: Vec<UserFreeEvents>,
    options: &BookingSlotsOptions,
) -> Vec<ServiceBookingSlot> {
    let mut slots_lookup: HashMap<i64, ServiceBookingSlot> = HashMap::new();

    for user in &users_free {
        let slots = get_booking_slots(&user.free_events, options);
        for slot in slots {
            if let Some(val) = slots_lookup.get(&slot.start) {
                let mut user_ids = val.user_ids.clone();
                user_ids.push(user.user_id.clone());
                slots_lookup.insert(
                    slot.start,
                    ServiceBookingSlot {
                        duration: slot.duration,
                        start: slot.start,
                        user_ids,
                    },
                );
            } else {
                slots_lookup.insert(
                    slot.start,
                    ServiceBookingSlot {
                        duration: slot.duration,
                        start: slot.start,
                        user_ids: vec![user.user_id.clone()],
                    },
                );
            }
        }
    }

    let mut slots = slots_lookup.drain().map(|s| s.1).collect::<Vec<_>>();
    slots.sort_by_key(|s| s.start);
    slots
}

pub fn get_booking_slots(
    free_events: &CompatibleInstances,
    options: &BookingSlotsOptions,
) -> Vec<BookingSlot> {
    let mut booking_slots = vec![];
    let &BookingSlotsOptions {
        start_ts,
        end_ts,
        duration,
        interval,
    } = options;

    if duration < 1 {
        return booking_slots;
    }

    let mut cursor = start_ts;
    while cursor + duration <= end_ts {
        let available_event = is_cursor_in_events(cursor, duration, &free_events);
        if let Some(event) = available_event {
            booking_slots.push(BookingSlot {
                start: cursor,
                duration,
                available_until: event.end_ts,
            });
        }

        cursor += interval;
    }

    booking_slots
}

pub fn validate_slots_interval(interval: i64) -> bool {
    let min_interval = 1000 * 60 * 10;
    let max_interval = 1000 * 60 * 60;
    interval >= min_interval && interval <= max_interval
}

pub struct BookingSlotsQuery {
    pub date: String,
    pub iana_tz: Option<String>,
    pub duration: i64,
    pub interval: i64,
}

pub enum BookingQueryError {
    InvalidInterval,
    InvalidDate(String),
    InvalidTimezone(String),
}

pub struct BookingTimespan {
    pub start_ts: i64,
    pub end_ts: i64,
}

pub fn validate_bookingslots_query(
    query: &BookingSlotsQuery,
) -> Result<BookingTimespan, BookingQueryError> {
    if !validate_slots_interval(query.interval) {
        return Err(BookingQueryError::InvalidInterval);
    }

    let iana_tz = query.iana_tz.clone().unwrap_or_else(|| "UTC".into());
    let tz: Tz = match iana_tz.parse() {
        Ok(tz) => tz,
        Err(_) => return Err(BookingQueryError::InvalidTimezone(iana_tz)),
    };

    let parsed_date = match date::is_valid_date(&query.date) {
        Ok(val) => val,
        Err(_) => return Err(BookingQueryError::InvalidDate(query.date.clone())),
    };

    let date = tz.ymd(parsed_date.0, parsed_date.1, parsed_date.2);
    let start_ts = date.and_hms(0, 0, 0).timestamp_millis();
    let end_ts = start_ts + 1000 * 60 * 60 * 24;

    Ok(BookingTimespan { start_ts, end_ts })
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn get_booking_slots_empty() {
        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );
        assert!(slots.is_empty());
    }

    #[test]
    fn get_booking_slots_from_one_event_1() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 12,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert!(slots.is_empty());
    }

    #[test]
    fn get_booking_slots_from_one_event_2() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 22,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 22,
                duration: 10,
                start: 10
            }
        );
    }

    #[test]
    fn get_booking_slots_from_one_event_3() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 42,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 3);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 42,
                duration: 10,
                start: 10
            }
        );
        assert_eq!(
            slots[1],
            BookingSlot {
                available_until: 42,
                duration: 10,
                start: 20
            }
        );
        assert_eq!(
            slots[2],
            BookingSlot {
                available_until: 42,
                duration: 10,
                start: 30
            }
        );
    }

    #[test]
    fn get_booking_slots_from_two_events() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 0,
            end_ts: 22,
        };

        let e2 = EventInstance {
            busy: false,
            start_ts: 30,
            end_ts: 50,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1, e2]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 4);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 22,
                duration: 10,
                start: 0
            }
        );
        assert_eq!(
            slots[1],
            BookingSlot {
                available_until: 22,
                duration: 10,
                start: 10
            }
        );
        assert_eq!(
            slots[2],
            BookingSlot {
                available_until: 50,
                duration: 10,
                start: 30
            }
        );
        assert_eq!(
            slots[3],
            BookingSlot {
                available_until: 50,
                duration: 10,
                start: 40
            }
        );
    }

    #[test]
    fn get_booking_slots_from_many_events() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 0,
            end_ts: 2,
        };

        let e2 = EventInstance {
            busy: false,
            start_ts: 33,
            end_ts: 50,
        };

        let e3 = EventInstance {
            busy: false,
            start_ts: 80,
            end_ts: 90,
        };

        let e4 = EventInstance {
            busy: false,
            start_ts: 90,
            end_ts: 100,
        };

        let e5 = EventInstance {
            busy: false,
            start_ts: 99,
            end_ts: 120,
        };

        let e6 = EventInstance {
            busy: false,
            start_ts: 140,
            end_ts: 160,
        };
        let availibility = CompatibleInstances::new(vec![e1, e3, e4, e2, e6, e5]);

        let slots = get_booking_slots(
            &availibility,
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 99,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 2);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 50,
                duration: 10,
                start: 40
            }
        );
        assert_eq!(
            slots[1],
            BookingSlot {
                available_until: 120,
                duration: 10,
                start: 80
            }
        );
    }

    #[test]
    fn slot_that_fits_right_at_end() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 81,
            end_ts: 100,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 100,
                duration: 10,
                start: 90
            }
        );
    }

    #[test]
    fn slot_that_crosses_end() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 81,
            end_ts: 120,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 0,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 120, // consider wether this should be available_event.end_ts or bookingoptions.end_ts
                duration: 10,
                start: 90
            }
        );
    }

    #[test]
    fn slot_that_crosses_start() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 30,
        };

        let slots = get_booking_slots(
            &CompatibleInstances::new(vec![e1]),
            &BookingSlotsOptions {
                start_ts: 10,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 2);
        assert_eq!(
            slots[0],
            BookingSlot {
                available_until: 30,
                duration: 10,
                start: 10
            }
        );
        assert_eq!(
            slots[1],
            BookingSlot {
                available_until: 30,
                duration: 10,
                start: 20
            }
        );
    }

    #[test]
    fn generate_service_bookingslots_with_one_user_in_service() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 30,
        };

        let user_id = ID::default();

        let mut users_free = vec![];
        users_free.push(UserFreeEvents {
            free_events: CompatibleInstances::new(vec![e1]),
            user_id: user_id.clone(),
        });

        let slots = get_service_bookingslots(
            users_free,
            &BookingSlotsOptions {
                start_ts: 10,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );

        assert_eq!(slots.len(), 2);
        assert_eq!(
            slots[0],
            ServiceBookingSlot {
                duration: 10,
                start: 10,
                user_ids: vec![user_id.clone()]
            }
        );
        assert_eq!(
            slots[1],
            ServiceBookingSlot {
                duration: 10,
                start: 20,
                user_ids: vec![user_id.clone()]
            }
        );
    }

    #[test]
    fn generate_service_bookingslots_with_two_users_in_service() {
        let e1 = EventInstance {
            busy: false,
            start_ts: 2,
            end_ts: 30,
        };

        let e2 = EventInstance {
            busy: false,
            start_ts: 33,
            end_ts: 52,
        };

        let user_id_1 = ID::default();
        let user_id_2 = ID::default();
        let mut users_free = vec![];
        users_free.push(UserFreeEvents {
            free_events: CompatibleInstances::new(vec![e1.clone()]),
            user_id: user_id_1.clone(),
        });
        users_free.push(UserFreeEvents {
            free_events: CompatibleInstances::new(vec![e1, e2]),
            user_id: user_id_2.clone(),
        });

        let slots = get_service_bookingslots(
            users_free,
            &BookingSlotsOptions {
                start_ts: 10,
                end_ts: 100,
                duration: 10,
                interval: 10,
            },
        );
        assert_eq!(slots.len(), 3);
        assert_eq!(
            slots[0],
            ServiceBookingSlot {
                duration: 10,
                start: 10,
                user_ids: vec![user_id_1.clone(), user_id_2.clone()]
            }
        );
        assert_eq!(
            slots[1],
            ServiceBookingSlot {
                duration: 10,
                start: 20,
                user_ids: vec![user_id_1.clone(), user_id_2.clone()]
            }
        );
        assert_eq!(
            slots[2],
            ServiceBookingSlot {
                duration: 10,
                start: 40,
                user_ids: vec![user_id_2.clone()]
            }
        );
    }
}
