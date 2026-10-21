/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSCalendar`.
//!
//! Only the Gregorian calendar is supported, and all calculations are done
//! with plain integer arithmetic, so no date/time library is needed.
//! The time zone is only consulted via `-[NSTimeZone secondsFromGMT]`
//! (currently always 0, see `ns_time_zone.rs`).

use super::ns_date_components::NS_UNDEFINED_DATE_COMPONENT;
use super::ns_string::{get_static_str, to_rust_string};
use super::{NSInteger, NSTimeInterval, NSUInteger};
use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::core_foundation::time::SECS_FROM_UNIX_TO_APPLE_EPOCHS;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;

const NSGregorianCalendar: &str = "gregorian";

pub const CONSTANTS: ConstantExports = &[(
    "_NSGregorianCalendar",
    HostConstant::NSString(NSGregorianCalendar),
)];

type NSCalendarUnit = NSUInteger;
const NSEraCalendarUnit: NSCalendarUnit = 1 << 1;
const NSYearCalendarUnit: NSCalendarUnit = 1 << 2;
const NSMonthCalendarUnit: NSCalendarUnit = 1 << 3;
const NSDayCalendarUnit: NSCalendarUnit = 1 << 4;
const NSHourCalendarUnit: NSCalendarUnit = 1 << 5;
const NSMinuteCalendarUnit: NSCalendarUnit = 1 << 6;
const NSSecondCalendarUnit: NSCalendarUnit = 1 << 7;
const NSWeekCalendarUnit: NSCalendarUnit = 1 << 8;
const NSWeekdayCalendarUnit: NSCalendarUnit = 1 << 9;
const NSWeekdayOrdinalCalendarUnit: NSCalendarUnit = 1 << 10;

struct NSCalendarHostObject {
    /// `NSString*`
    identifier: id,
    /// `NSTimeZone*`
    time_zone: id,
    /// `NSLocale*`
    locale: id,
    /// 1 = Sunday
    first_weekday: NSUInteger,
    minimum_days_in_first_week: NSUInteger,
}
impl HostObject for NSCalendarHostObject {}

// Calendar arithmetic. These are the well-known "civil from days" algorithms
// by Howard Hinnant, using the Unix epoch (1970-01-01) as day 0.

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Returns (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
    }
}

struct BrokenDown {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    /// Days since 1970-01-01
    days: i64,
}

impl BrokenDown {
    fn seconds_of_day(&self) -> i64 {
        self.hour * 3600 + self.minute * 60 + self.second
    }
    /// 1 = Sunday
    fn weekday(&self) -> i64 {
        // 1970-01-01 was a Thursday (5)
        (self.days + 4).rem_euclid(7) + 1
    }
}

/// Split seconds since the Unix epoch (in the calendar's local time) into
/// calendar fields.
fn break_down(local_secs: i64) -> BrokenDown {
    let days = local_secs.div_euclid(86400);
    let rem = local_secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    BrokenDown {
        year,
        month,
        day,
        hour: rem / 3600,
        minute: (rem % 3600) / 60,
        second: rem % 60,
        days,
    }
}

/// Combine calendar fields into seconds since the Unix epoch (in the
/// calendar's local time). Out-of-range fields overflow into the next larger
/// unit, like in Cocoa.
fn compose(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    let m0 = month - 1;
    let y = year + m0.div_euclid(12);
    let m = m0.rem_euclid(12) + 1;
    (days_from_civil(y, m, 1) + (day - 1)) * 86400 + hour * 3600 + minute * 60 + second
}

/// Add a number of months, clamping the day to the length of the target month
/// (so Jan 31 + 1 month = Feb 28/29).
fn add_months(from: &BrokenDown, months: i64) -> i64 {
    let total = from.year * 12 + (from.month - 1) + months;
    let year = total.div_euclid(12);
    let month = total.rem_euclid(12) + 1;
    let day = from.day.min(days_in_month(year, month));
    days_from_civil(year, month, day) * 86400 + from.seconds_of_day()
}

/// Week of the year, assuming the first week is the one containing Jan 1st.
// TODO: respect minimumDaysInFirstWeek
fn week_of_year(bd: &BrokenDown, first_weekday: i64) -> i64 {
    let jan1 = days_from_civil(bd.year, 1, 1);
    let day_of_year0 = bd.days - jan1;
    let jan1_weekday0 = (jan1 + 4).rem_euclid(7); // Sunday = 0
    let offset = (jan1_weekday0 - (first_weekday - 1)).rem_euclid(7);
    (day_of_year0 + offset) / 7 + 1
}

/// Offset of the calendar's time zone from GMT, in seconds.
fn time_zone_offset(env: &mut Environment, calendar: id) -> i64 {
    let time_zone = env.objc.borrow::<NSCalendarHostObject>(calendar).time_zone;
    if time_zone == nil {
        return 0;
    }
    let offset: NSInteger = msg![env; time_zone secondsFromGMT];
    offset.into()
}

/// Get a date's whole seconds since the Unix epoch in the calendar's local
/// time, and the fractional part of the second.
fn date_to_local(env: &mut Environment, calendar: id, date: id) -> (i64, f64) {
    let t: NSTimeInterval = msg![env; date timeIntervalSinceReferenceDate];
    let whole = t.floor();
    let offset = time_zone_offset(env, calendar);
    let local = whole as i64 + SECS_FROM_UNIX_TO_APPLE_EPOCHS as i64 + offset;
    (local, t - whole)
}

/// Inverse of [date_to_local]. Returns an autoreleased `NSDate*`.
fn local_to_date(env: &mut Environment, calendar: id, local: i64, fraction: f64) -> id {
    let offset = time_zone_offset(env, calendar);
    let t = (local - offset - SECS_FROM_UNIX_TO_APPLE_EPOCHS as i64) as f64 + fraction;
    msg_class![env; NSDate dateWithTimeIntervalSinceReferenceDate:t]
}

/// Returns an autoreleased `NSDateComponents*` with the requested units set.
fn make_components(
    env: &mut Environment,
    calendar: id,
    local: i64,
    unit_flags: NSCalendarUnit,
) -> id {
    let first_weekday: NSUInteger = env
        .objc
        .borrow::<NSCalendarHostObject>(calendar)
        .first_weekday;
    let bd = break_down(local);
    let comps: id = msg_class![env; NSDateComponents new];
    if unit_flags & NSEraCalendarUnit != 0 {
        let era = NSInteger::from(bd.year > 0);
        () = msg![env; comps setEra:era];
    }
    if unit_flags & NSYearCalendarUnit != 0 {
        let year = if bd.year > 0 { bd.year } else { 1 - bd.year };
        () = msg![env; comps setYear:(year as NSInteger)];
    }
    if unit_flags & NSMonthCalendarUnit != 0 {
        () = msg![env; comps setMonth:(bd.month as NSInteger)];
    }
    if unit_flags & NSDayCalendarUnit != 0 {
        () = msg![env; comps setDay:(bd.day as NSInteger)];
    }
    if unit_flags & NSHourCalendarUnit != 0 {
        () = msg![env; comps setHour:(bd.hour as NSInteger)];
    }
    if unit_flags & NSMinuteCalendarUnit != 0 {
        () = msg![env; comps setMinute:(bd.minute as NSInteger)];
    }
    if unit_flags & NSSecondCalendarUnit != 0 {
        () = msg![env; comps setSecond:(bd.second as NSInteger)];
    }
    if unit_flags & NSWeekCalendarUnit != 0 {
        let week = week_of_year(&bd, first_weekday.into());
        () = msg![env; comps setWeek:(week as NSInteger)];
    }
    if unit_flags & NSWeekdayCalendarUnit != 0 {
        () = msg![env; comps setWeekday:(bd.weekday() as NSInteger)];
    }
    if unit_flags & NSWeekdayOrdinalCalendarUnit != 0 {
        let ordinal = (bd.day - 1) / 7 + 1;
        () = msg![env; comps setWeekdayOrdinal:(ordinal as NSInteger)];
    }
    autorelease(env, comps)
}

/// A component value, or `default` if it is `NSUndefinedDateComponent`.
fn defined_or(value: NSInteger, default: i64) -> i64 {
    if value == NS_UNDEFINED_DATE_COMPONENT {
        default
    } else {
        value.into()
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSCalendar: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSCalendarHostObject {
        identifier: nil,
        time_zone: nil,
        locale: nil,
        first_weekday: 1, // Sunday
        minimum_days_in_first_week: 1,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)currentCalendar {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new init];
    autorelease(env, new)
}

+ (id)autoupdatingCurrentCalendar {
    msg![env; this currentCalendar]
}

- (id)init {
    let identifier = get_static_str(env, NSGregorianCalendar);
    msg![env; this initWithCalendarIdentifier:identifier]
}

- (id)initWithCalendarIdentifier:(id)identifier { // NSString*
    if identifier != nil {
        let name = to_rust_string(env, identifier);
        if name != NSGregorianCalendar {
            // TODO: support other calendars
            log!("TODO: NSCalendar identifier {:?} is not supported, using gregorian", name);
        }
    }
    let identifier = get_static_str(env, NSGregorianCalendar);
    let time_zone: id = msg_class![env; NSTimeZone systemTimeZone];
    retain(env, time_zone);
    let host_object = env.objc.borrow_mut::<NSCalendarHostObject>(this);
    host_object.identifier = identifier;
    host_object.time_zone = time_zone;
    this
}

- (())dealloc {
    let &NSCalendarHostObject { time_zone, locale, .. } = env.objc.borrow(this);
    release(env, time_zone);
    release(env, locale);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)calendarIdentifier {
    env.objc.borrow::<NSCalendarHostObject>(this).identifier
}

- (id)timeZone {
    env.objc.borrow::<NSCalendarHostObject>(this).time_zone
}
- (())setTimeZone:(id)time_zone { // NSTimeZone*
    retain(env, time_zone);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSCalendarHostObject>(this).time_zone,
        time_zone,
    );
    release(env, old);
}

- (id)locale {
    env.objc.borrow::<NSCalendarHostObject>(this).locale
}
- (())setLocale:(id)locale { // NSLocale*
    retain(env, locale);
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<NSCalendarHostObject>(this).locale,
        locale,
    );
    release(env, old);
}

- (NSUInteger)firstWeekday {
    env.objc.borrow::<NSCalendarHostObject>(this).first_weekday
}
- (())setFirstWeekday:(NSUInteger)weekday {
    env.objc.borrow_mut::<NSCalendarHostObject>(this).first_weekday = weekday;
}

- (NSUInteger)minimumDaysInFirstWeek {
    env.objc.borrow::<NSCalendarHostObject>(this).minimum_days_in_first_week
}
- (())setMinimumDaysInFirstWeek:(NSUInteger)days {
    env.objc.borrow_mut::<NSCalendarHostObject>(this).minimum_days_in_first_week = days;
}

- (id)components:(NSCalendarUnit)unit_flags
         fromDate:(id)date { // NSDate*
    assert!(date != nil); // TODO: raise NSInvalidArgumentException
    let (local, _fraction) = date_to_local(env, this, date);
    make_components(env, this, local, unit_flags)
}

- (id)components:(NSCalendarUnit)unit_flags
         fromDate:(id)start_date // NSDate*
           toDate:(id)end_date // NSDate*
          options:(NSUInteger)_options {
    assert!(start_date != nil && end_date != nil);
    let (start, _) = date_to_local(env, this, start_date);
    let (end, _) = date_to_local(env, this, end_date);
    let (low, high, sign) = if end >= start { (start, end, 1) } else { (end, start, -1) };

    let low_bd = break_down(low);
    let high_bd = break_down(high);

    // Whole months between the two dates.
    let mut months = (high_bd.year - low_bd.year) * 12 + (high_bd.month - low_bd.month);
    if (high_bd.day, high_bd.seconds_of_day()) < (low_bd.day, low_bd.seconds_of_day()) {
        months -= 1;
    }
    months = months.max(0);

    let want_years = unit_flags & NSYearCalendarUnit != 0;
    let want_months = unit_flags & NSMonthCalendarUnit != 0;
    let mut years = 0;
    let mut months_left = 0;
    if want_years {
        years = months / 12;
        if want_months {
            months_left = months % 12;
        }
    } else if want_months {
        months_left = months;
    }
    // Whatever isn't covered by years/months is expressed in smaller units.
    let anchor = if years != 0 || months_left != 0 {
        add_months(&low_bd, years * 12 + months_left)
    } else {
        low
    };
    let mut remaining = (high - anchor).max(0);

    let comps: id = msg_class![env; NSDateComponents new];
    if want_years {
        () = msg![env; comps setYear:((sign * years) as NSInteger)];
    }
    if want_months {
        () = msg![env; comps setMonth:((sign * months_left) as NSInteger)];
    }
    if unit_flags & NSWeekCalendarUnit != 0 {
        let weeks = remaining / (7 * 86400);
        remaining %= 7 * 86400;
        () = msg![env; comps setWeek:((sign * weeks) as NSInteger)];
    }
    if unit_flags & NSDayCalendarUnit != 0 {
        let days = remaining / 86400;
        remaining %= 86400;
        () = msg![env; comps setDay:((sign * days) as NSInteger)];
    }
    if unit_flags & NSHourCalendarUnit != 0 {
        let hours = remaining / 3600;
        remaining %= 3600;
        () = msg![env; comps setHour:((sign * hours) as NSInteger)];
    }
    if unit_flags & NSMinuteCalendarUnit != 0 {
        let minutes = remaining / 60;
        remaining %= 60;
        () = msg![env; comps setMinute:((sign * minutes) as NSInteger)];
    }
    if unit_flags & NSSecondCalendarUnit != 0 {
        () = msg![env; comps setSecond:((sign * remaining) as NSInteger)];
    }
    autorelease(env, comps)
}

- (id)dateFromComponents:(id)comps { // NSDateComponents*
    assert!(comps != nil); // TODO: raise NSInvalidArgumentException
    let era: NSInteger = msg![env; comps era];
    let year: NSInteger = msg![env; comps year];
    let month: NSInteger = msg![env; comps month];
    let day: NSInteger = msg![env; comps day];
    let hour: NSInteger = msg![env; comps hour];
    let minute: NSInteger = msg![env; comps minute];
    let second: NSInteger = msg![env; comps second];

    // TODO: unspecified year/month/day should come from the reference date
    // like in Cocoa, and week/weekday components are ignored here.
    let mut year = defined_or(year, 2001);
    if defined_or(era, 1) == 0 {
        year = 1 - year; // BC
    }
    let local = compose(
        year,
        defined_or(month, 1),
        defined_or(day, 1),
        defined_or(hour, 0),
        defined_or(minute, 0),
        defined_or(second, 0),
    );
    local_to_date(env, this, local, 0.0)
}

- (id)dateByAddingComponents:(id)comps // NSDateComponents*
                       toDate:(id)date // NSDate*
                      options:(NSUInteger)_options {
    assert!(comps != nil && date != nil);
    let year: NSInteger = msg![env; comps year];
    let month: NSInteger = msg![env; comps month];
    let day: NSInteger = msg![env; comps day];
    let hour: NSInteger = msg![env; comps hour];
    let minute: NSInteger = msg![env; comps minute];
    let second: NSInteger = msg![env; comps second];
    let week: NSInteger = msg![env; comps week];

    let (local, fraction) = date_to_local(env, this, date);
    let bd = break_down(local);

    let delta_months = defined_or(year, 0) * 12 + defined_or(month, 0);
    let month_shifted = add_months(&bd, delta_months);
    let delta_secs = (defined_or(day, 0) + defined_or(week, 0) * 7) * 86400
        + defined_or(hour, 0) * 3600
        + defined_or(minute, 0) * 60
        + defined_or(second, 0);
    local_to_date(env, this, month_shifted + delta_secs, fraction)
}

@end

};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for days in [-1_000_000, -719_468, -1, 0, 1, 10_957, 11_016, 20_000, 1_000_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        // The Apple reference date: 2001-01-01 is 978307200s after the epoch.
        assert_eq!(days_from_civil(2001, 1, 1) * 86400, 978_307_200);
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        // 1970-01-01 was a Thursday.
        assert_eq!(break_down(0).weekday(), 5);
        // 2001-01-01 was a Monday.
        assert_eq!(break_down(978_307_200).weekday(), 2);
    }

    #[test]
    fn month_clamping() {
        let jan31 = break_down(compose(2001, 1, 31, 12, 0, 0));
        assert_eq!(add_months(&jan31, 1), compose(2001, 2, 28, 12, 0, 0));
        let jan31_leap = break_down(compose(2000, 1, 31, 12, 0, 0));
        assert_eq!(add_months(&jan31_leap, 1), compose(2000, 2, 29, 12, 0, 0));
        assert_eq!(add_months(&jan31, -2), compose(2000, 11, 30, 12, 0, 0));
    }

    #[test]
    fn compose_overflow() {
        assert_eq!(compose(2001, 13, 1, 0, 0, 0), compose(2002, 1, 1, 0, 0, 0));
        assert_eq!(compose(2001, 1, 1, 24, 0, 0), compose(2001, 1, 2, 0, 0, 0));
        assert_eq!(compose(2001, 0, 1, 0, 0, 0), compose(2000, 12, 1, 0, 0, 0));
    }
}
